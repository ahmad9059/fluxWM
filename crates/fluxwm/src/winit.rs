// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
use crate::Fluxwm;
use smithay::{
    backend::{
        allocator::Fourcc,
        renderer::{
            Bind, Blit, ExportMem, Offscreen, TextureFilter, TextureMapping,
            damage::OutputDamageTracker,
            gles::{GlesRenderer, GlesTexture},
        },
        winit::{self, WinitEvent, WinitGraphicsBackend},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            EventLoop, LoopHandle,
            ping::make_ping,
            timer::{TimeoutAction, Timer},
        },
        winit::{dpi::LogicalSize, window::WindowAttributes},
    },
    utils::{Rectangle, Transform},
};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

pub fn init_winit(
    event_loop: &mut EventLoop<'static, Fluxwm>,
    state: &mut Fluxwm,
) -> Result<(), Box<dyn std::error::Error>> {
    let (backend, winit) = winit::init_from_attributes::<GlesRenderer>(
        WindowAttributes::default()
            .with_surface_size(LogicalSize::new(1280.0, 800.0))
            .with_title("FluxWM (nested)")
            .with_visible(true),
    )?;
    let backend = Rc::new(RefCell::new(backend));
    let size = backend.borrow().window_size();
    // A virtual output has a nominal mode, not a measured host presentation clock.
    let mode = Mode {
        size,
        refresh: 60_000,
    };
    let output = Output::new(
        "nested-1".into(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "FluxWM".into(),
            model: "Nested".into(),
            serial_number: "virtual".into(),
        },
    );
    let _global = output.create_global::<Fluxwm>(&state.display_handle);
    // Backend framebuffer orientation belongs in the renderer, not the output protocol.
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);
    state.space.map_output(&output, (0, 0));
    state.layout.set_viewport(flux_core::Size {
        width: f64::from(size.w),
        height: f64::from(size.h),
    })?;
    let context = Rc::new(RefCell::new(RenderContext {
        backend: Rc::try_unwrap(backend)
            .map_err(|_| "backend unexpectedly shared")?
            .into_inner(),
        damage_tracker: OutputDamageTracker::new(size, 1.0, Transform::Flipped180),
        output: output.clone(),
        next_frame: Instant::now(),
        timer_pending: false,
        texture: None,
        host_ready: false,
        present_dirty: true,
    }));
    let (ping, source) = make_ping()?;
    state.redraw_ping = Some(ping);
    let redraw_context = context.clone();
    let handle = event_loop.handle().downgrade();
    event_loop
        .handle()
        .insert_source(source, move |(), _, state| {
            if let Some(handle) = handle.upgrade() {
                schedule(&redraw_context, &handle, state);
            }
        })
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    event_loop
        .handle()
        .insert_source(winit, move |event, _, state| match event {
            WinitEvent::Resized { size, .. } => {
                state.bump_sequence();
                state.render_suspended = size.w <= 0 || size.h <= 0;
                if state.render_suspended {
                    state.scene = None;
                    state.pointer_focus_dirty = true;
                }
                if !state.render_suspended {
                    output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                    let mut renderer = context.borrow_mut();
                    renderer.damage_tracker =
                        OutputDamageTracker::new(size, 1.0, Transform::Flipped180);
                    renderer.texture = None;
                    renderer.present_dirty = true;
                    if let Err(error) = state.layout.set_viewport(flux_core::Size {
                        width: f64::from(size.w),
                        height: f64::from(size.h),
                    }) {
                        state.fail(error.to_string());
                    }
                    state.relayout();
                }
            }
            WinitEvent::Focus(focused) => state.host_focus_changed(focused),
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::Redraw => {
                context.borrow_mut().host_ready = true;
                state.request_redraw();
            }
            WinitEvent::CloseRequested => state.stop(),
        })
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    state.request_redraw();
    Ok(())
}

struct RenderContext {
    backend: WinitGraphicsBackend<GlesRenderer>,
    damage_tracker: OutputDamageTracker,
    output: Output,
    next_frame: Instant,
    timer_pending: bool,
    texture: Option<GlesTexture>,
    host_ready: bool,
    present_dirty: bool,
}

// Drive the virtual output independently of the host's occlusion-throttled
// RedrawRequested callbacks. Only dirty work arms this one-shot 60 Hz timer;
// this does not claim host presentation timing or continuously poll while idle.
fn schedule(
    context: &Rc<RefCell<RenderContext>>,
    handle: &LoopHandle<'static, Fluxwm>,
    state: &mut Fluxwm,
) {
    if !state.dirty || state.render_suspended {
        return;
    }
    let mut renderer = context.borrow_mut();
    if renderer.timer_pending {
        return;
    }
    if renderer.next_frame <= Instant::now() {
        renderer.draw(state);
        return;
    }
    let deadline = renderer.next_frame;
    renderer.timer_pending = true;
    let context = context.clone();
    if let Err(error) = handle.insert_source(Timer::from_deadline(deadline), move |_, _, state| {
        let mut renderer = context.borrow_mut();
        renderer.timer_pending = false;
        if state.dirty && !state.render_suspended {
            renderer.draw(state);
        }
        TimeoutAction::Drop
    }) {
        state.fail(format!("redraw timer: {error}"));
    }
}

