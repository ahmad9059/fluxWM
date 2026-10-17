// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
        KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
    },
    input::{
        keyboard::FilterResult,
        pointer::{AxisFrame, ButtonEvent, MotionEvent},
    },
    utils::SERIAL_COUNTER,
};

use crate::state::Fluxwm;

impl Fluxwm {
    pub fn refresh_pointer_focus(&mut self) {
        if !self.pointer_focus_dirty {
            return;
        }
        let Some(pointer) = self.seat.get_pointer() else {
            return;
        };
        if pointer.is_grabbed() {
            // Keep the pending refresh until the grab ends. A composition while
            // an implicit click grab is active must not lose enter/leave repair.
            return;
        }
        self.pointer_focus_dirty = false;
        let location = pointer.current_location();
        let under = self.surface_under(location);
        if pointer.current_focus() == under.as_ref().map(|(surface, _)| surface.clone())
            && self.pointer_scene_target == under
        {
            return;
        }
        self.pointer_scene_target = under.clone();
        pointer.motion(
            self,
            under,
            &MotionEvent {
                location,
                serial: SERIAL_COUNTER.next_serial(),
                time: smithay::backend::input::InputTime::now(),
            },
        );
        pointer.frame(self);
    }

    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>) {
        match event {
            InputEvent::Keyboard { event, .. } => {
                let serial = SERIAL_COUNTER.next_serial();
                let time = Event::time(&event);

                let code = event.key_code();
                let pressed = event.state() == KeyState::Pressed;
                let Some(keyboard) = self.seat.get_keyboard() else {
                    return;
                };
                let action = keyboard
                    .input::<Option<flux_core::Action>, _>(
                        self,
                        code,
                        event.state(),
                        serial,
                        time,
                        |state, modifiers, handle| {
                            if !pressed {
                                return if state.suppressed_keys.remove(&code.raw()) {
                                    FilterResult::Intercept(None)
                                } else {
                                    FilterResult::Forward
                                };
                            }
                            if state.suppressed_keys.contains(&code.raw()) {
                                return FilterResult::Intercept(None);
                            }
                            if let Some(action) = shortcut(modifiers, handle.modified_sym()) {
                                state.suppressed_keys.insert(code.raw());
                                FilterResult::Intercept(Some(action))
                            } else {
                                FilterResult::Forward
                            }
                        },
                    )
                    .flatten();
                if let Some(action) = action
                    && let Err(error) = self.apply_action(action)
                {
                    tracing::warn!(%error, "shortcut rejected");
                }
            }
            InputEvent::PointerMotion { event, .. } => {
                let Some(pointer) = self.seat.get_pointer() else {
                    return;
                };
                let Some(output) = self.space.outputs().next() else {
                    return;
                };
                let Some(geometry) = self.space.output_geometry(output) else {
                    return;
                };
                if geometry.size.w <= 0 || geometry.size.h <= 0 {
                    return;
                }
                let mut pos = pointer.current_location() + event.delta();
                if !pos.x.is_finite() || !pos.y.is_finite() {
                    return;
                }
                pos.x = pos.x.clamp(
                    f64::from(geometry.loc.x),
                    f64::from(geometry.loc.x) + f64::from(geometry.size.w) - 0.001,
                );
                pos.y = pos.y.clamp(
                    f64::from(geometry.loc.y),
                    f64::from(geometry.loc.y) + f64::from(geometry.size.h) - 0.001,
                );
                let under = self.surface_under(pos);
                self.pointer_scene_target = under.clone();
                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerMotionAbsolute { event, .. } => {
                let Some(output) = self.space.outputs().next() else {
                    return;
                };
                let Some(output_geo) = self.space.output_geometry(output) else {
                    return;
                };

                if output_geo.size.w <= 0 || output_geo.size.h <= 0 {
                    return;
                }
                let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();
                if !pos.x.is_finite() || !pos.y.is_finite() {
                    return;
                }
                let pos = smithay::utils::Point::from((
                    pos.x.clamp(
                        f64::from(output_geo.loc.x),
                        f64::from(output_geo.loc.x + output_geo.size.w) - 1.0,
                    ),
                    pos.y.clamp(
                        f64::from(output_geo.loc.y),
                        f64::from(output_geo.loc.y + output_geo.size.h) - 1.0,
                    ),
                ));

                let serial = SERIAL_COUNTER.next_serial();

                let pointer = self.seat.get_pointer().unwrap();

                let under = self.surface_under(pos);
                self.pointer_scene_target = under.clone();

                pointer.motion(
                    self,
                    under,
                    &MotionEvent {
                        location: pos,
                        serial,
                        time: event.time(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();

                if ButtonState::Pressed == button_state
                    && !pointer.is_grabbed()
                    && let Some((id, _, _)) = self
                        .scene
                        .as_ref()
                        .and_then(|scene| scene.hit(pointer.current_location(), self))
                {
                    self.focus_window_id(id);
                }

                // Relayout may change the surface under a stationary pointer.
                // Refresh the pointer target after focus/relayout and before the
                // button; an active grab retains its own routing.
                if !pointer.is_grabbed() {
                    let location = pointer.current_location();
                    let under = self.surface_under(location);
                    self.pointer_scene_target = under.clone();
                    pointer.motion(
                        self,
                        under,
                        &MotionEvent {
                            location,
                            serial,
                            time: event.time(),
                        },
                    );
                }
                if button_state == ButtonState::Pressed {
                    self.pointer_presses.insert(button, serial);
                } else {
                    self.pointer_presses.remove(&button);
                }
                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time: event.time(),
                    },
                );
                if button_state == ButtonState::Released {
                    // ClickGrab ends without restoring pending focus, including
                    // a drag across surfaces with no intervening scene redraw.
                    self.pointer_focus_dirty = true;
                }
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event, .. } => {
                self.refresh_pointer_focus();
                let source = event.source();

                let horizontal_amount = event.amount(Axis::Horizontal).unwrap_or_else(|| {
                    event.amount_v120(Axis::Horizontal).unwrap_or(0.0) * 15.0 / 120.
                });
                let vertical_amount = event.amount(Axis::Vertical).unwrap_or_else(|| {
                    event.amount_v120(Axis::Vertical).unwrap_or(0.0) * 15.0 / 120.
                });
                let horizontal_amount_discrete = event.amount_v120(Axis::Horizontal);
                let vertical_amount_discrete = event.amount_v120(Axis::Vertical);

                let mut frame = AxisFrame::new(event.time()).source(source);
                if horizontal_amount != 0.0 {
                    frame = frame.value(Axis::Horizontal, horizontal_amount);
                    if let Some(discrete) = horizontal_amount_discrete {
                        frame = frame.v120(Axis::Horizontal, discrete as i32);
                    }
                }
                if vertical_amount != 0.0 {
                    frame = frame.value(Axis::Vertical, vertical_amount);
                    if let Some(discrete) = vertical_amount_discrete {
                        frame = frame.v120(Axis::Vertical, discrete as i32);
                    }
                }

                if source == AxisSource::Finger {
                    if event.amount(Axis::Horizontal) == Some(0.0) {
                        frame = frame.stop(Axis::Horizontal);
                    }
                    if event.amount(Axis::Vertical) == Some(0.0) {
                        frame = frame.stop(Axis::Vertical);
                    }
                }

                let pointer = self.seat.get_pointer().unwrap();
                pointer.axis(self, frame);
                pointer.frame(self);
            }
            _ => {}
        }
    }
}

// Temporary nested development controls. Configuration binding integration is a
// separate phase; Alt avoids the usual host compositor's Super bindings.
fn shortcut(
    modifiers: &smithay::input::keyboard::ModifiersState,
    symbol: smithay::input::keyboard::Keysym,
) -> Option<flux_core::Action> {
    use flux_core::{Action, Direction, LayoutMode};
    use smithay::input::keyboard::Keysym;
    if !modifiers.alt || modifiers.ctrl || modifiers.logo {
        return None;
    }
    let direction = match symbol {
        Keysym::Left | Keysym::h | Keysym::H => Some(Direction::Left),
        Keysym::Right | Keysym::l | Keysym::L => Some(Direction::Right),
        Keysym::Up | Keysym::k | Keysym::K => Some(Direction::Up),
        Keysym::Down | Keysym::j | Keysym::J => Some(Direction::Down),
        _ => None,
    };
    if let Some(direction) = direction {
        return Some(if modifiers.shift {
            Action::Move { direction }
        } else {
            Action::Focus { direction }
        });
    }
    Some(match symbol {
        Keysym::Return => Action::Spawn {
            argv: vec!["foot".into()],
        },
        Keysym::q | Keysym::Q if modifiers.shift => Action::Quit,
        Keysym::q | Keysym::Q => Action::CloseWindow,
        Keysym::c | Keysym::C => Action::WorkspaceMode {
            mode: LayoutMode::Canvas,
        },
        Keysym::s | Keysym::S => Action::WorkspaceMode {
            mode: LayoutMode::Strip,
        },
        Keysym::equal | Keysym::plus => Action::Resize { delta: 32.0 },
        Keysym::minus => Action::Resize { delta: -32.0 },
        _ => return None,
    })
}
