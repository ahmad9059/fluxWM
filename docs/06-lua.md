# Lua configuration contract

This document specifies the target FluxWM schema. A strict subset is now implemented in the isolated Lua **5.4** helper; see [config implementation](development/config.md) and its [production audit](development/config-production-audit.md) for supported fields and remaining integration. The helper manifest pins mlua and its enabled features. Schema acceptance alone does not apply outputs or install bindings. The [nested runtime](development/runtime-config.md) now publishes gap/mode/bindings on startup/manual reload; full device/output transactions remain open.

## User contract

Load `$XDG_CONFIG_HOME/fluxwm/config.lua`, falling back to `~/.config/fluxwm/config.lua` when XDG_CONFIG_HOME is unset. Permit an explicit `--config` path. The file returns one table with `schema_version = 1`. Unknown fields are errors unless under a documented extension namespace. No implicit global setter calls are required, so evaluation can produce one validated snapshot.

Proposed top-level fields:

| Field | Contents | Validation |
| --- | --- | --- |
| `schema_version` | Integer | Exactly supported major version |
| `layout` | Mode, gaps, width presets, focus margin, canvas options | Finite dimensions, positive widths, valid enum |
| `input` | Keymap, pointer, touchpad, gestures | Capability-aware device settings |
| `outputs` | Ordered output match rules (later matching fields win) | Valid modes/scales/transforms; deterministic matches |
| `animations` | Enabled, reduced motion, curve definitions, transitions | Valid curve references and bounded parameters |
| `decoration` | Borders, corners, shadows, blur | Valid colors, finite radii, resource bounds |
| `bindings` | Chord and action tables | Unique normalized chords, valid action arguments |
| `rules` | Match predicates and placement/render policies | Compile patterns once, explicit precedence |
| `autostart` | Argument arrays and lifecycle policy | No shell splitting, bounded entries |
| `security` | Capture/IPC policy choices | Cannot weaken lock invariants |

Use logical pixels for geometry, milliseconds for durations, Hz for refresh, and dimensionless ratios for scale and opacity. Reject NaN/infinity from computed Lua values. Colors are explicit sRGB `#RRGGBB` or `#RRGGBBAA`; normalize to a typed internal representation.

## Evaluation isolation

A supervised config helper process creates a fresh Lua VM per reload. The parent enforces a wall deadline, bounded result transport, and kill/reap on timeout. The [Lua 5.4 reproducer](evidence/lua54-probe.txt) demonstrates why catchable instruction-hook errors alone are insufficient. Expose a restricted library set and no raw filesystem/network/process/FFI bindings. Provide an explicit allowlisted module loader rooted in the config directory if modular configs are supported; prevent path traversal and C module loading. Prefer pure table construction and helper functions that finish during evaluation.

