# Implemented configuration foundation

The source under `crates/flux-config` and `crates/fluxwm-config` implements the initial
schema and evaluator, including the documentation example in `docs/examples/config.lua`.
This does not close phase 6: compositor integration, transactions, keymap resolution,
autostart lifecycle, hot reload and reference generation still need implementation.
The wider documents describe the target contract; this file records current behavior.

## Run and consume

```sh
cargo build -p fluxwm-config
cargo run -p fluxwm-config -- --check docs/examples/config.lua
cargo test -p flux-config -p fluxwm-config
cargo clippy -p flux-config -p fluxwm-config --all-targets -- -D warnings
```

`--check FILE` opens a bounded source snapshot and invokes a fresh supervised copy of
the executable. It prints normalized JSON after parent-side schema/range/reference
validation and regex compilation; it does not connect to a compositor, open display
devices or run `spawn`/`autostart`. The internal `--evaluate` stdin mode is a child
entrypoint; invoke it through the supervisor, not as an isolation/deadline API.

The parent library does not depend on `mlua`:

```rust,ignore
let compiled = flux_config::load_with_helper(
    absolute_helper_path,
    source_snapshot,
    flux_config::HelperLimits::default(),
)?;
// compiled.config is the owned serializable snapshot.
// compiled.rules contains bounded Rust Regex values.
// compiled.binding_identities detects duplicate normalized chords.
```

The caller must run loading/compilation away from the compositor dispatch/render
path. Serialize requests with one active helper and one coalesced latest request;
attach generations and reject stale results before applying. This library does not
pretend an independent function invocation establishes that compositor-wide queue.
Use `Config::default()` as a candidate safe startup fallback and validate it; starting
an actually usable terminal also requires the compositor/backend and the separately
installed `foot` executable. Hardware/device acceptance is not schema acceptance.

## Implemented v1 schema

All fields in the example are decoded into owned Rust values. Section omission fills
explicit defaults; Lua source must explicitly return `schema_version = 1`. Unknown
fields are errors in every record, including unit action variants. Width/key/output
match records reject competing tags. Width presets are one-based and nonrecursive.
Colors normalize to four sRGB byte channels and serialize as `#RRGGBBAA`. Lua empty
tables are arrays only at declared array fields (`bindings`, `outputs`, `rules`,
`autostart`, `width_presets`, `argv`, `modifiers`, `points`); elsewhere they are records.

The current core action set is focus, move, pan, resize, workspace-mode, fullscreen,
floating, overview, close-window, reload-config, lock, spawn and quit. Broader action
families listed in `docs/06-lua.md` remain roadmap work. Output rules currently support
exact connector/make/model/serial conjunction or `{any=true}`, optional scale and
adaptive_sync; mode/transform/position transactions are not implemented by this schema.

Keys normalize modifier ordering and `Logo` to `Super` and reject duplicate normalized
chords, invalid modifier tokens, uppercase single letters and invalid evdev ranges.
Canonical xkb keysym existence and actual keymap/layout matching must be resolved by
the input integration; accepting a syntactically plausible name does not prove it is
a real keysym. Strings use Rust regex syntax with 4 KiB pattern bounds, per-pattern compiled-size/DFA limits of at most 1 MiB and a nesting limit of 64.
A shared 32 MiB engine-size budget is apportioned across all patterns, separately
for compiled programs and lazy DFA caches; configurations near the rule limit
receive smaller per-pattern allowances. These limits do not bound exact RSS.
The 1,024-rule bound is an admission bound,
not evidence of acceptable full-rule-set latency or RSS; benchmark/fuzz qualification
remains required.

`evaluate_rules` returns matching nonprivacy fields with later-wins precedence. Any
matching capture deny or existing sticky deny wins over all allows. The caller owns
stored sticky state and must re-evaluate before capture on every relevant metadata
change. Capture `allow` is eligibility only and cannot authorize a protocol operation.
Unknown relationship classification/conservative redaction is a compositor responsibility;
this initial API accepts only an already classified boolean `is_dialog`.

## Process and conversion bounds

The helper is pinned to mlua revision
`a186d461c7772aed4cb0a9fa1d1899121c6450e7` with `lua54,vendored`.
Only selected pure base functions plus math/string/table are exposed. No coroutine,
OS, I/O, package/require, dynamic loaders, debugger, metatable access/mutation, raw
access helpers, RNG, string.dump, print or host globals are exposed. Modules and utf8
helpers are not implemented. Native string/table operations remain bounded through
VM allocation, process limits and the parent wall deadline, not instruction hooks.

Limits: 1 MiB UTF-8 source, 32 MiB Lua allocator accounting, advisory cumulative
1,000,000-instruction hook, depth 32, 100,000 returned values, 4 MiB normalized result,
16 KiB stderr, default one-second deadline, 256 MiB address space, two CPU seconds,
32 descriptors, zero core/file-size allowance. Address space is not RSS. These are
starting admission limits, not measured production-safe policies. Parent JSON performs
a streaming structural budget check before typed allocation; source/result transports
are nonblocking, and per-pass reads are bounded so output cannot starve the deadline.

The Linux supervisor clears the environment except LANG, changes cwd to `/`, sets
no_new_privs and resource limits, and explicitly marks every descriptor above stderr
close-on-exec using close_range. Failure of these hardening operations fails the load;
there is no silently weaker fallback. A child guard kills/reaps on timeout, transport
failure and early return; successful children are reaped too. Linux close_range support
is required. These containment controls are not seccomp, a filesystem namespace or
protection against an exploitable native VM that executes arbitrary syscalls. No such
stronger boundary is claimed.

Returned data rejects functions, threads/userdata, cycles, metatables, sparse/mixed
arrays, non-UTF-8 strings, nonfinite numbers and invalid/unknown schema data. Embedded
NUL is rejected in OS-facing strings. Raw table iteration never invokes Lua callbacks.
The subprocess returns only normalized serializable data; regex compilation remains
in the parent library. The compositor must atomically install a fully accepted snapshot
and retain the last working generation on every error.

## Embedding signal policy

The supervisor writes to child pipes and relies on the normal Rust executable signal policy that ignores SIGPIPE, so a closed pipe becomes an I/O error. Embedders that change SIGPIPE disposition must provide an equivalent reviewed thread/signal policy. Do not change a host process's signal disposition implicitly from this library; qualify EPIPE behavior and cleanup in that embedding environment. This is source-derived integration guidance, not a tested guarantee for arbitrary non-Rust hosts.
