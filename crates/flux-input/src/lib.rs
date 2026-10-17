//! Keymap-qualified Rust binding tables. No device, seat, Lua or XKB object is retained.
use flux_config::{Binding, Key};
use flux_core::Action;
use std::collections::{BTreeMap, BTreeSet};
use xkbcommon::xkb;

const MAX_BINDINGS: usize = 4096;
const MAX_STRING_BYTES: usize = 4 * 1024 * 1024;
const MAX_KEYCODES: usize = 4096;
const MAX_LAYOUTS_PER_KEY: u32 = 32;
const MAX_RESOLVED_CHORDS: usize = 65_536;

/// Lock modifiers are deliberately absent. Level3/Level5 are separate from Alt
/// and suppress matching because the current schema cannot name those modifiers.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
    pub level3: bool,
    pub level5: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnmappedPolicy {
    Reject,
    /// For development defaults on foreign keymaps: retain a visible list of
    /// inactive bindings instead of failing the whole nested startup.
    RetainInactive,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct BindingError(String);

#[derive(Default, Debug)]
pub struct BindingSet {
    actions: Vec<Action>,
    codes: BTreeMap<(u16, Modifiers), usize>,
    symbols: BTreeMap<(u32, Modifiers), usize>,
    inactive: Vec<usize>,
}

impl BindingSet {
    /// Compile against all base levels of the supplied keymap. The caller must
    /// recompile when that keymap changes, before publishing new bindings.
    /// Canonical names are case sensitive; numeric aliases resolving to the same
    /// symbol and code/symbol overlap are rejected rather than ordered implicitly.
    pub fn compile(
        bindings: &[Binding],
        keymap: &xkb::Keymap,
        unmapped: UnmappedPolicy,
    ) -> Result<Self, BindingError> {
        let fail = |message: String| BindingError(message);
        if bindings.len() > MAX_BINDINGS {
            return Err(fail("bindings: maximum 4096".into()));
        }
        let mut result = Self::default();
        let mut bytes = 0usize;
        for (index, binding) in bindings.iter().enumerate() {
            binding
                .key
                .identity()
                .map_err(|e| fail(format!("bindings[{index}]: {e}")))?;
            binding
                .action
                .validate()
                .map_err(|e| fail(format!("bindings[{index}].action: {e}")))?;
            let (key_bytes, modifier_names, symbol_name, code) = match &binding.key {
                Key::Sym { sym } => {
                    let mut parts = sym.split('+').collect::<Vec<_>>();
                    let name = parts.pop().expect("validated nonempty chord");
                    (sym.len(), parts, Some(name), None)
                }
                Key::Code { code, modifiers } => (
                    modifiers.iter().map(String::len).sum(),
                    modifiers.iter().map(String::as_str).collect(),
                    None,
                    Some(*code),
                ),
            };
            bytes = bytes
                .checked_add(key_bytes)
                .ok_or_else(|| fail("binding string budget exceeded".into()))?;
            if let Action::Spawn { argv } = &binding.action {
                for arg in argv {
                    bytes = bytes
                        .checked_add(arg.len())
                        .ok_or_else(|| fail("binding string budget exceeded".into()))?;
                }
            }
            if bytes > MAX_STRING_BYTES {
                return Err(fail("binding strings exceed 4 MiB".into()));
            }
            let mut modifiers = Modifiers::default();
            for name in modifier_names {
                match name {
                    "Ctrl" => modifiers.ctrl = true,
                    "Alt" => modifiers.alt = true,
                    "Shift" => modifiers.shift = true,
                    "Super" | "Logo" => modifiers.logo = true,
                    _ => unreachable!("schema normalization already checked modifier tokens"),
                }
            }
            let previous = if let Some(name) = symbol_name {
                let sym = xkb::keysym_from_name(name, xkb::KEYSYM_NO_FLAGS);
                if sym == xkb::Keysym::NoSymbol {
                    return Err(fail(format!(
                        "bindings[{index}]: unknown case-sensitive keysym {name}"
                    )));
                }
                result.symbols.insert((sym.raw(), modifiers), index)
            } else {
                result
                    .codes
                    .insert((code.expect("validated code binding"), modifiers), index)
            };
            if let Some(previous) = previous {
                return Err(fail(format!(
                    "bindings[{index}] conflicts with bindings[{previous}] after keysym/modifier resolution"
                )));
            }
            result.actions.push(binding.action.clone());
        }

        let mut keys = Vec::new();
        keymap.key_for_each(|_, code| {
            if keys.len() <= MAX_KEYCODES {
                keys.push(code);
            }
        });
        if keys.len() > MAX_KEYCODES {
            return Err(fail("keymap exceeds 4096-key validation budget".into()));
        }
        // Physical evdev bindings do not require a symbol in this keymap. A
        // custom keyboard may report a valid code with no XKB assignment.
        let mut reached: BTreeSet<_> = result.codes.values().copied().collect();
        let mut resolved = 0usize;
        let chord_modifiers: BTreeSet<_> = result
            .codes
            .keys()
            .map(|(_, m)| *m)
            .chain(result.symbols.keys().map(|(_, m)| *m))
            .collect();
        for key in keys {
            let Some(code) = key.raw().checked_sub(8).and_then(|c| u16::try_from(c).ok()) else {
                continue;
            };
            if !(1..=767).contains(&code) {
                continue;
            }
            let layouts = keymap.num_layouts_for_key(key);
            if layouts > MAX_LAYOUTS_PER_KEY {
                return Err(fail("keymap exceeds 32 layouts per key".into()));
            }
            for layout in 0..layouts {
                let symbols = keymap.key_get_syms_by_level(key, layout, 0);
                for modifiers in &chord_modifiers {
                    let mut owner = result.codes.get(&(code, *modifiers)).copied();
                    for symbol in symbols {
                        if let Some(&index) = result.symbols.get(&(symbol.raw(), *modifiers)) {
                            if let Some(previous) = owner
                                && previous != index
                            {
                                return Err(fail(format!(
                                    "bindings[{index}] and bindings[{previous}] both match evdev {code} in layout {layout}"
                                )));
                            }
                            owner = Some(index);
                        }
                    }
                    if let Some(index) = owner {
                        reached.insert(index);
                        resolved += 1;
                        if resolved > MAX_RESOLVED_CHORDS {
                            return Err(fail(
                                "binding expansion exceeds 65536 resolved chords".into(),
                            ));
                        }
                    }
                }
            }
        }
        result.inactive = (0..bindings.len())
            .filter(|index| !reached.contains(index))
            .collect();
        if unmapped == UnmappedPolicy::Reject && !result.inactive.is_empty() {
            return Err(fail(format!(
                "bindings[{}]: chord is absent from the keymap's base levels",
                result.inactive[0]
            )));
        }
        result.symbols.retain(|_, index| reached.contains(index));
        Ok(result)
    }

    pub fn inactive_indices(&self) -> &[usize] {
        &self.inactive
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Input code is evdev, not XKB (which adds 8). Base symbols must come from
    /// the current group of the same keymap used for compilation. Ignore locked
    /// Caps/Num state; caller passes exact Ctrl/Alt/Shift/Super and level shifts.
    pub fn action(
        &self,
        code: u16,
        base_symbols: &[xkb::Keysym],
        modifiers: Modifiers,
    ) -> Option<&Action> {
        if !(1..=767).contains(&code) || modifiers.level3 || modifiers.level5 {
            return None;
        }
        let mut owner = self.codes.get(&(code, modifiers)).copied();
        for symbol in base_symbols {
            if let Some(&index) = self.symbols.get(&(symbol.raw(), modifiers)) {
                if let Some(previous) = owner
                    && previous != index
                {
                    return None;
                }
                owner = Some(index);
            }
        }
        owner.map(|index| &self.actions[index])
    }
}
