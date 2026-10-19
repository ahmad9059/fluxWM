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
    /// Cancel the nested seat's held state when the host keyboard focus leaves.
    /// Logical window selection persists; no absent backend release is required.
    pub fn host_focus_changed(&mut self, focused: bool) {
        if self.host_focus == Some(focused) {
            return;
        }
        self.host_focus = Some(focused);
        self.bump_sequence();
        if focused {
            return;
        }
        self.stats.host_focus_losses += 1;
        let time = smithay::backend::input::InputTime::now();
        if let Some(keyboard) = self.seat.get_keyboard() {
            let mut keys: Vec<_> = keyboard.pressed_keys().into_iter().collect();
            keys.sort_by_key(|key| key.raw());
            self.stats.focus_cancelled_keys += keys.len() as u64;
            for code in keys {
                keyboard.input::<(), _>(
                    self,
                    code,
                    KeyState::Released,
                    SERIAL_COUNTER.next_serial(),
                    time,
                    |state, _, _| {
                        if state.suppressed_keys.remove(&code.raw()) {
                            FilterResult::Intercept(())
                        } else {
                            FilterResult::Forward
                        }
                    },
                );
            }
        }
        self.suppressed_keys.clear();
        let buttons: Vec<_> = self.pointer_presses.keys().copied().collect();
        self.stats.focus_cancelled_buttons += buttons.len() as u64;
        if let Some(pointer) = self.seat.get_pointer() {
            for button in buttons {
                pointer.button(
                    self,
                    &ButtonEvent {
                        button,
                        state: ButtonState::Released,
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                    },
                );
            }
            pointer.frame(self);
        }
        self.pointer_presses.clear();
        if let Some((_, grab)) = self.popup_grab.as_mut() {
            grab.ungrab(smithay::desktop::PopupUngrabStrategy::All);
        }
        self.cleanup_popup_grab();
        if let Some(pointer) = self.seat.get_pointer()
            && pointer.is_grabbed()
        {
            pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        }
        self.pointer_focus_dirty = true;
    }

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
                if self.host_focus == Some(false) {
                    return;
                }
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
                            // Smithay's actual backend/KeysymHandle code is XKB
                            // (+8), despite the backend trait's evdev wording.
                            let evdev = handle
                                .raw_code()
                                .raw()
                                .checked_sub(8)
                                .and_then(|code| u16::try_from(code).ok());
                            let action = evdev
                                .and_then(|code| {
                                    state.bindings.action(
                                        code,
                                        &handle.raw_syms(),
                                        flux_input::Modifiers {
                                            ctrl: modifiers.ctrl,
                                            alt: modifiers.alt,
                                            shift: modifiers.shift,
                                            logo: modifiers.logo,
                                            level3: modifiers.iso_level3_shift,
                                            level5: modifiers.iso_level5_shift,
                                        },
                                    )
                                })
                                .cloned();
                            if let Some(action) = action {
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
                    f64::from(geometry.loc.x) + f64::from(geometry.size.w) - 1.0,
                );
                pos.y = pos.y.clamp(
                    f64::from(geometry.loc.y),
                    f64::from(geometry.loc.y) + f64::from(geometry.size.h) - 1.0,
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
                if self.backend == "native" {
                    self.request_redraw();
                }
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
                if self.backend == "native" {
                    self.request_redraw();
                }
            }
            InputEvent::PointerButton { event, .. } => {
                let pointer = self.seat.get_pointer().unwrap();

                let serial = SERIAL_COUNTER.next_serial();

                let button = event.button_code();

                let button_state = event.state();
                if (button_state == ButtonState::Released
                    && !self.pointer_presses.contains_key(&button))
                    || (button_state == ButtonState::Pressed
                        && self.pointer_presses.contains_key(&button))
                {
                    return;
                }

                if ButtonState::Pressed == button_state
                    && !pointer.is_grabbed()
                    && let Some((id, _, _)) = self
                        .scene
                        .as_ref()
                        .and_then(|scene| scene.hit(pointer.current_location(), self))
                {
                    self.focus_window_id(id);
                }

                // Route through the last composed scene after focus handling.
                // Any queued relayout becomes eligible only after composition;
                // an active grab retains its own routing.
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
pub fn development_bindings() -> Vec<flux_config::Binding> {
    use flux_core::{Action, Direction, LayoutMode};
    let mut bindings = Vec::new();
    let mut add = |chord: String, action| {
        bindings.push(flux_config::Binding {
            key: flux_config::Key::Sym { sym: chord },
            action,
        })
    };
    for (symbol, direction) in [
        ("Left", Direction::Left),
        ("h", Direction::Left),
        ("Right", Direction::Right),
        ("l", Direction::Right),
        ("Up", Direction::Up),
        ("k", Direction::Up),
        ("Down", Direction::Down),
        ("j", Direction::Down),
    ] {
        add(format!("Alt+{symbol}"), Action::Focus { direction });
        add(format!("Alt+Shift+{symbol}"), Action::Move { direction });
    }
    for shift in ["", "Shift+"] {
        for (symbol, action) in [
            (
                "Return",
                Action::Spawn {
                    argv: vec!["foot".into()],
                },
            ),
            (
                "c",
                Action::WorkspaceMode {
                    mode: LayoutMode::Canvas,
                },
            ),
            (
                "s",
                Action::WorkspaceMode {
                    mode: LayoutMode::Strip,
                },
            ),
            ("equal", Action::Resize { delta: 32.0 }),
        ] {
            add(format!("Alt+{shift}{symbol}"), action);
        }
    }
    add("Alt+minus".into(), Action::Resize { delta: -32.0 });
    add("Alt+plus".into(), Action::Resize { delta: 32.0 });
    add("Alt+q".into(), Action::CloseWindow);
    add("Alt+Shift+q".into(), Action::Quit);
    bindings
}
