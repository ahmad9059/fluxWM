// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
use smithay::{
    desktop::{
        PopupKeyboardGrab, PopupKind, PopupPointerGrab, PopupUngrabStrategy,
        find_popup_root_surface, get_popup_toplevel_coords,
    },
    input::{
        Seat,
        pointer::{Focus, GrabStartData as PointerGrabStartData},
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::{
            Resource,
            protocol::{wl_seat, wl_surface::WlSurface},
        },
    },
    utils::{Rectangle, Serial},
    wayland::{
        compositor::with_states,
        shell::xdg::{
            PopupSurface, PositionerState, ShellClient, ToplevelSurface, XdgShellHandler,
            XdgShellState, XdgToplevelSurfaceData,
        },
    },
};

use crate::{
    Fluxwm,
    grabs::{MoveSurfaceGrab, ResizeSurfaceGrab},
};

impl XdgShellHandler for Fluxwm {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.xdg_shell_state
    }

    fn new_client(&mut self, client: ShellClient) {
        self.health.add(client, std::time::Instant::now());
        self.health_requested = true;
    }
    fn client_pong(&mut self, client: ShellClient) {
        // Pinned Smithay invokes this only for its matching pending serial.
        self.health.pong(&client, std::time::Instant::now());
        self.health_requested = true;
    }
    fn client_destroyed(&mut self, client: ShellClient) {
        self.health.remove(&client);
        self.health_requested = true;
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        self.add_toplevel(surface);
    }

    fn new_popup(&mut self, surface: PopupSurface, _positioner: PositionerState) {
        // Creating a fresh popup after its root left the scene must not recreate
        // a tree that was dismissed by an earlier unmap/hide transaction.
        let visible = find_popup_root_surface(&PopupKind::Xdg(surface.clone()))
            .is_ok_and(|root| self.popup_root_visible(&root));
        if !visible {
            surface.send_popup_done();
            return;
        }
        self.unconstrain_popup(&surface);
        let _ = self.popups.track_popup(PopupKind::Xdg(surface));
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            let geometry = positioner.get_geometry();
            state.geometry = geometry;
            state.positioner = positioner;
        });
        self.unconstrain_popup(&surface);
        surface.send_repositioned(token);
    }

    fn move_request(&mut self, surface: ToplevelSurface, seat: wl_seat::WlSeat, serial: Serial) {
        let Some(seat) = Seat::from_resource(&seat) else {
            return;
        };

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let Some(pointer) = seat.get_pointer() else {
                return;
            };

            let Some(id) = self.window_id(wl_surface) else {
                return;
            };
            let Some(item) = self
                .windows
                .get(id)
                .filter(|item| item.mapped && item.floating)
            else {
                return;
            };
            let window = item.window.clone();
            let Some(initial_window_location) = self.space.element_location(&window) else {
                return;
            };

            let grab = MoveSurfaceGrab {
                start_data,
                window,
                initial_window_location,
            };

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        seat: wl_seat::WlSeat,
        serial: Serial,
        edges: xdg_toplevel::ResizeEdge,
    ) {
        let Some(seat) = Seat::from_resource(&seat) else {
            return;
        };

        let wl_surface = surface.wl_surface();

        if let Some(start_data) = check_grab(&seat, wl_surface, serial) {
            let Some(pointer) = seat.get_pointer() else {
                return;
            };

            let Some(id) = self.window_id(wl_surface) else {
                return;
            };
            let Some(item) = self
                .windows
                .get(id)
                .filter(|item| item.mapped && item.floating)
            else {
                return;
            };
            let window = item.window.clone();
            let Some(initial_window_location) = self.space.element_location(&window) else {
                return;
            };
            let initial_window_size = window.geometry().size;

            surface.with_pending_state(|state| {
                state.states.set(xdg_toplevel::State::Resizing);
            });

            surface.send_pending_configure();

            let grab = ResizeSurfaceGrab::start(
                start_data,
                window,
                edges.into(),
                Rectangle::new(initial_window_location, initial_window_size),
            );

            pointer.set_grab(self, grab, serial, Focus::Clear);
        }
    }

    fn grab(&mut self, surface: PopupSurface, seat: wl_seat::WlSeat, serial: Serial) {
        self.cleanup_popup_grab();
        let Some(seat) = Seat::from_resource(&seat) else {
            surface.send_popup_done();
            return;
        };
        // Only an active same-client pointer press authorizes a new popup grab.
        // Keyboard/touch initiated grabs remain unsupported until their input
        // serial provenance is tracked explicitly.
        if !self
            .pointer_presses
            .values()
            .any(|pressed| *pressed == serial)
            || check_grab(&seat, surface.wl_surface(), serial).is_none()
        {
            surface.send_popup_done();
            return;
        }
        let kind = PopupKind::Xdg(surface.clone());
        let Ok(root) = find_popup_root_surface(&kind) else {
            surface.send_popup_done();
            return;
        };
        let Some(id) = self.window_id(&root) else {
            surface.send_popup_done();
            return;
        };
        if !self.popup_root_visible(&root) {
            surface.send_popup_done();
            return;
        }
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else {
            surface.send_popup_done();
            return;
        };
        self.popup_grab = Some((id, grab.clone()));
        if let Some(keyboard) = seat.get_keyboard() {
            if keyboard.is_grabbed()
                && !(keyboard.has_grab(serial)
                    || keyboard.has_grab(grab.previous_serial().unwrap_or(serial)))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            if pointer.is_grabbed()
                && !(pointer.has_grab(serial)
                    || pointer.has_grab(grab.previous_serial().unwrap_or_else(|| grab.serial())))
            {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }

    fn app_id_changed(&mut self, _surface: ToplevelSurface) {
        self.bump_sequence();
    }

    fn title_changed(&mut self, _surface: ToplevelSurface) {
        self.bump_sequence();
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.remove_toplevel(surface.wl_surface());
    }

    fn popup_destroyed(&mut self, _surface: PopupSurface) {
        self.popups.cleanup();
        self.cleanup_popup_grab();
        self.request_redraw();
    }
}

fn check_grab(
    seat: &Seat<Fluxwm>,
    surface: &WlSurface,
    serial: Serial,
) -> Option<PointerGrabStartData<Fluxwm>> {
    let pointer = seat.get_pointer()?;

    // Check that this surface has a click grab.
    if !pointer.has_grab(serial) {
        return None;
    }

    let start_data = pointer.grab_start_data()?;

    let (focus, _) = start_data.focus.as_ref()?;
    // If the focus was for a different surface, ignore the request.
    if !focus.id().same_client_as(&surface.id()) {
        return None;
    }

    Some(start_data)
}

/// Should be called on `WlSurface::commit`
pub fn handle_commit(state: &mut Fluxwm, surface: &WlSurface, just_unmapped: bool) {
    if let Some(id) = state.window_id(surface) {
        let initial = with_states(surface, |states| {
            states
                .data_map
                .get::<XdgToplevelSurfaceData>()
                .is_some_and(|data| !data.lock().unwrap().initial_configure_sent)
        });
        // A null-buffer unmap is not the initial bufferless commit of the
        // next mapping cycle. Wait for that subsequent commit before configuring.
        if initial
            && !just_unmapped
            && let Some(toplevel) = state
                .windows
                .get(id)
                .and_then(|item| item.window.toplevel())
        {
            toplevel.send_configure();
        }
    }
    state.popups.commit(surface);
    if let Some(PopupKind::Xdg(xdg)) = state.popups.find_popup(surface)
        && !xdg.is_initial_configure_sent()
        && let Err(error) = xdg.send_configure()
    {
        tracing::debug!(%error, "popup configure rejected");
    }
}

impl Fluxwm {
    fn unconstrain_popup(&self, popup: &PopupSurface) {
        let Ok(root) = find_popup_root_surface(&PopupKind::Xdg(popup.clone())) else {
            return;
        };
        let Some(window) = self
            .space
            .elements()
            .find(|w| w.toplevel().unwrap().wl_surface() == &root)
        else {
            return;
        };

        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let Some(output_geo) = self.space.output_geometry(output) else {
            return;
        };
        let Some(window_geo) = self.space.element_geometry(window) else {
            return;
        };

        // The target geometry for the positioner should be relative to its parent's geometry, so
        // we will compute that here.
        let mut target = output_geo;
        target.loc -= get_popup_toplevel_coords(&PopupKind::Xdg(popup.clone()));
        target.loc -= window_geo.loc;

        popup.with_pending_state(|state| {
            state.geometry = state.positioner.get_unconstrained_geometry(target);
        });
    }
}
