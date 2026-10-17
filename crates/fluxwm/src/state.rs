// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
use flux_core::{Arena, Size, WindowId};
use flux_layout::{Layout, SizeConstraints};
use std::{ffi::OsString, path::PathBuf, sync::Arc};

use smithay::{
    desktop::{PopupManager, Space, Window},
    input::{Seat, SeatState},
    reexports::{
        calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction, generic::Generic},
        wayland_server::{
            Display, DisplayHandle,
            backend::{ClientData, ClientId, DisconnectReason},
            protocol::wl_surface::WlSurface,
        },
    },
    utils::{IsAlive, Logical, Point, SERIAL_COUNTER},
    wayland::{
        compositor::{CompositorClientState, CompositorState},
        output::OutputManagerState,
        selection::data_device::DataDeviceState,
        shell::xdg::{SurfaceCachedState, XdgShellState},
        shm::ShmState,
        socket::ListeningSocketSource,
    },
};

#[derive(serde::Serialize)]
pub struct LaunchOutcome {
    pub operation_id: String,
    pub pid: Option<u32>,
    pub error: Option<String>,
}

pub struct ManagedWindow {
    pub window: Window,
    /// Root-buffer presence is independent of successful layout admission.
    pub buffered: bool,
    pub mapped: bool,
    pub admission_error: Option<String>,
    pub floating: bool,
}

#[derive(Default, serde::Serialize)]
pub struct RuntimeStats {
    pub redraws: u64,
    pub submissions: u64,
    pub compositions: u64,
    pub maps: u64,
    pub unmaps: u64,
    pub destroyed: u64,
    pub frames_sent: u64,
    pub final_windows: usize,
    pub wayland_requests: u64,
    pub flush_cycles: u64,
    pub flush_would_block: u64,
    pub flush_retry_timer_fired: u64,
    pub ipc_drain_timed_out: u64,
    pub host_focus_losses: u64,
    pub focus_cancelled_keys: u64,
    pub focus_cancelled_buttons: u64,
    pub popup_grabs_denied_missing_press: u64,
    pub ipc_dispatches_after_shutdown: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc_drain_budget_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipc_drain_elapsed_ms: Option<u64>,
}

pub struct Fluxwm {
    pub config: Option<crate::config::RuntimeConfig>,
    pub stopping: bool,
    pub bindings: flux_input::BindingSet,
    pub host_focus: Option<bool>,
    pub health: crate::health::Monitor<Fluxwm>,
    pub health_requested: bool,
    pub scene: Option<crate::scene::RuntimeScene>,
    pub windows: Arena<ManagedWindow>,
    pub sequence: u64,
    flush_retry_ping: smithay::reexports::calloop::ping::Ping,
    flush_retry_pending: bool,
    pub ipc: Option<flux_ipc::EventLoopDriver<Fluxwm>>,
    pub ipc_requested: bool,
    pub shutdown_deadline: Option<std::time::Instant>,
    pub shutdown_started: Option<std::time::Instant>,
    pub popup_grab: Option<(WindowId, smithay::desktop::PopupGrab<Self>)>,
    pub pointer_presses: std::collections::BTreeMap<u32, smithay::utils::Serial>,
    pub pointer_focus_dirty: bool,
    pub pointer_scene_target: Option<(WlSurface, Point<f64, Logical>)>,
    pub suppressed_keys: std::collections::BTreeSet<u32>,
    pub spawned: Vec<flux_process::ManagedChild>,
    pub spawn_worker: Option<flux_process::SpawnWorker>,
    pub pending_spawns: usize,
    pub launch_outcomes: std::collections::VecDeque<LaunchOutcome>,
    pub ipc_socket: Option<PathBuf>,
    pub layout: Layout,
    pub redraw_ping: Option<smithay::reexports::calloop::ping::Ping>,
    pub dirty: bool,
    pub render_suspended: bool,
    pub stats: RuntimeStats,
    pub screenshot: Option<PathBuf>,
    pub screenshot_writer: Option<std::thread::JoinHandle<std::io::Result<()>>>,
    pub fatal_error: Option<String>,
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,

