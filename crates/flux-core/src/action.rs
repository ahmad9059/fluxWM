use crate::{Direction, LayoutMode};
use serde::{Deserialize, Serialize};

pub const MAX_PAN_DELTA: f64 = 1_000_000.;
pub const MAX_RESIZE_DELTA: f64 = 65_536.;
pub const MAX_ARGV_ENTRIES: usize = 256;
pub const MAX_ARGUMENT_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct InvalidAction(pub String);

/// Typed user intent. Deserialization validates shared scalar/argv policy; callers
/// constructing variants directly must call `validate` at their command boundary.
/// No command carries a raw GPU or Wayland object.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "name", rename_all = "kebab-case")]
pub enum Action {
    Focus { direction: Direction },
    Move { direction: Direction },
    Pan { dx: f64, dy: f64 },
    Resize { delta: f64 },
    WorkspaceMode { mode: LayoutMode },
    Fullscreen { toggle: bool },
    Floating { toggle: bool },
    Overview { toggle: bool },
    CloseWindow,
    ReloadConfig,
    Lock,
    Spawn { argv: Vec<String> },
    Quit,
}

// Internally tagged *unit* variants in serde accept extra fields despite
// deny_unknown_fields. Empty struct variants preserve the wire shape and make
// every variant's payload strict without going through a lossy JSON Value map.
#[derive(Deserialize)]
#[serde(tag = "name", rename_all = "kebab-case", deny_unknown_fields)]
enum WireAction {
    Focus { direction: Direction },
    Move { direction: Direction },
    Pan { dx: f64, dy: f64 },
    Resize { delta: f64 },
    WorkspaceMode { mode: LayoutMode },
    Fullscreen { toggle: bool },
    Floating { toggle: bool },
    Overview { toggle: bool },
    CloseWindow {},
    ReloadConfig {},
    Lock {},
    Spawn { argv: Vec<String> },
    Quit {},
}
impl<'de> Deserialize<'de> for Action {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let action = match WireAction::deserialize(de)? {
            WireAction::Focus { direction } => Self::Focus { direction },
            WireAction::Move { direction } => Self::Move { direction },
            WireAction::Pan { dx, dy } => Self::Pan { dx, dy },
            WireAction::Resize { delta } => Self::Resize { delta },
            WireAction::WorkspaceMode { mode } => Self::WorkspaceMode { mode },
            WireAction::Fullscreen { toggle } => Self::Fullscreen { toggle },
            WireAction::Floating { toggle } => Self::Floating { toggle },
            WireAction::Overview { toggle } => Self::Overview { toggle },
            WireAction::CloseWindow {} => Self::CloseWindow,
            WireAction::ReloadConfig {} => Self::ReloadConfig,
            WireAction::Lock {} => Self::Lock,
            WireAction::Spawn { argv } => Self::Spawn { argv },
            WireAction::Quit {} => Self::Quit,
        };
        action.validate().map_err(serde::de::Error::custom)?;
        Ok(action)
    }
}
/// Shared executable argument admission; no shell splitting or existence check.
/// Empty arguments after `argv[0]` are meaningful and permitted.
pub fn validate_argv(argv: &[String]) -> Result<(), InvalidAction> {
    if argv.is_empty() || argv.len() > MAX_ARGV_ENTRIES {
        return Err(InvalidAction(format!(
            "argv: expected 1..={MAX_ARGV_ENTRIES} arguments"
        )));
    }
    for (i, s) in argv.iter().enumerate() {
        if i == 0 && s.is_empty() || s.len() > MAX_ARGUMENT_BYTES || s.contains('\0') {
            return Err(InvalidAction(format!(
                "argv[{i}]: empty executable, oversized argument or embedded NUL"
            )));
        }
    }
    Ok(())
}
impl Action {
    /// Resource/input policy shared by IPC, configuration and internal commands.
    /// Window existence, lock authority and session permissions are checked by
    /// the compositor at application time; validation never grants permission.
    pub fn validate(&self) -> Result<(), InvalidAction> {
        fn delta(value: f64, max: f64, field: &str) -> Result<(), InvalidAction> {
            if !value.is_finite() || value.abs() > max {
                Err(InvalidAction(format!(
                    "{field}: expected finite value in -{max}..={max}"
                )))
            } else {
                Ok(())
            }
        }
        match self {
            Self::Pan { dx, dy } => {
                delta(*dx, MAX_PAN_DELTA, "pan.dx")?;
                delta(*dy, MAX_PAN_DELTA, "pan.dy")
            }
            Self::Resize { delta: d } => delta(*d, MAX_RESIZE_DELTA, "resize.delta"),
            Self::Spawn { argv } => validate_argv(argv),
            _ => Ok(()),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_variant_keeps_its_wire_shape_and_rejects_extra_fields() {
        let actions = [
            Action::Focus {
                direction: Direction::Left,
            },
            Action::Move {
                direction: Direction::Right,
            },
            Action::Pan { dx: 0., dy: 1. },
            Action::Resize { delta: -2. },
            Action::WorkspaceMode {
                mode: LayoutMode::Canvas,
            },
            Action::Fullscreen { toggle: true },
            Action::Floating { toggle: false },
            Action::Overview { toggle: true },
            Action::CloseWindow,
            Action::ReloadConfig,
            Action::Lock,
            Action::Spawn {
                argv: vec!["foot".into()],
            },
            Action::Quit,
        ];
        for action in actions {
            let mut value = serde_json::to_value(&action).unwrap();
            assert_eq!(
                serde_json::from_value::<Action>(value.clone()).unwrap(),
                action
            );
            value
                .as_object_mut()
                .unwrap()
                .insert("unexpected".into(), true.into());
            assert!(
                serde_json::from_value::<Action>(value).is_err(),
                "accepted unknown field for {action:?}"
            );
        }
        assert_eq!(
            serde_json::to_string(&Action::Lock).unwrap(),
            r#"{"name":"lock"}"#
        );
        assert_eq!(
            serde_json::to_string(&Action::Pan { dx: 1., dy: 2. }).unwrap(),
            r#"{"name":"pan","dx":1.0,"dy":2.0}"#
        );
    }
    #[test]
    fn duplicate_fields_invalid_types_and_unknown_variants_are_rejected() {
        for wire in [
            r#"{"name":"lock","name":"quit"}"#,
            r#"{"name":"pan","dx":1,"dx":2,"dy":0}"#,
            r#"{"name":"spawn","argv":["a"],"argv":["b"]}"#,
            r#"{"name":"pan","dx":"1","dy":0}"#,
            r#"{"name":"lock","argv":[]}"#,
            r#"{"name":"execute","argv":["sh"]}"#,
            r#"{"name":"fullscreen","toggle":null}"#,
            r#"{"name":"focus","direction":"north"}"#,
        ] {
            assert!(
                serde_json::from_str::<Action>(wire).is_err(),
                "accepted {wire}"
            );
        }
    }
    #[test]
    fn shared_numeric_policy_covers_json_and_internal_construction() {
        for action in [
            Action::Pan {
                dx: f64::NAN,
                dy: 0.,
            },
            Action::Pan {
                dx: 0.,
                dy: f64::NEG_INFINITY,
            },
            Action::Pan {
                dx: MAX_PAN_DELTA + 1.,
                dy: 0.,
            },
            Action::Resize {
                delta: MAX_RESIZE_DELTA + 1.,
            },
        ] {
            assert!(action.validate().is_err());
        }
        for wire in [
            r#"{"name":"pan","dx":1000001,"dy":0}"#,
            r#"{"name":"resize","delta":-65537}"#,
        ] {
            assert!(serde_json::from_str::<Action>(wire).is_err());
        }
        Action::Pan {
            dx: -MAX_PAN_DELTA,
            dy: MAX_PAN_DELTA,
        }
        .validate()
        .unwrap();
        Action::Resize {
            delta: MAX_RESIZE_DELTA,
        }
        .validate()
        .unwrap();
    }
    #[test]
    fn argv_limits_preserve_empty_arguments_and_literal_shell_characters() {
        validate_argv(&["foot".into(), "".into(), "$(do-not-execute)".into()]).unwrap();
        for argv in [
            vec![],
            vec![String::new()],
            vec!["a\0b".into()],
            vec!["x".repeat(MAX_ARGUMENT_BYTES + 1)],
            vec!["x".into(); MAX_ARGV_ENTRIES + 1],
        ] {
            assert!(validate_argv(&argv).is_err());
            assert!(Action::Spawn { argv: argv.clone() }.validate().is_err());
            let wire = serde_json::json!({"name":"spawn","argv":argv});
            assert!(serde_json::from_value::<Action>(wire).is_err());
        }
    }
}
