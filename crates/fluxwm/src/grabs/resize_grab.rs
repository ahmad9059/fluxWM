// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
//! Resize grab is the state of a composer during which the client window is being resized.
//!
//! eg. Usually whenever a user clicks on the app's border and starts dragging, the compositors
//! enters a ResizeSurfaceGrab state.

use super::move_grab::floating_role_live;
use crate::Fluxwm;
use flux_core::{Rect, WindowId};
use smithay::{
    input::pointer::{
        AxisFrame, ButtonEvent, GestureHoldBeginEvent, GestureHoldEndEvent, GesturePinchBeginEvent,
        GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
        GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData as PointerGrabStartData,
        MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
    },
    reexports::{
        wayland_protocols::xdg::shell::server::xdg_toplevel,
        wayland_server::protocol::wl_surface::WlSurface,
    },
    utils::{Logical, Point},
};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ResizeEdge: u32 {
        const TOP          = 0b0001;
        const BOTTOM       = 0b0010;
        const LEFT         = 0b0100;
        const RIGHT        = 0b1000;

        const TOP_LEFT     = Self::TOP.bits() | Self::LEFT.bits();
        const BOTTOM_LEFT  = Self::BOTTOM.bits() | Self::LEFT.bits();

        const TOP_RIGHT    = Self::TOP.bits() | Self::RIGHT.bits();
        const BOTTOM_RIGHT = Self::BOTTOM.bits() | Self::RIGHT.bits();
    }
}

impl ResizeEdge {
    pub fn from_protocol(edge: xdg_toplevel::ResizeEdge) -> Option<Self> {
        let edges = Self::from_bits(edge as u32)?;
        (!edges.is_empty()
            && !edges.contains(Self::TOP | Self::BOTTOM)
            && !edges.contains(Self::LEFT | Self::RIGHT))
        .then_some(edges)
    }
}

pub struct ResizeSurfaceGrab {
    start_data: PointerGrabStartData<Fluxwm>,
    window_id: WindowId,
    surface: WlSurface,
    edges: ResizeEdge,
    initial_rect: Rect,
    initial_zoom: f64,
}

impl ResizeSurfaceGrab {
    pub fn start(
        start_data: PointerGrabStartData<Fluxwm>,
        window_id: WindowId,
        surface: WlSurface,
        edges: ResizeEdge,
        initial_rect: Rect,
        initial_zoom: f64,
    ) -> Self {
        Self {
            start_data,
            window_id,
            surface,
            edges,
            initial_rect,
            initial_zoom,
        }
    }

    pub fn window_id(&self) -> WindowId {
        self.window_id
    }

    fn finish(&self, data: &mut Fluxwm) {
        if !floating_role_live(data, self.window_id, &self.surface) {
            return;
        }
        let Some(toplevel) = data
            .windows
            .get(self.window_id)
            .and_then(|item| item.window.toplevel())
        else {
            return;
        };
        toplevel.with_pending_state(|state| {
            state.states.unset(xdg_toplevel::State::Resizing);
        });
        crate::configure::defer_reply(toplevel);
        crate::configure::send_pending(toplevel);
    }
}

impl PointerGrab<Fluxwm> for ResizeSurfaceGrab {
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
        let mut delta = event.location - self.start_data.location;
        delta.x /= zoom;
        delta.y /= zoom;
        let Some(constraints) = data.layout.member_constraints(self.window_id) else {
            return;
        };
        let mut rect = self.initial_rect;
        if self.edges.intersects(ResizeEdge::LEFT | ResizeEdge::RIGHT) {
            let requested = rect.width
                + if self.edges.contains(ResizeEdge::LEFT) {
                    -delta.x
                } else {
                    delta.x
                };
            rect.width = requested.max(constraints.min.width).min(
                constraints
                    .max
                    .map_or(flux_layout::MAX_COORD, |max| max.width),
            );
            if self.edges.contains(ResizeEdge::LEFT) {
                rect.x = self.initial_rect.x + self.initial_rect.width - rect.width;
            }
        }
        if self.edges.intersects(ResizeEdge::TOP | ResizeEdge::BOTTOM) {
            let requested = rect.height
                + if self.edges.contains(ResizeEdge::TOP) {
                    -delta.y
                } else {
                    delta.y
                };
            rect.height = requested.max(constraints.min.height).min(
                constraints
                    .max
                    .map_or(flux_layout::MAX_COORD, |max| max.height),
            );
            if self.edges.contains(ResizeEdge::TOP) {
                rect.y = self.initial_rect.y + self.initial_rect.height - rect.height;
            }
        }
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

    fn unset(&mut self, data: &mut Fluxwm) {
        self.finish(data);
    }
}