Set a VM memory limit and instruction budget; proposed starting limits are 32 MiB and one million instructions, to tune using real configs. A Lua hook cannot preempt an arbitrary blocking native call, so expose no blocking native helpers. Restrict or budget coroutines too. The [mlua API](https://docs.rs/mlua/latest/mlua/struct.Lua.html) documents memory limits and instruction hooks; its `sandbox` method has backend-specific availability and must not be assumed to sandbox Lua 5.4 automatically.

No Lua executes in the compositor process or on its render path in this design. Bindings are data compiled into Rust action enums. Rules compile into typed predicates. Future runtime callbacks require separately supervised execution and a bounded event queue with timeout/circuit-breaker semantics; they are outside version 1. This provides Lua expressiveness for configuration without adding a per-frame VM cost.

## Atomic reload

1. Debounce filesystem changes and evaluate a new generation in the helper process.
2. Convert to owned Rust values, rejecting unknown fields and invalid types with a full field path.
3. Compile patterns, bindings, curves, and device/output rules. Resolve references.
4. Compute a diff and classify settings as live, transactional, restart-required, or unsupported.
5. Send a complete validated snapshot and generation to the main loop. Drop stale helper results and reap the completed process.
6. Apply logical settings atomically between dispatch batches. Perform device/output operations through their own validated transactions.
7. On output/device application failure preserve or restore the last working hardware state and report actual effective settings.
8. Publish the new config generation and diagnostics. Retain a last-known-good snapshot.

Do not rerun autostart processes on every reload. Trigger autostart once per session unless an entry explicitly requests restart-on-change with supervised ownership. An invalid startup config should start a usable safe default session and show the error, including a terminal/reload binding. A proposed `fluxctl config check PATH` uses the same evaluation/validation engine without opening display devices or spawning commands.

## Actions and rules

Action tables use `{ name = "focus", direction = "left" }` and similar typed records. Supported action families: focus, move, resize, pan, workspace/output transfer, mode switch, floating/fullscreen/maximize, minimize/restore, graceful close, explicit client-disconnect recovery, overview/search, spawn, reload, lock, screenshot, and quit. Spawn takes an argv array; shell execution requires an explicit shell argv chosen by the user.

Rules are ordered; defaults apply first, then matching rules in order, with later values replacing earlier values for the same nonprivacy field. `capture` is the exception: any matching deny wins within the evaluation and becomes sticky before allow is considered; existing sticky deny also wins. Match native app ID, title, parent/dialog status, or X11 metadata where available, using Rust regex dialect for string patterns. Use a bounded non-backtracking pattern engine rather than arbitrary Lua predicates. App IDs/titles are untrusted metadata and cannot grant lock/capture/IPC privilege. Re-evaluate placement only when marked dynamic, avoiding moves on ordinary title updates. Re-evaluate privacy on every relevant metadata/relationship change, including app_id changes, before admitting a new capture job. Unknown/unclassified sensitive relationship state uses conservative redaction; app-controlled IDs cannot grant permission.

Output matches may include connector and EDID hints, using connector/make/model/serial field conjunctions; all rules apply in file order and later matching fields win, including wildcard rules. A disconnected output rule remains stored. Unsupported requested capability produces an actionable diagnostic and effective fallback; the user can inspect desired and actual state.

## Diagnostics and migration

Examples: `animations.camera.curve: unknown curve 'snappy'`; `outputs[2].scale: expected finite number between supported limits`; `bindings[4]: duplicate normalized chord Mod+Return`. Preserve source path and Lua traceback for evaluation failures, while omitting environment secrets from logs. Schema major changes require an explicit migration guide; minor additions remain backwards compatible. Generated reference docs and example validation should eventually share schema definitions.

## Conversion and hard-limit contract

The helper accepts text-only Lua source. Returned configuration is plain data: reject cycles, metatables, functions, threads, userdata, mixed/sparse arrays, embedded NUL in OS fields, nonfinite numbers, and excessive depth/entries/bytes. Traverse with raw operations that cannot invoke user metamethods. Parent-side validation repeats structural/schema checks before applying the result. See [resource policy](17-resource-security.md) for proposed numeric ceilings and why allocator limits do not bound whole-process RSS.

The host `lua` is currently 5.5.1, so use explicit `lua5.4`/`luac5.4` when checking this schema's target runtime. The example was rechecked with Lua 5.4.9; that standalone check is not a FluxWM parser test. A separate [pinned mlua probe](evidence/probes/mlua-hook/README.md) also reproduced catchability; it is a historical research probe, separate from the now-implemented helper. A bounded process result is the only candidate config accepted by the main loop; helper stdout cannot be interpreted as arbitrary commands.

Across the process boundary, return normalized serializable schema data, not Rust compiled regex/GPU/device objects. Parent validates bounded structure and compiles accepted native binding/rule data in bounded background work before atomic application. Lua itself runs only in the helper. Public IPC's 1 MiB frame bound is separate from the helper-result transport's proposed 4 MiB bound.

## Schema v1 disambiguation

Widths are tagged records: `{ logical_px = 960 }`, `{ fraction = 0.5 }`, or `{ preset = 2 }`; exactly one tag. Preset definitions cannot recursively reference a preset. Fraction uses usable viewport width. Geometry is logical pixels except explicitly suffixed fields such as `snap_distance_physical_px`. A key is `{ sym = "Super+h" }` for a keysym chord or `{ code = 35, modifiers = { "Super" } }` for an evdev-code chord; the backend performs the required xkb offset conversion. Letter keysyms use lowercase, and Shift is explicit in the chord.

Output match is `{ any = true }` or an AND combination of exact `connector`, `make`, `model`, `serial` hints; none is guaranteed unique. Use ordered later-wins field merging uniformly. An optional future selector ID must not silently change this precedence. Ambiguous physical matches produce diagnostics and safe placement. String rule patterns use bounded Rust-regex syntax. Current `Regex::is_match` matching is unanchored: a substring match is sufficient. Use `^` and `$` explicitly for whole-string rules. This is distinct from Lua patterns.

[Example contract checker](examples/check-config.lua) validates the supplied fixture's declared schema shapes, tagged widths/keys, output order representation, enums and field whitelist. It runs only against trusted documentation fixture data. It is not the future hardened helper/parser, hardware validator, or proof of adversarial config safety.

Keysym names use canonical case-sensitive xkb names (`space`, `Return`, `Escape`); letter keysyms are lowercase and Shift is an explicit modifier token, interpreted via the current keymap's unshifted/base symbol for matching. Non-letter symbols retain their canonical names. The fixture checker does not resolve actual keymap names; phase 6 must validate them with xkbcommon and test layout/Shift transitions. `capture = allow` cannot clear sticky deny; only an explicit trusted compositor action does.