    // Smithay State
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub _output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Fluxwm>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Self>,
}

impl Fluxwm {
    pub fn new(
        event_loop: &mut EventLoop<'static, Self>,
        display: Display<Self>,
        socket: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let start_time = std::time::Instant::now();
        let (flush_retry_ping, source) = smithay::reexports::calloop::ping::make_ping()?;
        let weak = event_loop.handle().downgrade();
        event_loop
            .handle()
            .insert_source(source, move |(), _, state| {
                let Some(handle) = weak.upgrade() else {
                    return;
                };
                use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
                if let Err(error) = handle.insert_source(
                    Timer::from_duration(std::time::Duration::from_millis(16)),
                    |_, _, state: &mut Self| {
                        state.stats.flush_retry_timer_fired += 1;
                        state.flush_retry_pending = false;
                        state.flush_wayland();
                        TimeoutAction::Drop
                    },
                ) {
                    state.fail(format!("Wayland flush retry registration: {error}"));
                }
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        let dh = display.handle();

        // Here we initialize implementations of some wayland protocols
        // Some of them require us to implement traits on the Fluxwm state,
        // you can find those implementations in the `crate::handlers` module

        // Initialize protocols needed for displaying windows
        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new_with_capabilities::<Self>(&dh, Vec::new());
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let popups = PopupManager::default();

        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);

        // Data device is responsible for clipboard and drag-and-drop
        let data_device_state = DataDeviceState::new::<Self>(&dh);

        // A seat is a group of keyboards, pointer and touch devices.
        // A seat typically has a pointer and maintains a keyboard focus and a pointer focus.
        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "winit");

        // Notify clients that we have a keyboard, for the sake of the example we assume that keyboard is always present.
        // You may want to track keyboard hot-plug in real compositor.
        seat.add_keyboard(Default::default(), 200, 25)?;

        // Notify clients that we have a pointer (mouse)
        // Here we assume that there is always pointer plugged in
        seat.add_pointer();

        // A space represents a two-dimensional plane. Windows and Outputs can be mapped onto it.
        //
        // Windows get a position and stacking order through mapping.
        // Outputs become views of a part of the Space and can be rendered via Space::render_output.
        let space = Space::default();

        // Setup a wayland socket that will be used to accept clients
        let socket_name = Self::init_wayland_listener(display, event_loop, socket)?;

        // Get the loop signal, used to stop the event loop
        let loop_signal = event_loop.get_signal();

        let mut state = Self {
            config: None,
            stopping: false,
            bindings: Default::default(),
            host_focus: None,
            health: crate::health::Monitor::new(&event_loop.handle(), |state| {
                state.health_requested = true
            }),
            health_requested: false,
            scene: None,
            windows: Arena::new(),
            sequence: 0,
            flush_retry_ping,
            flush_retry_pending: false,
            ipc: None,
            ipc_requested: true,
            shutdown_deadline: None,
            shutdown_started: None,
            popup_grab: None,
            pointer_presses: Default::default(),
            pointer_focus_dirty: true,
            pointer_scene_target: None,
            suppressed_keys: Default::default(),
            spawned: Vec::new(),
            spawn_worker: None,
            pending_spawns: 0,
            launch_outcomes: Default::default(),
            ipc_socket: None,
            layout: Layout::new(Size {
                width: 1280.0,
                height: 800.0,
            })?,
            redraw_ping: None,
            dirty: true,
            render_suspended: false,
            stats: RuntimeStats::default(),
            screenshot: None,
            screenshot_writer: None,
            fatal_error: None,
            start_time,
            display_handle: dh,

            space,
            loop_signal,
            socket_name,

            compositor_state,
            xdg_shell_state,
            shm_state,
            _output_manager_state: output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
        };
        let keyboard = state.seat.get_keyboard().expect("keyboard added above");
        let defaults = crate::input::development_bindings();
        state.bindings = keyboard.with_xkb_state(&mut state, |context| {
            let xkb = context.xkb().lock().unwrap();
            // SAFETY: compile only borrows the keymap while this Xkb guard is
            // held. It retains pure Rust tables, no native object or refcount.
            flux_input::BindingSet::compile(
                &defaults,
                unsafe { xkb.keymap() },
                flux_input::UnmappedPolicy::RetainInactive,
            )
        })?;
        if !state.bindings.inactive_indices().is_empty() {
            tracing::info!(
                inactive = state.bindings.inactive_indices().len(),
                "development bindings absent from selected keymap"
            );
        }
        Ok(state)
    }

