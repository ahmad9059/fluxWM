# Nested runtime configuration

The nested executable now reads Lua on startup and explicit reload, prepares bindings off dispatch, and publishes **layout gaps, workspace mode and bindings** together between dispatch batches. This is partial phase-6 integration, not full schema application or a production desktop.

```sh
cargo build --workspace --locked
fluxwm --backend nested --config /absolute/config.lua
fluxctl reload
fluxctl query config.effective
```

The standalone `fluxwm-config --check FILE` remains the detailed evaluator/schema diagnostic path. Runtime `config.check` is unavailable. No Lua VM is linked into the compositor.

## Source and helper selection

`--config PATH` selects a file; relative paths resolve against startup working directory. Otherwise an absolute nonempty `XDG_CONFIG_HOME` selects `fluxwm/config.lua`; absent/empty/relative XDG roots fall back to an absolute HOME's `.config/fluxwm/config.lua`. With neither absolute root, loading is disabled. `--no-config` explicitly disables it and is mutually exclusive with `--config`. Missing/invalid selected files keep development defaults on startup and retain the last successful settings on reload, with a static failure diagnostic. Missing default files currently also report failure rather than a special absent-default status.

The helper is the executable named `fluxwm-config` beside the compositor's resolved executable. `--config-helper PATH` explicitly overrides it; PATH is never searched. Paths must be nonempty, NUL-free and at most 4096 bytes before and after absolute expansion. Source names may contain non-UTF-8 bytes. Source contents must be UTF-8 and at most 1 MiB. The worker opens with nonblocking/CLOEXEC flags, verifies the opened descriptor is a regular file, and uses a bounded read. Symlinks are allowed. Modules are unavailable. Atomic replacement by the writer is recommended; concurrent in-place writes can produce mixed bytes. Kernel/filesystem operations have no universal wall deadline and can delay helper start or shutdown despite off-thread ownership.

## Generations and publication

An accepted reload returns a positive decimal-string generation immediately. It is not application acknowledgment. One replaceable pending path, one completion and one retirement slot bound the runtime controller; the inner coordinator owns one active source/evaluation and one latest pending source. A newer accepted request immediately invalidates older outer completions even before its file is opened. Stale results and replaced schemas/tables are discarded on workers. The coordinator's library `last_good` commit is not used as a substitute for compositor publication; the runtime retains its own successfully published desired snapshot.

The startup seat keymap is serialized before dispatch. A worker reconstructs a private native map, resolves the bindings and returns only Rust data. Keymap replacement is unavailable. Unknown/ambiguous/unmapped chords, modifier/group-changing triggers, unavailable binding actions and gaps above the layout model's 1024 limit reject the whole candidate. The wider schema accepts gaps through 4096; acceptance there does not promise runtime applicability. Publication also validates a clone of the live model, preserving existing IDs, focus and width policies; geometry failure leaves the old model/table/generation intact.

Publication waits until keyboard keys, tracked pointer buttons and pointer/keyboard grabs are clear. Diagnostics expose `prepared_generation` so a user can distinguish prepared work waiting for a safe boundary from an unprepared request. Holding input can defer publication indefinitely; release/focus cancellation wakes normal dispatch. Original consumed-key ownership remains intact. Signal, timer, host-close, fatal-error and quit paths seal publication before final dispatch. Shutdown joins the controller and cancels/reaps an active helper outside event dispatch.

Old schema strings, binding tables and replaced model storage move to one worker retirement slot. Publication waits for that slot to clear. Live-model cloning/validation and relayout still scale with admitted windows on dispatch; there is no universal latency, memory or hostile-client fairness claim. Source/evaluation/binding preparation is not render work.

## IPC and effective state

`capabilities.config_reload` indicates whether loading is enabled. `config.reload` and the `reload-config` action select the same fixed source; no arbitrary IPC path is accepted. `diagnostics.configuration` reports nullable requested/prepared/applied decimal-string generations, `pending`/`applied`/`failed`/`disabled` status and one static source-free error. A failed requested generation can differ from the retained applied generation. These diagnostics do not include paths, source text, binding identities or argv values.

`config.effective` returns the last successful desired gap/mode/binding count, current effective gap/mode/binding count and the `startup-seat-keymap` identity, plus explicit unsupported categories. Actions can subsequently change effective mode without rewriting the desired config or its applied generation. The response is a concise implemented-settings view, not serialization of the full potentially large schema.

The following remain unapplied and explicitly listed as unsupported: outer gap, width presets/default width/focus margin/canvas policy, input/device/keymap settings, outputs, animations, decoration, rules, autostart and security policy. The entire desired schema is retained internally, but acceptance does not activate these fields. In particular even valid autostart entries do not execute yet. Invalid startup configuration preserves development controls; terminal installation, accessible startup-error UI and a guaranteed recovery session still require packaging/session work. A valid user configuration can replace those controls, including with an empty binding list.

Automatic file watching/debounce, full effective-schema reporting, rules/autostart supervision, device/output transactions/rollback, coherent keymap replacement, native input/session qualification, sustained frame progress during hostile reloads, fuzzing and soak remain open. See the [private smoke harness](runtime-config-smoke.md) for actual scenarios and their limits.
