// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
//! Move grab is the state of a composer during which the client window is being dragged around.
//!
//! eg. Usually whenever a user clicks on the app's titlebar and starts dragging, the compositors
//! enters a MoveSurfaceGrab state.

use crate::Fluxwm;
use flux_core::{Rect, WindowId};
use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::wayland_server::{Resource, protocol::wl_surface::WlSurface},
    utils::{Logical, Point},
};

pub struct MoveSurfaceGrab {
    pub start_data: PointerGrabStartData<Fluxwm>,
    pub window_id: WindowId,
    pub surface: WlSurface,
    pub initial_rect: Rect,
    pub initial_zoom: f64,
}

impl MoveSurfaceGrab {
    pub fn window_id(&self) -> WindowId {
        self.window_id
    }
}

impl PointerGrab<Fluxwm> for MoveSurfaceGrab {
    fn motion(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        // While the grab is active, no client has pointer focus
        handle.motion(data, None, event);

        if !floating_role_live(data, self.window_id, &self.surface) {
            handle.unset_grab(self, data, event.serial, event.time, true);
            return;
        }
        let zoom = self.initial_zoom;
        let delta = event.location - self.start_data.location;
        let mut rect = self.initial_rect;
        rect.x += delta.x / zoom;
        rect.y += delta.y / zoom;
        let _ = data.set_window_floating_rect(self.window_id, rect);
    }

    fn relative_motion(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &RelativeMotionEvent,
    ) {
        handle.relative_motion(data, focus, event);
    }

    fn button(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);

        if !handle.current_pressed().contains(&self.start_data.button) {
            // No more buttons are pressed, release the grab.
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn axis(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        details: AxisFrame,
    ) {
        handle.axis(data, details)
    }

    fn frame(&mut self, data: &mut Fluxwm, handle: &mut PointerInnerHandle<'_, Fluxwm>) {
        handle.frame(data);
    }

    fn gesture_swipe_begin(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GestureSwipeBeginEvent,
    ) {
        handle.gesture_swipe_begin(data, event)
    }

    fn gesture_swipe_update(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GestureSwipeUpdateEvent,
    ) {
        handle.gesture_swipe_update(data, event)
    }

    fn gesture_swipe_end(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GestureSwipeEndEvent,
    ) {
        handle.gesture_swipe_end(data, event)
    }

    fn gesture_pinch_begin(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GesturePinchBeginEvent,
    ) {
        handle.gesture_pinch_begin(data, event)
    }

    fn gesture_pinch_update(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GesturePinchUpdateEvent,
    ) {
        handle.gesture_pinch_update(data, event)
    }

    fn gesture_pinch_end(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GesturePinchEndEvent,
    ) {
        handle.gesture_pinch_end(data, event)
    }

    fn gesture_hold_begin(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GestureHoldBeginEvent,
    ) {
        handle.gesture_hold_begin(data, event)
    }

    fn gesture_hold_end(
        &mut self,
        data: &mut Fluxwm,
        handle: &mut PointerInnerHandle<'_, Fluxwm>,
        event: &GestureHoldEndEvent,
    ) {
        handle.gesture_hold_end(data, event)
    }

    fn start_data(&self) -> &PointerGrabStartData<Fluxwm> {
        &self.start_data
    }

    fn unset(&mut self, _data: &mut Fluxwm) {}
}

/// Both arena identity and protocol role must still designate the grabbed mapping.
pub(crate) fn floating_role_live(data: &Fluxwm, id: WindowId, surface: &WlSurface) -> bool {
    surface.is_alive()
        && data.windows.get(id).is_some_and(|item| {
            item.mapped
                && item
                    .window
                    .toplevel()
                    .is_some_and(|top| top.wl_surface() == surface)
        })
        && data.layout.is_floating(id)
        && !data.layout.is_fullscreen(id)
        && !data.layout.is_maximized(id)
        && !data.layout.is_minimized(id)
}
