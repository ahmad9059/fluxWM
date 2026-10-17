use crate::{Result, invalid};
use flux_core::{Action, Direction, LayoutMode};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Schema v1. Defaults are usable when input is absent; unsupported fields are errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub layout: Layout,
    pub input: Input,
    pub outputs: Vec<OutputRule>,
    pub animations: Animations,
    pub decoration: Decoration,
    pub bindings: Vec<Binding>,
    pub rules: Vec<Rule>,
    pub autostart: Vec<Autostart>,
    pub security: Security,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            layout: Layout::default(),
            input: Input::default(),
            outputs: vec![],
            animations: Animations::default(),
            decoration: Decoration::default(),
            bindings: vec![
                Binding {
                    key: Key::Sym {
                        sym: "Super+Return".into(),
                    },
                    action: Action::Spawn {
                        argv: vec!["foot".into()],
                    },
                },
                Binding {
                    key: Key::Sym {
                        sym: "Super+r".into(),
                    },
                    action: Action::ReloadConfig,
                },
                Binding {
                    key: Key::Sym {
                        sym: "Super+q".into(),
                    },
                    action: Action::CloseWindow,
                },
            ],
            rules: vec![],
            autostart: vec![],
            security: Security::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Width {
    Pixels { logical_px: f64 },
    Fraction { fraction: f64 },
    Preset { preset: usize },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Layout {
    pub mode: LayoutMode,
    pub gaps: f64,
    pub outer_gap: f64,
    pub width_presets: Vec<Width>,
    pub default_width: Width,
    pub focus_margin: f64,
    pub canvas: Canvas,
}
impl Default for Layout {
    fn default() -> Self {
        Self {
            mode: LayoutMode::Strip,
            gaps: 12.,
            outer_gap: 12.,
            width_presets: vec![
                Width::Fraction { fraction: 0.33 },
                Width::Fraction { fraction: 0.5 },
                Width::Fraction { fraction: 0.66 },
            ],
            default_width: Width::Preset { preset: 2 },
            focus_margin: 24.,
            canvas: Canvas::default(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Canvas {
    pub snap: bool,
    pub snap_distance_physical_px: f64,
    pub insertion: Direction,
}
impl Default for Canvas {
    fn default() -> Self {
        Self {
            snap: true,
            snap_distance_physical_px: 12.,
            insertion: Direction::Right,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Input {
    pub keymap: Keymap,
    pub pointer: Pointer,
    pub touchpad: Touchpad,
    pub gestures: Gestures,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Keymap {
    pub layout: String,
    pub options: String,
}
impl Default for Keymap {
    fn default() -> Self {
        Self {
            layout: "us".into(),
            options: String::new(),
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Pointer {
    pub focus_follows_mouse: bool,
    pub acceleration: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Touchpad {
    pub tap: bool,
    pub natural_scroll: bool,
}
impl Default for Touchpad {
    fn default() -> Self {
        Self {
            tap: true,
            natural_scroll: true,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Gestures {
    pub pan_fingers: u8,
    pub overview_fingers: u8,
}
impl Default for Gestures {
    fn default() -> Self {
        Self {
            pan_fingers: 3,
            overview_fingers: 4,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged, deny_unknown_fields)]
pub enum OutputMatch {
    Any {
        any: bool,
    },
    Fields {
        connector: Option<String>,
        make: Option<String>,
        model: Option<String>,
        serial: Option<String>,
    },
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AdaptiveSync {
    Auto,
    On,
    Off,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OutputRule {
    #[serde(rename = "match")]
    pub matcher: OutputMatch,
    pub scale: Option<f64>,
    pub adaptive_sync: Option<AdaptiveSync>,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutputSettings {
    pub scale: Option<f64>,
    pub adaptive_sync: Option<AdaptiveSync>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Curve {
    Spring {
        mass: f64,
        stiffness: f64,
        damping: f64,
    },
    Bezier {
        points: [f64; 4],
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub curve: String,
    pub duration_ms: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Animations {
    pub enabled: bool,
    pub reduced_motion: bool,
    pub curves: BTreeMap<String, Curve>,
    pub camera: Transition,
    pub window: Transition,
    pub fade: Transition,
}
impl Default for Animations {
    fn default() -> Self {
        Self {
            enabled: true,
            reduced_motion: false,
            curves: BTreeMap::from([
                (
                    "camera_spring".into(),
                    Curve::Spring {
                        mass: 1.,
                        stiffness: 320.,
                        damping: 36.,
                    },
                ),
                (
                    "gentle".into(),
                    Curve::Bezier {
                        points: [0.2, 0., 0.2, 1.],
                    },
                ),
            ]),
            camera: Transition {
                curve: "camera_spring".into(),
                duration_ms: None,
            },
            window: Transition {
                curve: "gentle".into(),
                duration_ms: Some(180.),
            },
            fade: Transition {
                curve: "gentle".into(),
                duration_ms: Some(120.),
            },
        }
    }
}
/// Explicit sRGB byte channels. Parsing and serialization use #RRGGBBAA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color(pub [u8; 4]);
impl Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        ))
    }
}
impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        if !matches!(s.len(), 7 | 9)
            || !s.starts_with('#')
            || !s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(serde::de::Error::custom(
                "expected sRGB #RRGGBB or #RRGGBBAA",
            ));
        }
        let mut c = [255; 4];
        for (i, v) in c.iter_mut().enumerate().take((s.len() - 1) / 2) {
            *v = u8::from_str_radix(&s[1 + i * 2..3 + i * 2], 16)
                .map_err(serde::de::Error::custom)?;
        }
        Ok(Self(c))
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Decoration {
    pub border: Border,
    pub corner_radius: f64,
    pub shadow: Shadow,
    pub blur: Blur,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Border {
    pub width: f64,
    pub active: Color,
    pub inactive: Color,
}
impl Default for Border {
    fn default() -> Self {
        Self {
            width: 2.,
            active: Color([137, 180, 250, 255]),
            inactive: Color([69, 71, 90, 255]),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Shadow {
    pub enabled: bool,
    pub radius: f64,
    pub opacity: f64,
}
impl Default for Shadow {
    fn default() -> Self {
        Self {
            enabled: true,
            radius: 18.,
            opacity: 0.25,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Blur {
    pub enabled: bool,
    pub radius: f64,
    pub passes: u8,
}
impl Default for Blur {
    fn default() -> Self {
        Self {
            enabled: false,
            radius: 8.,
            passes: 2,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged, deny_unknown_fields)]
pub enum Key {
    Sym { sym: String },
    Code { code: u16, modifiers: Vec<String> },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub key: Key,
    pub action: Action,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    #[serde(rename = "match")]
    pub matcher: RuleMatch,
    pub set: RuleSet,
    #[serde(default)]
    pub dynamic: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RuleMatch {
    pub app_id: Option<String>,
    pub title: Option<String>,
    pub is_dialog: Option<bool>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CapturePolicy {
    Allow,
    Deny,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RuleSet {
    pub floating: Option<bool>,
    pub center_on_parent: Option<bool>,
    pub capture: Option<CapturePolicy>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Autostart {
    pub argv: Vec<String>,
    #[serde(default)]
    pub restart_on_change: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PortalCapture {
    Consent,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IpcPolicy {
    SameUser,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct Security {
    pub portal_capture: PortalCapture,
    pub ipc: IpcPolicy,
}
impl Default for Security {
    fn default() -> Self {
        Self {
            portal_capture: PortalCapture::Consent,
            ipc: IpcPolicy::SameUser,
        }
    }
}

#[derive(Debug)]
pub struct CompiledRule {
    pub app_id: Option<Regex>,
    pub title: Option<Regex>,
    pub is_dialog: Option<bool>,
    pub set: RuleSet,
    pub dynamic: bool,
}
#[derive(Debug)]
pub struct CompiledConfig {
    pub config: Config,
    pub rules: Vec<CompiledRule>,
    pub binding_identities: Vec<String>,
}

fn range(v: f64, lo: f64, hi: f64, path: &str) -> Result<()> {
    if !v.is_finite() || v < lo || v > hi {
        return Err(invalid(format!(
            "{path}: expected finite number in {lo}..={hi}"
        )));
    }
    Ok(())
}
fn text(s: &str, path: &str, empty: bool) -> Result<()> {
    if (!empty && s.is_empty()) || s.len() > 4096 || s.contains('\0') {
        return Err(invalid(format!(
            "{path}: empty, oversized, or NUL-containing string"
        )));
    }
    Ok(())
}
fn argv(a: &[String], path: &str) -> Result<()> {
    flux_core::validate_argv(a).map_err(|error| invalid(format!("{path}: {error}")))
}

fn pattern(p: &Option<String>, path: &str, engine_limit: usize) -> Result<Option<Regex>> {
    p.as_ref()
        .map(|s| {
            text(s, path, true)?;
            RegexBuilder::new(s)
                .size_limit(engine_limit)
                .dfa_size_limit(engine_limit)
                .nest_limit(64)
                .build()
                .map_err(|e| invalid(format!("{path}: {e}")))
        })
        .transpose()
}
fn width(w: &Width, n: usize, allow_preset: bool, path: &str) -> Result<()> {
    match w {
        Width::Pixels { logical_px } => range(*logical_px, 1., 65536., path),
        Width::Fraction { fraction } => range(*fraction, 0.01, 16., path),
        Width::Preset { preset } if allow_preset && *preset > 0 && *preset <= n => Ok(()),
        _ => Err(invalid(format!(
            "{path}: invalid or recursive width preset"
        ))),
    }
}
impl Key {
    /// Deterministic chord normalization. Actual keysym/keymap resolution is a backend gate.
    pub fn identity(&self) -> Result<String> {
        fn mods(parts: impl Iterator<Item = String>) -> Result<String> {
            let mut out = BTreeSet::new();
            for p in parts {
                if !matches!(p.as_str(), "Super" | "Alt" | "Ctrl" | "Shift" | "Logo") {
                    return Err(invalid(format!("key: unknown modifier {p}")));
                }
                let p = if p == "Logo" { "Super".to_string() } else { p };
                if !out.insert(p) {
                    return Err(invalid("key: duplicate modifier"));
                }
            }
            Ok(out.into_iter().collect::<Vec<_>>().join("+"))
        }
        match self {
            Key::Sym { sym } => {
                text(sym, "key.sym", false)?;
                let mut p = sym.split('+').collect::<Vec<_>>();
                let key = p.pop().unwrap();
                if key.is_empty()
                    || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    || key.len() == 1 && key.as_bytes()[0].is_ascii_uppercase()
                {
                    return Err(invalid(
                        "key.sym: use canonical keysym names and lowercase letters",
                    ));
                }
                Ok(format!(
                    "sym:{}:{key}",
                    mods(p.into_iter().map(str::to_owned))?
                ))
            }
            Key::Code { code, modifiers } => {
                if !(1..=767).contains(code) {
                    return Err(invalid("key.code: expected evdev code 1..=767"));
                }
                Ok(format!("code:{}:{code}", mods(modifiers.iter().cloned())?))
            }
        }
    }
}
impl Config {
    /// Parse bounded normalized JSON using the same schema as helper output, with field paths.
    pub fn from_json(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(invalid("config result exceeds 4 MiB"));
        }
        crate::bounds::preflight(bytes)?;
        let mut de = serde_json::Deserializer::from_slice(bytes);
        let value = serde_path_to_error::deserialize(&mut de)
            .map_err(|e| invalid(format!("{}: {}", e.path(), e.inner())))?;
        de.end().map_err(|e| invalid(e.to_string()))?;
        Ok(value)
    }
    /// Validates all schema values and compiles bounded non-backtracking rule patterns.
    pub fn validate(&self) -> Result<CompiledConfig> {
        if self.schema_version != 1 {
            return Err(invalid("schema_version: only version 1 is supported"));
        }
        let l = &self.layout;
        for (v, p) in [
            (l.gaps, "layout.gaps"),
            (l.outer_gap, "layout.outer_gap"),
            (l.focus_margin, "layout.focus_margin"),
        ] {
            range(v, 0., 4096., p)?;
        }
        if l.width_presets.is_empty() || l.width_presets.len() > 128 {
            return Err(invalid("layout.width_presets: expected 1..=128 widths"));
        }
        for w in &l.width_presets {
            width(w, l.width_presets.len(), false, "layout.width_presets")?;
        }
        width(
            &l.default_width,
            l.width_presets.len(),
            true,
            "layout.default_width",
        )?;
        range(
            l.canvas.snap_distance_physical_px,
            0.,
            512.,
            "layout.canvas.snap_distance_physical_px",
        )?;
        text(&self.input.keymap.layout, "input.keymap.layout", false)?;
        text(&self.input.keymap.options, "input.keymap.options", true)?;
        range(
            self.input.pointer.acceleration,
            -1.,
            1.,
            "input.pointer.acceleration",
        )?;
        for n in [
            self.input.gestures.pan_fingers,
            self.input.gestures.overview_fingers,
        ] {
            if !(2..=5).contains(&n) {
                return Err(invalid("input.gestures: finger count must be 2..=5"));
            }
        }
        if self.outputs.len() > 128 {
            return Err(invalid("outputs: maximum 128 rules"));
        }
        for (i, o) in self.outputs.iter().enumerate() {
            let p = format!("outputs[{i}]");
            match &o.matcher {
                OutputMatch::Any { any: true } => (),
                OutputMatch::Any { any: false } => {
                    return Err(invalid(format!("{p}.match.any: must be true")));
                }
                OutputMatch::Fields {
                    connector,
                    make,
                    model,
                    serial,
                } => {
                    let values = [connector, make, model, serial];
                    if values.iter().all(|v| v.is_none()) {
                        return Err(invalid(format!("{p}.match: empty selector")));
                    }
                    for s in values.into_iter().flatten() {
                        text(s, &p, false)?;
                    }
                }
            }
            if let Some(s) = o.scale {
                range(s, 0.25, 8., &format!("{p}.scale"))?;
            }
        }
        let a = &self.animations;
        if a.curves.len() > 128 {
            return Err(invalid("animations.curves: maximum 128 curves"));
        }
        for (name, c) in &a.curves {
            text(name, "animations.curves name", false)?;
            match c {
                Curve::Spring {
                    mass,
                    stiffness,
                    damping,
                } => {
                    range(*mass, 0.001, 100., "spring.mass")?;
                    range(*stiffness, 0.001, 100000., "spring.stiffness")?;
                    range(*damping, 0., 100000., "spring.damping")?;
                }
                Curve::Bezier { points } => {
                    for (i, p) in points.iter().enumerate() {
                        range(
                            *p,
                            if i % 2 == 0 { 0. } else { -4. },
                            if i % 2 == 0 { 1. } else { 4. },
                            "bezier.points",
                        )?;
                    }
                }
            }
        }
        for (p, t) in [
            ("camera", &a.camera),
            ("window", &a.window),
            ("fade", &a.fade),
        ] {
            if !a.curves.contains_key(&t.curve) {
                return Err(invalid(format!(
                    "animations.{p}.curve: unknown curve {}",
                    t.curve
                )));
            }
            if let Some(d) = t.duration_ms {
                range(d, 0., 10000., &format!("animations.{p}.duration_ms"))?;
            }
            if matches!(a.curves.get(&t.curve), Some(Curve::Bezier { .. }))
                && t.duration_ms.is_none()
            {
                return Err(invalid(format!(
                    "animations.{p}.duration_ms: required for bezier"
                )));
            }
        }
        let d = &self.decoration;
        for (v, p, max) in [
            (d.border.width, "decoration.border.width", 64.),
            (d.corner_radius, "decoration.corner_radius", 512.),
            (d.shadow.radius, "decoration.shadow.radius", 512.),
            (d.blur.radius, "decoration.blur.radius", 64.),
            (d.shadow.opacity, "decoration.shadow.opacity", 1.),
        ] {
            range(v, 0., max, p)?;
        }
        if !(1..=8).contains(&d.blur.passes) {
            return Err(invalid("decoration.blur.passes: expected 1..=8"));
        }
        if self.bindings.len() > 4096 {
            return Err(invalid("bindings: maximum 4096"));
        }
        let mut ids = BTreeSet::new();
        let mut binding_identities = Vec::new();
        for (i, b) in self.bindings.iter().enumerate() {
            let id = b.key.identity()?;
            if !ids.insert(id.clone()) {
                return Err(invalid(format!(
                    "bindings[{i}]: duplicate normalized chord {id}"
                )));
            }
            binding_identities.push(id);
            b.action
                .validate()
                .map_err(|error| invalid(format!("bindings[{i}].action: {error}")))?;
        }
        if self.rules.len() > 1024 {
            return Err(invalid("rules: maximum 1024"));
        }
        // Allocate a shared engine-size budget across every pattern before any
        // compilation. Limits concern regex engine accounting, not exact RSS.
        let pattern_count = self
            .rules
            .iter()
            .map(|r| {
                usize::from(r.matcher.app_id.is_some()) + usize::from(r.matcher.title.is_some())
            })
            .sum::<usize>();
        let engine_limit = (32 * 1024 * 1024 / pattern_count.max(1)).min(1024 * 1024);
        let mut rules = Vec::new();
        for (i, r) in self.rules.iter().enumerate() {
            if r.matcher.app_id.is_none()
                && r.matcher.title.is_none()
                && r.matcher.is_dialog.is_none()
            {
                return Err(invalid(format!("rules[{i}].match: empty matcher")));
            }
            rules.push(CompiledRule {
                app_id: pattern(
                    &r.matcher.app_id,
                    &format!("rules[{i}].match.app_id"),
                    engine_limit,
                )?,
                title: pattern(
                    &r.matcher.title,
                    &format!("rules[{i}].match.title"),
                    engine_limit,
                )?,
                is_dialog: r.matcher.is_dialog,
                set: r.set.clone(),
                dynamic: r.dynamic,
            });
        }
        if self.autostart.len() > 128 {
            return Err(invalid("autostart: maximum 128 commands"));
        }
        for (i, a) in self.autostart.iter().enumerate() {
            argv(&a.argv, &format!("autostart[{i}].argv"))?;
        }
        Ok(CompiledConfig {
            config: self.clone(),
            rules,
            binding_identities,
        })
    }
    pub fn output_settings(
        &self,
        connector: &str,
        make: &str,
        model: &str,
        serial: &str,
    ) -> OutputSettings {
        let mut settings = OutputSettings::default();
        for o in &self.outputs {
            let matches = match &o.matcher {
                OutputMatch::Any { any } => *any,
                OutputMatch::Fields {
                    connector: c,
                    make: m,
                    model: mo,
                    serial: s,
                } => {
                    c.as_deref().is_none_or(|v| v == connector)
                        && m.as_deref().is_none_or(|v| v == make)
                        && mo.as_deref().is_none_or(|v| v == model)
                        && s.as_deref().is_none_or(|v| v == serial)
                }
            };
            if matches {
                if let Some(scale) = o.scale {
                    settings.scale = Some(scale);
                }
                if let Some(a) = o.adaptive_sync {
                    settings.adaptive_sync = Some(a);
                }
            }
        }
        settings
    }
}
impl CompiledConfig {
    /// Every metadata update must re-evaluate privacy before capture admission.
    /// This returns eligibility only, never capture authority; deny remains sticky.
    pub fn evaluate_rules(
        &self,
        app_id: &str,
        title: &str,
        is_dialog: bool,
        sticky_deny: bool,
    ) -> RuleSet {
        let mut result = RuleSet::default();
        let mut deny = sticky_deny;
        for r in &self.rules {
            if r.app_id.as_ref().is_some_and(|p| !p.is_match(app_id))
                || r.title.as_ref().is_some_and(|p| !p.is_match(title))
                || r.is_dialog.is_some_and(|v| v != is_dialog)
            {
                continue;
            }
            if let Some(v) = r.set.floating {
                result.floating = Some(v);
            }
            if let Some(v) = r.set.center_on_parent {
                result.center_on_parent = Some(v);
            }
            if let Some(v) = r.set.capture {
                deny |= v == CapturePolicy::Deny;
                result.capture = Some(v);
            }
        }
        if deny {
            result.capture = Some(CapturePolicy::Deny);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_roundtrip_and_remain_valid() {
        let c = Config::default();
        let json = serde_json::to_vec(&c).unwrap();
        let decoded = Config::from_json(&json).unwrap();
        assert_eq!(c, decoded);
        decoded.validate().unwrap();
    }
    #[test]
    fn tagged_records_reject_ambiguity_and_unknown_fields() {
        for value in [
            r#"{"layout":{"default_width":{"fraction":0.5,"logical_px":100}}}"#,
            r#"{"bindings":[{"key":{"sym":"Super+h","code":35},"action":{"name":"lock"}}]}"#,
            r#"{"outputs":[{"match":{"any":true,"connector":"eDP-1"}}]}"#,
            r#"{"input":{"pointer":{"accleration":0.5}}}"#,
            r#"{"bindings":[{"key":{"sym":"a"},"action":{"name":"lock","argv":[]}}]}"#,
        ] {
            assert!(
                Config::from_json(value.as_bytes()).is_err(),
                "accepted {value}"
            );
        }
    }
    #[test]
    fn invalid_references_regex_ranges_and_duplicate_keys_rejected() {
        let mut c = Config::default();
        c.layout.default_width = Width::Preset { preset: 999 };
        assert!(c.validate().is_err());
        c = Config::default();
        c.input.pointer.acceleration = f64::NAN;
        assert!(c.validate().is_err());
        c = Config::default();
        c.bindings.push(Binding {
            key: Key::Sym {
                sym: "Logo+Return".into(),
            },
            action: Action::Lock,
        });
        assert!(c.validate().is_err());
        c = Config::default();
        c.rules.push(Rule {
            matcher: RuleMatch {
                app_id: Some("(?=backtracking)".into()),
                ..Default::default()
            },
            set: RuleSet::default(),
            dynamic: false,
        });
        assert!(c.validate().is_err());
        c = Config::default();
        c.autostart.push(Autostart {
            argv: vec!["evil\0argv".into()],
            restart_on_change: false,
        });
        assert!(c.validate().is_err());
    }
    #[test]
    fn aggregate_pattern_engine_budget_rejects_large_combined_programs() {
        let rule = Rule {
            matcher: RuleMatch {
                app_id: Some("a{5000}".into()),
                title: Some("a{5000}".into()),
                is_dialog: None,
            },
            set: RuleSet::default(),
            dynamic: false,
        };
        let mut c = Config {
            rules: vec![rule.clone()],
            ..Default::default()
        };
        c.validate().unwrap();
        c.rules = vec![rule; 1024];
        assert!(
            c.validate()
                .expect_err("aggregate budget must reject")
                .to_string()
                .contains("size limit")
        );
    }
    #[test]
    fn all_matching_output_rules_merge_fields_in_order() {
        let c=Config::from_json(br#"{"outputs":[{"match":{"connector":"eDP-1"},"scale":1.25,"adaptive_sync":"on"},{"match":{"any":true},"scale":2.0}]}"#).unwrap();
        c.validate().unwrap();
        assert_eq!(
            c.output_settings("eDP-1", "", "", ""),
            OutputSettings {
                scale: Some(2.),
                adaptive_sync: Some(AdaptiveSync::On)
            }
        );
    }
    #[test]
    fn deny_first_capture_is_sticky_but_other_fields_are_later_wins() {
        let c=Config::from_json(br#"{"rules":[{"match":{"app_id":"secret"},"set":{"capture":"deny","floating":true}},{"match":{"app_id":"secret"},"set":{"capture":"allow","floating":false}}]}"#).unwrap().validate().unwrap();
        let r = c.evaluate_rules("secret", "", false, false);
        assert_eq!(r.capture, Some(CapturePolicy::Deny));
        assert_eq!(r.floating, Some(false));
        assert_eq!(
            c.evaluate_rules("renamed", "", false, true).capture,
            Some(CapturePolicy::Deny)
        );
        assert_eq!(c.evaluate_rules("renamed", "", false, false).capture, None);
    }
}
