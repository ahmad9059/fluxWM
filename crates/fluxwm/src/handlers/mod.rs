// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
mod compositor;
mod xdg_shell;

use crate::Fluxwm;

//
// Wl Seat
//

use smithay::input::dnd::{DndGrabHandler, GrabType, Source};
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::Serial;
use smithay::wayland::output::OutputHandler;
use smithay::wayland::pointer_constraints::PointerConstraintsHandler;
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    DataDeviceHandler, DataDeviceState, WaylandDndGrabHandler, set_data_device_focus,
};

impl SeatHandler for Fluxwm {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Fluxwm> {
        &mut self.seat_state
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        image: smithay::input::pointer::CursorImageStatus,
    ) {
        self.cursor_image = image;
        if self.backend == "native" {
            self.request_redraw();
        }
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        let dh = &self.display_handle;
        let client = focused.and_then(|s| dh.get_client(s.id()).ok());
        set_data_device_focus(dh, seat, client);
    }
}

impl PointerConstraintsHandler for Fluxwm {}

//
// Wl Data Device
//

impl SelectionHandler for Fluxwm {
    type SelectionUserData = ();
}

impl DataDeviceHandler for Fluxwm {
    fn data_device_state(&mut self) -> &mut DataDeviceState {
        &mut self.data_device_state
    }
}

impl DndGrabHandler for Fluxwm {}
impl WaylandDndGrabHandler for Fluxwm {
    fn dnd_requested<S: Source>(
        &mut self,
        source: S,
        _icon: Option<WlSurface>,
        _seat: Seat<Self>,
        _serial: Serial,
        _type_: GrabType,
    ) {
        // The pinned protocol helper checks a grab serial but this generic
        // callback does not establish source-client/active-press ownership.
        // Keep DnD unavailable until that full admission policy is implemented.
        source.cancel();
    }
}

//
// Wl Output & Xdg Output
//

impl OutputHandler for Fluxwm {}

smithay::delegate_dispatch2!(Fluxwm);
