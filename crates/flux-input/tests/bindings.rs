use flux_config::{Binding, Key};
use flux_core::{Action, LayoutMode};
use flux_input::{BindingSet, Modifiers, UnmappedPolicy};
use xkbcommon::xkb;

fn map(layout: &str) -> xkb::Keymap {
    let context = xkb::Context::new(xkb::CONTEXT_NO_ENVIRONMENT_NAMES);
    xkb::Keymap::new_from_names(
        &context,
        "evdev",
        "pc105",
        layout,
        "",
        Some(String::new()),
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .unwrap()
}
fn sym(name: &str) -> xkb::Keysym {
    xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS)
}
fn binding(chord: &str) -> Binding {
    Binding {
        key: Key::Sym { sym: chord.into() },
        action: Action::WorkspaceMode {
            mode: LayoutMode::Canvas,
        },
    }
}
fn code(value: u16, modifiers: &[&str]) -> Binding {
    Binding {
        key: Key::Code {
            code: value,
            modifiers: modifiers.iter().map(|v| (*v).into()).collect(),
        },
        action: Action::Quit,
    }
}
fn alt() -> Modifiers {
    Modifiers {
        alt: true,
        ..Default::default()
    }
}

#[test]
fn exact_modifiers_and_base_symbols_ignore_locked_caps_and_shifted_letters() {
    let keymap = map("us");
    let mut shifted = binding("Alt+Shift+c");
    shifted.action = Action::CloseWindow;
    let bindings = BindingSet::compile(
        &[binding("Alt+c"), shifted],
        &keymap,
        UnmappedPolicy::Reject,
    )
    .unwrap();
    let mut state = xkb::State::new(&keymap);
    // Caps changes modified C, but the base-level contract still matches c.
    state.update_key(xkb::Keycode::new(66), xkb::KeyDirection::Down);
    state.update_key(xkb::Keycode::new(66), xkb::KeyDirection::Up);
    assert_eq!(state.key_get_one_sym(xkb::Keycode::new(54)), sym("C"));
    let base = keymap.key_get_syms_by_level(xkb::Keycode::new(54), 0, 0);
    assert_eq!(
        bindings.action(46, base, alt()),
        Some(&binding("Alt+c").action)
    );
    assert_eq!(
        bindings.action(
            46,
            base,
            Modifiers {
                shift: true,
                ..alt()
            }
        ),
        Some(&Action::CloseWindow)
    );
    assert!(
        bindings
            .action(
                46,
                base,
                Modifiers {
                    ctrl: true,
                    ..alt()
                }
            )
            .is_none()
    );
    assert!(
        bindings
            .action(
                46,
                base,
                Modifiers {
                    logo: true,
                    ..alt()
                }
            )
            .is_none()
    );
    assert!(
        bindings
            .action(
                46,
                base,
                Modifiers {
                    level3: true,
                    ..alt()
                }
            )
            .is_none()
    );
    assert!(
        bindings
            .action(
                46,
                base,
                Modifiers {
                    level5: true,
                    ..alt()
                }
            )
            .is_none()
    );
}

#[test]
fn invalid_names_numeric_aliases_and_duplicate_modifier_chords_fail_before_install() {
    let keymap = map("us");
    for bindings in [
        vec![binding("Alt+NoSuchFluxwmKeysym")],
        vec![binding("Alt+return")],
        vec![binding("Alt+Prior"), binding("Alt+Page_Up")],
        vec![binding("Logo+h"), binding("Super+h")],
        vec![code(35, &["Alt", "Alt"])],
    ] {
        assert!(
            BindingSet::compile(&bindings, &keymap, UnmappedPolicy::Reject).is_err(),
            "accepted {bindings:?}"
        );
    }
}

#[test]
fn code_and_symbol_collisions_are_rejected_in_both_orders() {
    let keymap = map("us");
    for bindings in [
        vec![binding("Alt+h"), code(35, &["Alt"])],
        vec![code(35, &["Alt"]), binding("Alt+h")],
    ] {
        let error = BindingSet::compile(&bindings, &keymap, UnmappedPolicy::Reject)
            .unwrap_err()
            .to_string();
        assert!(error.contains("both match evdev 35"), "{error}");
    }
}

#[test]
fn evdev_code_is_not_xkb_code_and_can_bind_without_symbol_assignment() {
    let keymap = map("us");
    let bindings = BindingSet::compile(
        &[code(35, &["Logo"]), code(767, &["Alt"])],
        &keymap,
        UnmappedPolicy::Reject,
    )
    .unwrap();
    assert_eq!(
        bindings.action(
            35,
            &[sym("h")],
            Modifiers {
                logo: true,
                ..Default::default()
            }
        ),
        Some(&Action::Quit)
    );
    assert!(
        bindings
            .action(
                43,
                &[sym("h")],
                Modifiers {
                    logo: true,
                    ..Default::default()
                }
            )
            .is_none()
    );
    assert_eq!(bindings.action(767, &[], alt()), Some(&Action::Quit));
}

#[test]
fn absent_symbols_are_explicit_and_foreign_groups_use_current_base_symbols() {
    let russian = map("ru");
    assert!(BindingSet::compile(&[binding("Alt+a")], &russian, UnmappedPolicy::Reject).is_err());
    let inactive = BindingSet::compile(
        &[binding("Alt+a")],
        &russian,
        UnmappedPolicy::RetainInactive,
    )
    .unwrap();
    assert_eq!(inactive.inactive_indices(), &[0]);
    assert!(inactive.action(30, &[sym("a")], alt()).is_none());
    let mixed = map("us,ru");
    let bindings = BindingSet::compile(
        &[binding("Alt+a"), binding("Alt+Cyrillic_ef")],
        &mixed,
        UnmappedPolicy::Reject,
    )
    .unwrap();
    let latin = mixed.key_get_syms_by_level(xkb::Keycode::new(38), 0, 0);
    let cyrillic = mixed.key_get_syms_by_level(xkb::Keycode::new(38), 1, 0);
    assert_eq!(latin, &[sym("a")]);
    assert_eq!(cyrillic, &[sym("Cyrillic_ef")]);
    assert!(bindings.action(30, latin, alt()).is_some());
    assert!(bindings.action(30, cyrillic, alt()).is_some());
    // A caller passing inconsistent keymap data cannot make an ambiguous event
    // select whichever binding happened to be first in the input list.
    assert!(
        bindings
            .action(30, &[sym("a"), sym("Cyrillic_ef")], alt())
            .is_none()
    );
}

#[test]
fn limits_and_action_validation_apply_to_direct_rust_callers_too() {
    let keymap = map("us");
    assert!(
        BindingSet::compile(
            &vec![binding("Alt+h"); 4097],
            &keymap,
            UnmappedPolicy::Reject
        )
        .is_err()
    );
    for value in [0, 768, u16::MAX] {
        assert!(BindingSet::compile(&[code(value, &[])], &keymap, UnmappedPolicy::Reject).is_err());
    }
    let mut invalid = binding("Alt+h");
    invalid.action = Action::Spawn { argv: vec![] };
    assert!(BindingSet::compile(&[invalid], &keymap, UnmappedPolicy::Reject).is_err());
}
