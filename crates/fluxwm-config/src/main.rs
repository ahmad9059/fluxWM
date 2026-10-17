//! Fresh-process Lua 5.4 evaluator. The compositor library never links this VM.
use flux_config::{Config, MAX_RESULT_BYTES, MAX_SOURCE_BYTES};
use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Table, Value, VmState};
use serde_json::{Map, Number, Value as Json};
use std::{
    collections::HashSet,
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

const MAX_DEPTH: usize = 32;
const MAX_VALUES: usize = 100_000;
const VM_BYTES: usize = 32 * 1024 * 1024;
fn fail(message: impl Into<String>) -> mlua::Error {
    mlua::Error::RuntimeError(message.into())
}
struct Conversion {
    active: HashSet<usize>,
    values: usize,
    bytes: usize,
}
impl Conversion {
    fn value(&mut self, value: Value, depth: usize, path: &str) -> mlua::Result<Json> {
        self.values += 1;
        if self.values > MAX_VALUES || depth > MAX_DEPTH {
            return Err(fail(format!("{path}: config depth/value budget exceeded")));
        }
        match value {
            Value::Nil => Err(fail(format!("{path}: nil is not a configuration value"))),
            Value::Boolean(b) => Ok(Json::Bool(b)),
            Value::Integer(n) => Ok(Json::Number(n.into())),
            Value::Number(n) => Number::from_f64(n)
                .map(Json::Number)
                .ok_or_else(|| fail(format!("{path}: nonfinite number"))),
            Value::String(s) => {
                let s = s.to_str()?.to_string();
                self.bytes += s.len();
                if self.bytes > MAX_RESULT_BYTES {
                    return Err(fail("config string budget exceeded"));
                }
                Ok(Json::String(s))
            }
            Value::Table(t) => self.table(t, depth, path),
            _ => Err(fail(format!("{path}: returned config must be plain data"))),
        }
    }
    fn table(&mut self, t: Table, depth: usize, path: &str) -> mlua::Result<Json> {
        if t.metatable().is_some() {
            return Err(fail(format!("{path}: metatables are forbidden")));
        }
        let id = t.to_pointer() as usize;
        if !self.active.insert(id) {
            return Err(fail(format!("{path}: cyclic table")));
        }
        let mut numeric = std::collections::BTreeMap::new();
        let mut fields = Map::new();
        // mlua Table::pairs uses lua_next, not __pairs; no Lua callback is run.
        for pair in t.pairs::<Value, Value>() {
            let (k, v) = pair?;
            match k {
                Value::Integer(i) if i > 0 => {
                    numeric.insert(i, self.value(v, depth + 1, &format!("{path}[{i}]"))?);
                }
                Value::String(k) => {
                    let k = k.to_str()?.to_string();
                    self.bytes += k.len();
                    if self.bytes > MAX_RESULT_BYTES {
                        return Err(fail("config key budget exceeded"));
                    }
                    let child = format!("{path}.{k}");
                    fields.insert(k, self.value(v, depth + 1, &child)?);
                }
                _ => {
                    return Err(fail(format!(
                        "{path}: expected string fields or positive integer array keys"
                    )));
                }
            }
        }
        self.active.remove(&id);
        if !numeric.is_empty() && !fields.is_empty() {
            return Err(fail(format!("{path}: mixed array and object")));
        }
        if !numeric.is_empty() {
            if numeric.keys().copied().ne(1..=numeric.len() as i64) {
                return Err(fail(format!("{path}: sparse array")));
            }
            Ok(Json::Array(numeric.into_values().collect()))
        } else if fields.is_empty() && is_array_path(path) {
            Ok(Json::Array(vec![]))
        } else {
            Ok(Json::Object(fields))
        }
    }
}
fn is_array_path(path: &str) -> bool {
    // Lua's empty table carries no array/object tag. Only declared array fields
    // receive this interpretation; unknown fields still fail schema validation.
    matches!(
        path.rsplit('.').next().unwrap_or(path),
        "outputs"
            | "bindings"
            | "rules"
            | "autostart"
            | "width_presets"
            | "argv"
            | "modifiers"
            | "points"
    )
}
fn evaluate(source: &[u8]) -> Result<Config, Box<dyn std::error::Error>> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err("source exceeds 1 MiB".into());
    }
    let source = std::str::from_utf8(source)?;
    // Loading only selected libs avoids even creating io/os/package/debug/coroutine.
    let lua = Lua::new_with(
        StdLib::MATH | StdLib::STRING | StdLib::TABLE,
        LuaOptions::default(),
    )?;
    lua.set_memory_limit(VM_BYTES)?;
    let globals = lua.globals();
    let math: Table = globals.get("math")?;
    math.raw_set("random", Value::Nil)?;
    math.raw_set("randomseed", Value::Nil)?;
    let strings: Table = globals.get("string")?;
    strings.raw_set("dump", Value::Nil)?;
    let allowed = [
        "assert", "error", "ipairs", "next", "pairs", "pcall", "select", "tonumber", "tostring",
        "type", "xpcall", "math", "string", "table",
    ];
    let names = globals
        .clone()
        .pairs::<String, Value>()
        .map(|p| p.map(|(k, _)| k))
        .collect::<mlua::Result<Vec<_>>>()?;
    for name in names {
        if !allowed.contains(&name.as_str()) {
            globals.raw_set(name, Value::Nil)?;
        }
    }
    let instructions = Arc::new(AtomicU64::new(0));
    let count = instructions.clone();
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(1000),
        move |_, _| {
            // Exhaustion remains exhausted after Lua catches the error with pcall.
            if count.fetch_add(1000, Ordering::Relaxed) >= 1_000_000 {
                Err(fail(
                    "instruction budget exhausted (parent deadline is authoritative)",
                ))
            } else {
                Ok(VmState::Continue)
            }
        },
    )?;
    let value = lua
        .load(source)
        .set_name("config.lua")
        .set_mode(mlua::chunk::ChunkMode::Text)
        .eval::<Value>()?;
    let data = Conversion {
        active: HashSet::new(),
        values: 0,
        bytes: 0,
    }
    .value(value, 0, "config")?;
    if !data.is_object() {
        return Err("config must return one table of named fields".into());
    }
    if data.get("schema_version") != Some(&Json::from(1)) {
        return Err("schema_version: explicit version 1 is required".into());
    }
    let bytes = serde_json::to_vec(&data)?;
    let config = Config::from_json(&bytes)?;
    // Structural decoding/defaulting occurs here; parent compiles regexes and
    // validates all numeric/resource constraints before publication.
    Ok(config)
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [mode] if mode == "--evaluate" => {
            let mut source = Vec::new();
            std::io::stdin()
                .take((MAX_SOURCE_BYTES + 1) as u64)
                .read_to_end(&mut source)?;
            let config = evaluate(&source)?;
            let bytes = serde_json::to_vec(&config)?;
            if bytes.len() > MAX_RESULT_BYTES {
                return Err("normalized result exceeds 4 MiB".into());
            }
            std::io::stdout().write_all(&bytes)?;
        }
        [mode, path] if mode == "--check" => {
            let file = std::fs::File::open(path)?;
            let mut source = Vec::new();
            file.take((MAX_SOURCE_BYTES + 1) as u64)
                .read_to_end(&mut source)?;
            let config = flux_config::load_with_helper(
                &std::env::current_exe()?,
                &source,
                Default::default(),
            )?;
            println!("{}", serde_json::to_string_pretty(&config.config)?);
        }
        _ => {
            return Err(
                "usage: fluxwm-config --check FILE (or internal --evaluate on stdin)".into(),
            );
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        let mut message = error.to_string();
        let max = 8192;
        if message.len() > max {
            message.truncate(message.floor_char_boundary(max));
        }
        eprintln!("{message}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_conversion_rejects_metatables_without_executing_metamethods() {
        let lua = Lua::new();
        let table = lua.create_table().unwrap();
        let meta = lua.create_table().unwrap();
        // A hostile __pairs must never execute during traversal.
        meta.raw_set(
            "__pairs",
            lua.create_function(|_, ()| -> mlua::Result<()> { panic!("metamethod executed") })
                .unwrap(),
        )
        .unwrap();
        table.set_metatable(Some(meta)).unwrap();
        let result = Conversion {
            active: HashSet::new(),
            values: 0,
            bytes: 0,
        }
        .value(Value::Table(table), 0, "config");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("metatables are forbidden")
        );
    }
    #[test]
    fn pinned_build_really_embeds_lua54() {
        let lua = Lua::new_with(StdLib::MATH, LuaOptions::default()).unwrap();
        assert_eq!(lua.globals().get::<String>("_VERSION").unwrap(), "Lua 5.4");
    }
    #[test]
    fn executable_return_values_and_text_only_chunks_are_rejected() {
        assert!(evaluate(b"return function() end").is_err());
        assert!(evaluate(b"\x1bLua\x54\0").is_err());
    }
}
