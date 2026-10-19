//! Explicit single-output native session. Recovery and multi-output are future work.
use crate::{Fluxwm, scene::RuntimeScene};
use flux_native::system::{Driver, NativeEvent};
use smithay::{
    backend::renderer::damage::OutputDamageTracker,
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            EventLoop, LoopHandle, RegistrationToken,
            ping::make_ping,
            timer::{TimeoutAction, Timer},
        },
        wayland_server::backend::GlobalId,
    },
    utils::Transform,
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

struct PresentedScene {
    scene: Rc<RuntimeScene>,
    cursor_surface: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
}

struct Context {
    driver: Driver<Fluxwm>,
    device: u64,
    output: Output,
    global: Option<GlobalId>,
    damage: OutputDamageTracker,
    stopped: bool,
    cursor_warning: Option<Instant>,
    watchdog: Option<RegistrationToken>,
    handle: Box<dyn Fn() -> Option<LoopHandle<'static, Fluxwm>>>,
}
pub struct Handle(Rc<RefCell<Context>>);
impl Handle {
    pub fn shutdown(&mut self, state: &mut Fluxwm) {
        self.0.borrow_mut().shutdown(state);
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        // Also run on startup errors, while the loop and its registrations live.
        let mut context = self.0.borrow_mut();
        if context.stopped {
            return;
        }
        context.stopped = true;
        context.driver.stop();
        if context.driver.output_cleanup_failures() != 0 || context.driver.close_failures() != 0 {
            tracing::error!(
                "native shutdown could not conclusively restore and retire all resources"
            );
        }
    }
}
impl Context {
    fn cancel_watchdog(&mut self) {
        if let Some(token) = self.watchdog.take()
            && let Some(handle) = (self.handle)()
        {
            handle.remove(token);
        }
    }
    fn shutdown(&mut self, state: &mut Fluxwm) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        self.cancel_watchdog();
        state.scene = None;
        state.pointer_focus_dirty = true;
        state.render_suspended = true;
        state.host_focus_changed(false);
        state.space.unmap_output(&self.output);
        if let Some(global) = self.global.take() {
            state.display_handle.remove_global::<Fluxwm>(global);
        }
        self.driver.stop();
        if self.driver.output_cleanup_failures() != 0 || self.driver.close_failures() != 0 {
            state.fail("native shutdown failed to restore or close resources".into());
        }
    }
    fn fail(&mut self, state: &mut Fluxwm, error: String) {
        self.shutdown(state);
        state.fail(error);
    }
    fn draw(&mut self, state: &mut Fluxwm) {
        if self.stopped || state.stopping || !state.dirty || state.render_suspended {
            return;
        }
        let Some(info) = self.driver.output_preview_info(self.device) else {
            self.fail(state, "native output disappeared".into());
            return;
        };
        if info.queue.pending_cookie.is_some() {
            return;
        }
        state.stats.redraws += 1;
        let damage = &mut self.damage;
        let cursor_warning = &mut self.cursor_warning;
        let result = self
            .driver
            .render_output_frame(self.device, |renderer, target, size| {
                let scene = Rc::new(
                    RuntimeScene::capture(
                        state,
                        flux_core::Rect {
                            x: 0.,
                            y: 0.,
                            width: f64::from(size.w),
                            height: f64::from(size.h),
                        },
                    )
                    .map_err(|e| e.to_string())?,
                );
                let elements = scene.render_elements(renderer).map_err(|e| e.to_string())?;
                // Every scanout target is fresh. Age zero also erases the previous
                // software cursor overlay before drawing its new position.
                let result = damage
                    .render_output(renderer, target, 0, &elements, [0.06, 0.07, 0.09, 1.])
                    .map_err(|e| e.to_string())?;
                let cursor_sync = crate::cursor::paint(
                    renderer,
                    target,
                    size,
                    &state.cursor_image,
                    state
                        .seat
                        .get_pointer()
                        .map(|p| p.current_location())
                        .unwrap_or_default(),
                    cursor_warning,
                )?;
                let cursor_surface = if cursor_sync.is_some() {
                    if let smithay::input::pointer::CursorImageStatus::Surface(surface) =
                        &state.cursor_image
                    {
                        Some(surface.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                let sync = cursor_sync.unwrap_or(result.sync);
                Ok((
                    Rc::new(PresentedScene {
                        scene,
                        cursor_surface,
                    }),
                    sync,
                ))
            });
        if let Err(error) = result {
            self.fail(state, error.to_string());
            return;
        }
        state.dirty = false;
        state.stats.compositions += 1;
        state.stats.submissions += 1;
        if self
            .driver
            .output_preview_info(self.device)
            .and_then(|i| i.queue.pending_cookie)
            .is_none()
        {
            self.fail(state, "native submission has no completion cookie".into());
        }
    }
}

// The callback owner installs the watchdog without borrowing the context twice.
fn draw(context: &Rc<RefCell<Context>>, handle: &LoopHandle<'static, Fluxwm>, state: &mut Fluxwm) {
    let before = context
        .borrow()
        .driver
        .output_preview_info(context.borrow().device)
        .and_then(|i| i.queue.pending_cookie);
    context.borrow_mut().draw(state);
    let after = context
        .borrow()
        .driver
        .output_preview_info(context.borrow().device)
        .and_then(|i| i.queue.pending_cookie);
    if before == after {
        return;
    }
    if let Some(cookie) = after {
        let weak = Rc::downgrade(context);
        match handle.insert_source(
            Timer::from_duration(Duration::from_secs(2)),
            move |_, _, state| {
                if let Some(context) = weak.upgrade() {
                    let mut context = context.borrow_mut();
                    context.watchdog = None;
                    if !context.stopped
                        && context
                            .driver
                            .output_preview_info(context.device)
                            .is_some_and(|i| i.queue.pending_cookie == Some(cookie))
                    {
                        context.fail(state, "native page flip timed out; session stopped".into());
                    }
                }
                TimeoutAction::Drop
            },
        ) {
            Ok(token) => context.borrow_mut().watchdog = Some(token),
            Err(error) => context
                .borrow_mut()
                .fail(state, format!("native frame timer: {error}")),
        }
    }
}

fn lifecycle_failure(event: &NativeEvent, selected: u64) -> Option<String> {
    match event {
        NativeEvent::SessionActive(false) => {
            Some("native seat paused; recovery is not implemented".into())
        }
        NativeEvent::GpuChanged(id) if *id == selected => {
            Some("native GPU topology changed; session stopped".into())
        }
        NativeEvent::GpuRemoved(id) if *id == selected => {
            Some("native GPU removed; session stopped".into())
        }
        NativeEvent::GpuReady(info) if info.device == selected.to_string() => {
            Some("native GPU was replaced; session stopped".into())
        }
        NativeEvent::Failure { device, error } if device.is_none() || *device == Some(selected) => {
            Some(format!("native backend: {error:?}"))
        }
        _ => None,
    }
}

pub fn init(
    event_loop: &mut EventLoop<'static, Fluxwm>,
    state: &mut Fluxwm,
    device: u64,
    connector: u32,
) -> Result<Handle, Box<dyn std::error::Error>> {
    let slot: Rc<RefCell<std::rc::Weak<RefCell<Context>>>> = Rc::default();
    let notify_slot = slot.clone();
    let init_failure: Rc<RefCell<Option<String>>> = Rc::default();
    let notify_failure = init_failure.clone();
    let driver = Driver::new(&event_loop.handle(), state, move |event, state| {
        let Some(context) = notify_slot.borrow().upgrade() else {
            // Driver initialization reports GPU/session events synchronously.
            match event {
                NativeEvent::Failure {
                    device: failed_device,
                    error,
                } if failed_device.is_none() || failed_device == Some(device) => {
                    *notify_failure.borrow_mut() =
                        Some(format!("native initialization: {error:?}"));
                }
                NativeEvent::SessionActive(false) => {
                    *notify_failure.borrow_mut() = Some("native seat is inactive".into());
                }
                NativeEvent::Failure { error, .. } => {
                    tracing::warn!(?error, "unselected native GPU initialization")
                }
                _ => (),
            }
            return;
        };
        let mut context = context.borrow_mut();
        if context.stopped || state.stopping {
            return;
        }
        if let Some(error) = lifecycle_failure(&event, context.device) {
            context.fail(state, error);
            return;
        }
        match event {
            NativeEvent::Input(event) => state.process_input_event(event),
            NativeEvent::PageFlip { device, .. } if device == context.device => {
                let Some(info) = context.driver.output_preview_info(device) else {
                    context.fail(state, "native output disappeared".into());
                    return;
                };
                if info.queue.faulted || info.queue.closed || !info.queue.active {
                    context.fail(state, "native output completion failed".into());
                    return;
                }
                if info.queue.pending_cookie.is_none() {
                    context.cancel_watchdog();
                }
                let content = context.driver.output_content::<PresentedScene>(device);
                let scene = content.as_ref().map(|content| content.scene.clone());
                let changed = match (&state.scene, &scene) {
                    (Some(old), Some(new)) => !Rc::ptr_eq(old, new),
                    (None, Some(_)) | (Some(_), None) => true,
                    _ => false,
                };
                if changed {
                    state.scene = scene;
                    state.pointer_focus_dirty = true;
                    if let Some(surface) = content
                        .as_ref()
                        .and_then(|content| content.cursor_surface.as_ref())
                    {
                        smithay::desktop::utils::send_frames_surface_tree(
                            surface,
                            &context.output,
                            state.start_time.elapsed(),
                            Some(Duration::ZERO),
                            |_, _| Some(context.output.clone()),
                        );
                    }
                    if let Some(scene) = &state.scene {
                        for id in scene.windows() {
                            if let Some(item) = state.windows.get(id) {
                                item.window.send_frame(
                                    &context.output,
                                    state.start_time.elapsed(),
                                    Some(Duration::ZERO),
                                    |_, _| Some(context.output.clone()),
                                );
                                state.stats.frames_sent += 1;
                            }
                        }
                    }
                }
                if state.dirty {
                    state.request_redraw();
                }
            }
            _ => (),
        }
    })?;
    if let Some(error) = init_failure.borrow_mut().take() {
        return Err(error.into());
    }
    let assignment = driver
        .gpu_info()
        .into_iter()
        .find(|g| g.device == device.to_string())
        .and_then(|g| g.output_plan)
        .and_then(|p| p.assignments.into_iter().find(|a| a.connector == connector))
        .ok_or("requested native output is not available in the prepared output plan")?;
    let size = (
        i32::from(assignment.mode.width),
        i32::from(assignment.mode.height),
    )
        .into();
    let mode = Mode {
        size,
        refresh: i32::try_from(assignment.refresh_millihz)?,
    };
    let output = Output::new(
        format!("drm-{device}-{connector}"),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "FluxWM".into(),
            model: "DRM".into(),
            serial_number: String::new(),
        },
    );
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.layout.set_viewport(flux_core::Size {
        width: f64::from(size.w),
        height: f64::from(size.h),
    })?;
    let (ping, source) = make_ping()?;
    let context = Rc::new(RefCell::new(Context {
        driver,
        device,
        output,
        global: None,
        damage: OutputDamageTracker::new(size, 1., Transform::Normal),
        stopped: false,
        cursor_warning: None,
        watchdog: None,
        handle: {
            let weak = event_loop.handle().downgrade();
            Box::new(move || weak.upgrade())
        },
    }));
    *slot.borrow_mut() = Rc::downgrade(&context);
    let guard = Handle(context.clone());
    let bootstrap =
        context
            .borrow_mut()
            .driver
            .begin_output_preview(device, connector, [15, 18, 23, 255]);
    match bootstrap {
        Ok(info)
            if info.last_failure.is_none()
                && info.queue.pending_cookie.is_none()
                && info.queue.front_cookie.is_some()
                && !info.queue.faulted
                && info.queue.active => {}
        result => {
            context.borrow_mut().shutdown(state);
            return Err(match result {
                Err(error) => error.to_string(),
                Ok(_) => "native output bootstrap did not complete conclusively".into(),
            }
            .into());
        }
    }
    {
        let mut context = context.borrow_mut();
        context.global = Some(
            context
                .output
                .create_global::<Fluxwm>(&state.display_handle),
        );
        state.space.map_output(&context.output, (0, 0));
    }
    state.redraw_ping = Some(ping);
    let weak_loop = event_loop.handle().downgrade();
    let redraw_context = context.clone();
    if let Err(error) = event_loop
        .handle()
        .insert_source(source, move |(), _, state| {
            if let Some(handle) = weak_loop.upgrade() {
                draw(&redraw_context, &handle, state);
            }
        })
    {
        context.borrow_mut().shutdown(state);
        return Err(std::io::Error::other(error.to_string()).into());
    }
    state.backend = "native";
    state.host_focus_changed(true);
    state.request_redraw();
    Ok(guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_changes_and_removal_have_distinct_failure_paths() {
        let changed = lifecycle_failure(&NativeEvent::GpuChanged(7), 7).unwrap();
        let removed = lifecycle_failure(&NativeEvent::GpuRemoved(7), 7).unwrap();
        assert!(changed.contains("topology changed"));
        assert!(removed.contains("removed"));
        assert_ne!(changed, removed);
        for event in [
            NativeEvent::GpuChanged(8),
            NativeEvent::GpuRemoved(8),
            NativeEvent::SessionActive(true),
        ] {
            assert!(lifecycle_failure(&event, 7).is_none());
        }
        assert!(lifecycle_failure(&NativeEvent::SessionActive(false), 7).is_some());
        let failure = |device| NativeEvent::Failure {
            device,
            error: flux_native::system::Failure {
                stage: flux_native::system::Stage::DrmEvent,
                errno: None,
            },
        };
        assert!(lifecycle_failure(&failure(None), 7).is_some());
        assert!(lifecycle_failure(&failure(Some(7)), 7).is_some());
        assert!(lifecycle_failure(&failure(Some(8)), 7).is_none());
    }
}