impl RenderContext {
    fn draw(&mut self, state: &mut Fluxwm) {
        state.stats.redraws += 1;
        state.dirty = false;
        self.next_frame = Instant::now() + Duration::from_nanos(1_000_000_000 / 60);
        let result: Result<(), Box<dyn std::error::Error>> = (|| {
            let backend = &mut self.backend;
            let size = backend.window_size();
            if size.w <= 0 || size.h <= 0 {
                state.scene = None;
                state.pointer_focus_dirty = true;
                return Ok(());
            }
            if size.w > 8192 || size.h > 8192 || i64::from(size.w) * i64::from(size.h) > 16_777_216
            {
                return Err("nested target exceeds pixel budget".into());
            }
            let age = if self.texture.is_some() { 1 } else { 0 };
            if self.texture.is_none() {
                self.texture = Some(
                    backend
                        .renderer()
                        .create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())?,
                );
            }
            let scene = crate::scene::RuntimeScene::capture(
                state,
                flux_core::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: f64::from(size.w),
                    height: f64::from(size.h),
                },
            )?;
            let damage;
            {
                let renderer = backend.renderer();
                let mut framebuffer =
                    renderer.bind(self.texture.as_mut().ok_or("missing composition target")?)?;
                let elements = scene.render_elements(renderer)?;
                let result = self.damage_tracker.render_output(
                    renderer,
                    &mut framebuffer,
                    age,
                    &elements,
                    [0.06, 0.07, 0.09, 1.0],
                )?;
                damage = result.damage.cloned();
                if state.stats.maps > 0
                    && let Some(path) = state.screenshot.take()
                {
                    let mapping = renderer.copy_framebuffer(
                        &framebuffer,
                        Rectangle::from_size((size.w, size.h).into()),
                        Fourcc::Abgr8888,
                    )?;
                    let flip = mapping.flipped();
                    let rgba = renderer.map_texture(&mapping)?.to_vec();
                    let (width, height) = (size.w as usize, size.h as usize);
                    state.screenshot_writer = Some(std::thread::spawn(move || {
                        use std::io::Write;
                        let mut file = std::io::BufWriter::new(
                            std::fs::OpenOptions::new()
                                .write(true)
                                .create_new(true)
                                .open(path)?,
                        );
                        write!(file, "P6\n{width} {height}\n255\n")?;
                        for row in 0..height {
                            let y = if flip { height - 1 - row } else { row };
                            let mut rgb = Vec::with_capacity(width * 3);
                            for pixel in rgba[y * width * 4..(y + 1) * width * 4].as_chunks::<4>().0
                            {
                                rgb.extend_from_slice(&pixel[..3]);
                            }
                            file.write_all(&rgb)?;
                        }
                        file.flush()
                    }));
                }
            }
            // Input observes exactly this successfully composed virtual scene,
            // including copied input regions, until the next composition.
            state.scene = Some(scene);
            state.pointer_focus_dirty = true;
            if damage.is_some() {
                self.present_dirty = true;
                state.stats.compositions += 1;
            }
            if self.host_ready && self.present_dirty {
                // Compose offscreen even when the host is occlusion-throttled.
                // Submit to its EGL window only on a host redraw opportunity.
                let source = backend
                    .renderer()
                    .bind(self.texture.as_mut().ok_or("missing composition target")?)?;
                {
                    let (renderer, mut destination) = backend.bind()?;
                    let rect = Rectangle::from_size(size);
                    let _blit_completion = renderer.blit(
                        &source,
                        &mut destination,
                        rect,
                        rect,
                        TextureFilter::Nearest,
                    )?;
                }
                backend.submit(Some(&[Rectangle::from_size(size)]))?;
                self.host_ready = false;
                self.present_dirty = false;
                state.stats.submissions += 1;
            } else if self.present_dirty {
                backend.window().request_redraw();
            }
            for id in state
                .scene
                .as_ref()
                .into_iter()
                .flat_map(|scene| scene.windows())
            {
                let Some(item) = state.windows.get(id) else {
                    continue;
                };
                item.window.send_frame(
                    &self.output,
                    state.start_time.elapsed(),
                    Some(Duration::ZERO),
                    |_, _| Some(self.output.clone()),
                );
                state.stats.frames_sent += 1;
            }
            Ok(())
        })();
        if let Err(error) = result {
            state.fail(format!("nested renderer: {error}"));
        }
    }
}