    fn init_wayland_listener(
        display: Display<Fluxwm>,
        event_loop: &mut EventLoop<'static, Self>,
        socket: &str,
    ) -> Result<OsString, Box<dyn std::error::Error>> {
        // The upstream helper unlinks preexisting endpoints. Refuse collisions,
        // including dangling symlinks, before calling it. This is not protection
        // against a same-user process racing path replacement.
        let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR missing")?;
        for path in [
            PathBuf::from(&runtime).join(socket),
            PathBuf::from(&runtime).join(format!("{socket}.lock")),
        ] {
            match std::fs::symlink_metadata(&path) {
                Ok(_) => {
                    return Err(
                        format!("Wayland socket path already exists: {}", path.display()).into(),
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        let listening_socket = ListeningSocketSource::with_name(socket)?;

        // Get the name of the listening socket.
        // Clients will connect to this socket.
        let socket_name = listening_socket.socket_name().to_os_string();

        let loop_handle = event_loop.handle();

        loop_handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                // Inside the callback, you should insert the client into the display.
                //
                // You may also associate some data with the client when inserting the client.
                if let Err(error) = state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                {
                    tracing::warn!(%error, "client admission failed");
                }
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        // You also need to add the display itself to the event loop, so that client events will be processed by wayland-server.
        loop_handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    // Safety: we don't drop the display
                    unsafe {
                        match display.get_mut().dispatch_clients(state) {
                            Ok(requests) => state.stats.wayland_requests += requests as u64,
                            Err(error) => state.fail(format!("Wayland dispatch failed: {error}")),
                        }
                    }
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|e| std::io::Error::other(e.to_string()))?;

        Ok(socket_name)
    }

    pub fn bump_sequence(&mut self) {
        if let Some(sequence) = self.sequence.checked_add(1) {
            self.sequence = sequence;
        } else {
            self.fail("state sequence exhausted".into());
        }
    }

    pub fn request_redraw(&mut self) {
        self.pointer_focus_dirty = true;
        self.dirty = true;
        if let Some(ping) = &self.redraw_ping {
            ping.ping();
        }
    }

    /// Seal runtime publication before requesting event-loop termination.
    pub fn stop(&mut self) {
        self.stopping = true;
        self.loop_signal.stop();
    }

    pub fn fail(&mut self, message: String) {
        tracing::error!(%message);
        self.fatal_error = Some(message);
        self.stop();
    }

    pub fn window_id(&self, surface: &WlSurface) -> Option<WindowId> {
        self.windows.iter().find_map(|(id, item)| {
            item.window
                .toplevel()
                .filter(|t| t.wl_surface() == surface)
                .map(|_| id)
        })
    }

    pub fn add_toplevel(&mut self, surface: smithay::wayland::shell::xdg::ToplevelSurface) {
        if self.windows.len() >= 1024 {
            use smithay::reexports::wayland_server::Resource;
            tracing::warn!("toplevel admission limit reached; disconnecting owning client");
            if let Some(client) = surface.wl_surface().client() {
                self.display_handle
                    .backend_handle()
                    .kill_client(client.id(), DisconnectReason::ConnectionClosed);
            }
            return;
        }
        let window = Window::new_wayland_window(surface);
        match self.windows.insert(ManagedWindow {
            window,
            buffered: false,
            mapped: false,
            admission_error: None,
            floating: false,
        }) {
            Ok(id) => {
                self.bump_sequence();
                tracing::debug!(%id, "toplevel_created");
            }
            Err(error) => self.fail(format!("window allocation failed: {}", error.cause)),
        }
    }

    /// A popup may be tracked only while its managed root belongs to the scene.
    /// This is scene membership, not proof of output visibility or clipping.
    pub(crate) fn popup_root_visible(&self, surface: &WlSurface) -> bool {
        self.window_id(surface)
            .and_then(|id| self.windows.get(id))
            .is_some_and(|item| item.mapped && self.space.element_location(&item.window).is_some())
    }

    /// PopupManager owns non-grabbing trees independently of the seat's active
    /// PopupGrab. Detach every subtree when its root leaves the scene so a later
    /// remap cannot resurrect old popups. Collect before mutating the tree;
    /// dismiss_popup sends popup_done to descendants before their parent.
    fn dismiss_root_popups(surface: &WlSurface) {
        if !surface.alive() {
            return;
        }
        let popups: Vec<_> = PopupManager::popups_for_surface(surface)
            .map(|(popup, _)| popup)
            .collect();
        for popup in popups {
            // An earlier ancestor dismissal may already have detached this node.
            // The pinned manager treats an absent subtree as a no-op.
            let _ = PopupManager::dismiss_popup(surface, &popup);
        }
    }

    pub fn remove_toplevel(&mut self, surface: &WlSurface) {
        let Some(id) = self.window_id(surface) else {
            return;
        };
        Self::dismiss_root_popups(surface);
        if let Some(item) = self.windows.remove(id) {
            self.space.unmap_elem(&item.window);
            if item.mapped {
                let _ = self.layout.remove(id);
            }
            self.stats.destroyed += 1;
            self.bump_sequence();
            tracing::info!(%id, "toplevel_destroyed");
            self.relayout();
            self.restore_keyboard_focus();
        }
    }

    /// Called after Smithay has applied buffer/subsurface commit state.
    pub fn commit_window(&mut self, root: &WlSurface) {
        let Some(id) = self.window_id(root) else {
            return;
        };
        let has_buffer =
            smithay::backend::renderer::utils::with_renderer_surface_state(root, |s| {
                s.buffer().is_some()
            })
            .unwrap_or(false);
        let item = self
            .windows
            .get_mut(id)
            .expect("live lookup within one dispatch");
        let was_buffered = item.buffered;
        item.buffered = has_buffer;
        if !has_buffer {
            item.admission_error = None;
        }
        item.window.on_commit();
        let was_mapped = item.mapped;
        let window = item.window.clone();
        if has_buffer && !was_mapped {
            let constraints = self.constraints_for(root);
            let activate = !self.seat.get_keyboard().is_some_and(|k| k.is_grabbed());
            if let Err(error) = self.layout.insert_with_focus(id, constraints, activate) {
                self.windows
                    .get_mut(id)
                    .expect("live dispatch ID")
                    .admission_error = Some(error.to_string());
                self.bump_sequence();
                tracing::warn!(%id, %error, "layout admission rejected");
                return;
            }
            let item = self.windows.get_mut(id).expect("live dispatch ID");
            item.mapped = true;
            item.admission_error = None;
            self.stats.maps += 1;
            self.bump_sequence();
            tracing::info!(%id, "window_mapped");
            self.relayout();
            if activate {
                self.restore_keyboard_focus();
            }
        } else if !has_buffer && was_mapped {
            self.windows.get_mut(id).expect("live dispatch ID").mapped = false;
            Self::dismiss_root_popups(root);
            self.space.unmap_elem(&window);
            let _ = self.layout.remove(id);
            self.stats.unmaps += 1;
            self.bump_sequence();
            tracing::info!(%id, "window_unmapped");
            self.relayout();
            self.restore_keyboard_focus();
        } else if has_buffer {
            // A committed geometry/constraint change is observable through IPC.
            self.bump_sequence();
            if let Err(error) = self.layout.set_constraints(id, self.constraints_for(root)) {
                tracing::debug!(%id, %error, "constraint update rejected");
            }
            self.relayout();
        } else if was_buffered {
            // A rejected admission can lose its buffer without ever entering
            // the layout. Its buffer/error diagnostics still changed.
            self.bump_sequence();
        }
        self.request_redraw();
    }

    fn constraints_for(&self, surface: &WlSurface) -> SizeConstraints {
        smithay::wayland::compositor::with_states(surface, |states| {
            let mut cached = states.cached_state.get::<SurfaceCachedState>();
            let data = cached.current();
            SizeConstraints::from_xdg(
                (data.min_size.w, data.min_size.h),
                (data.max_size.w, data.max_size.h),
            )
            .unwrap_or_default()
        })
    }

    pub fn relayout(&mut self) {
        // Capacity depends on the other members and viewport, not just the
        // rejected client's hints. Retry retained buffers on model changes so
        // a static client does not need to commit again after capacity returns.
        // One arena-order pass (at most the 1,024 owned roles), no timer or
        // recursive relayout. Recovery preserves the current selection.
        let pending: Vec<_> = self
            .windows
            .iter()
            .filter_map(|(id, item)| {
                (item.buffered
                    && !item.mapped
                    && item.admission_error.is_some()
                    && item.window.alive())
                .then_some(id)
            })
            .collect();
        let mut admitted = false;
        for id in pending {
            let surface = self
                .windows
                .get(id)
                .and_then(|item| item.window.toplevel())
                .expect("live rejected role in one dispatch")
                .wl_surface()
                .clone();
            if self
                .layout
                .insert_with_focus(id, self.constraints_for(&surface), false)
                .is_ok()
            {
                let item = self.windows.get_mut(id).expect("live dispatch ID");
                item.mapped = true;
                item.admission_error = None;
                self.stats.maps += 1;
                self.bump_sequence();
                admitted = true;
                tracing::info!(%id, "window_admission_recovered");
            }
        }
        for placement in self.layout.placements() {
            let Some(item) = self.windows.get(placement.window) else {
                continue;
            };
            if !item.mapped {
                continue;
            }
            if let Some(toplevel) = item.window.toplevel() {
                toplevel.with_pending_state(|s| {
                    s.size = Some((placement.rect.width.round() as i32, placement.rect.height.round() as i32).into());
                    if placement.focused { s.states.set(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated); }
                    else { s.states.unset(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Activated); }
                });
                crate::configure::send_pending(toplevel);
            }
            if placement.visible {
                let rect = self.layout.camera().rect_to_view(placement.rect);
                self.space.map_element(
                    item.window.clone(),
                    (rect.x.round() as i32, rect.y.round() as i32),
                    placement.focused,
                );
            } else {
                if let Some(toplevel) = item.window.toplevel() {
                    Self::dismiss_root_popups(toplevel.wl_surface());
                }
                self.space.unmap_elem(&item.window);
            }
        }
        // Release seat wrappers only after all scene removals are authoritative;
        // cleanup repairs keyboard/pointer focus using the completed layout.
        self.cleanup_popup_grab();
        if admitted {
            self.restore_keyboard_focus();
        }
        self.request_redraw();
    }

    pub fn focus_window_id(&mut self, id: WindowId) {
        if self.seat.get_keyboard().is_some_and(|k| k.is_grabbed()) {
            return;
        }
        if self.layout.focused() == Some(id) {
            // Reassert keyboard ownership without publishing an unchanged scene.
            self.restore_keyboard_focus();
            return;
        }
        if self.layout.focus_window(id).is_ok() {
            self.relayout();
            self.restore_keyboard_focus();
            self.bump_sequence();
        }
    }

    pub fn restore_keyboard_focus(&mut self) {
        let Some(keyboard) = self.seat.get_keyboard() else {
            return;
        };
        if keyboard.is_grabbed() {
            return;
        }
        let surface = self
            .layout
            .focused()
            .and_then(|id| self.windows.get(id))
            .filter(|item| item.mapped)
            .and_then(|item| item.window.toplevel())
            .map(|t| t.wl_surface().clone());
        keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
    }

    pub fn init_processes(
        &mut self,
        handle: &calloop::LoopHandle<'static, Self>,
    ) -> std::io::Result<()> {
        let (worker, completions) = flux_process::SpawnWorker::new()?;
        handle.insert_source(completions, |event, _, state| match event {
            calloop::channel::Event::Msg(outcome) => {
                state.pending_spawns = state.pending_spawns.saturating_sub(1);
                if state.launch_outcomes.len() == 64 { state.launch_outcomes.pop_front(); }
                let (pid, error) = match &outcome.result {
                    Ok(child) => (Some(child.id()), None),
                    Err(error) => {
                        let mut end = error.len().min(1024);
                        while !error.is_char_boundary(end) { end -= 1; }
                        (None, Some(error[..end].to_owned()))
                    }
                };
                state.launch_outcomes.push_back(LaunchOutcome {
                    operation_id: outcome.operation.to_string(), pid, error,
                });
                state.bump_sequence();
                match outcome.result {
                    Ok(mut child) => {
                        // A fast process may exit before its completion arrives;
                        // the earlier SIGCHLD alone is insufficient to reap it.
                        match child.try_wait() {
                            Ok(Some(status)) => tracing::info!(operation=outcome.operation, %status, "spawned application exited"),
                            Ok(None) => state.spawned.push(child),
                            Err(error) => {
                                // Keep ownership until shutdown, outside dispatch.
                                state.spawned.push(child);
                                state.fail(format!("spawn completion reaping failed: {error}"));
                            }
                        }
                    }
                    Err(error) => tracing::warn!(operation=outcome.operation, %error, "application launch failed"),
                }
            }
            calloop::channel::Event::Closed => state.fail("application launch worker stopped".into()),
        }).map_err(|e| std::io::Error::other(e.to_string()))?;
        self.spawn_worker = Some(worker);
        Ok(())
    }

    fn reap_children(&mut self) {
        let mut i = 0;
        while i < self.spawned.len() {
            match self.spawned[i].try_wait() {
                Ok(Some(status)) => {
                    tracing::info!(pid=self.spawned[i].id(), %status, "application exited");
                    self.spawned.swap_remove(i);
                }
                Ok(None) => i += 1,
                Err(error) => {
                    self.fail(format!("application reaping failed: {error}"));
                    break;
                }
            }
        }
    }

    pub fn cleanup_popup_grab(&mut self) {
        let Some((root, mut grab)) = self.popup_grab.take() else {
            return;
        };
        let visible = self.windows.get(root).is_some_and(|item| item.mapped)
            && self
                .layout
                .placements()
                .iter()
                .any(|p| p.window == root && p.visible);
        if !visible {
            grab.ungrab(smithay::desktop::PopupUngrabStrategy::All);
        }
        if !grab.has_ended() {
            // Destroying a nested child may leave its parent grab alive. Repair
            // keyboard focus now, without waiting for another physical event.
            if let Some(keyboard) = self.seat.get_keyboard()
                && keyboard
                    .with_grab(|serial, active| {
                        (serial == grab.serial() || Some(serial) == grab.previous_serial())
                            && active.is::<smithay::desktop::PopupKeyboardGrab<Self>>()
                    })
                    .unwrap_or(false)
                && keyboard.current_focus() != grab.current_grab()
            {
                keyboard.set_focus(self, grab.current_grab(), SERIAL_COUNTER.next_serial());
            }
            self.popup_grab = Some((root, grab));
            return;
        }
        // Release only wrappers installed by this tracked popup chain. Another
        // subsystem's grab must not be cancelled merely because this chain ended.
        let matches = |serial| serial == grab.serial() || Some(serial) == grab.previous_serial();
        if let Some(keyboard) = self.seat.get_keyboard()
            && keyboard
                .with_grab(|serial, active| {
                    matches(serial) && active.is::<smithay::desktop::PopupKeyboardGrab<Self>>()
                })
                .unwrap_or(false)
        {
            keyboard.unset_grab(self);
        }
        if let Some(pointer) = self.seat.get_pointer()
            && pointer
                .with_grab(|serial, active| {
                    matches(serial) && active.is::<smithay::desktop::PopupPointerGrab<Self>>()
                })
                .unwrap_or(false)
        {
            // Update pending focus before the ended popup wrapper restores it.
            let location = pointer.current_location();
            let under = self.surface_under(location);
            self.pointer_scene_target = under.clone();
            let serial = SERIAL_COUNTER.next_serial();
            let time = smithay::backend::input::InputTime::now();
            pointer.motion(
                self,
                under,
                &smithay::input::pointer::MotionEvent {
                    location,
                    serial,
                    time,
                },
            );
            if pointer
                .with_grab(|serial, active| {
                    matches(serial) && active.is::<smithay::desktop::PopupPointerGrab<Self>>()
                })
                .unwrap_or(false)
            {
                pointer.unset_grab(self, serial, time);
            }
            pointer.frame(self);
        }
        // A dismissed-but-alive popup must disappear without waiting for the
        // client to destroy its role or for unrelated damage to trigger a frame.
        self.request_redraw();
        self.restore_keyboard_focus();
        self.bump_sequence();
    }

    pub fn finish_dispatch(&mut self) {
        let dead: Vec<_> = self
            .windows
            .iter()
            .filter(|(_, item)| !item.window.alive())
            .map(|(id, _)| id)
            .collect();
        for id in dead {
            if let Some(surface) = self
                .windows
                .get(id)
                .and_then(|item| item.window.toplevel())
                .map(|toplevel| toplevel.wl_surface().clone())
            {
                Self::dismiss_root_popups(&surface);
            }
            if let Some(item) = self.windows.remove(id) {
                self.space.unmap_elem(&item.window);
                if item.mapped {
                    let _ = self.layout.remove(id);
                }
                self.stats.destroyed += 1;
                self.bump_sequence();
                self.relayout();
                self.restore_keyboard_focus();
            }
        }
        self.reap_children();
        self.space.refresh();
        self.popups.cleanup();
        self.cleanup_popup_grab();
        if std::mem::take(&mut self.health_requested) || self.shutdown_deadline.is_some() {
            match self
                .health
                .poll(std::time::Instant::now(), self.shutdown_deadline.is_some())
            {
                Ok(true) => self.bump_sequence(),
                Ok(false) => (),
                Err(error) => self.fail(format!("shell health timer: {error}")),
            }
        }
        self.drive_ipc();
        self.cleanup_popup_grab();
        self.drive_config();
        self.refresh_pointer_focus();
        self.flush_wayland();
    }

    fn flush_wayland(&mut self) {
        self.stats.flush_cycles += 1;
        // flush(None) hides individual clients' WouldBlock in the pinned Rust
        // backend. Collect IDs outside the backend lock, then flush separately.
        let mut backend = self.display_handle.backend_handle();
        let mut clients = Vec::new();
        backend.with_all_clients(|id| clients.push(id));
        let mut blocked = false;
        for client in clients {
            match backend.flush(Some(client)) {
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    self.stats.flush_would_block += 1;
                    blocked = true;
                }
                Err(error) => tracing::debug!(%error, "client flush error"),
                Ok(()) => (),
            }
        }
        // At most one retry timer; no recurring idle timer after output drains.
        // This services slow readers independently of scene redraws/input.
        if blocked && !self.flush_retry_pending {
            self.flush_retry_pending = true;
            self.flush_retry_ping.ping();
        }
    }

    pub fn surface_under(
        &self,
        pos: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        self.scene
            .as_ref()?
            .hit(pos, self)
            .map(|(_, surface, origin)| (surface, origin))
    }
}

/// Data associated with a wayland client that connects to Fluxwm.
/// One instance of this type per client.
#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}
