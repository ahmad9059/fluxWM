I did this review with Read and Grep only, against the frozen `2a4cfb79` snapshot. I ran no commands or tests and made no edits. I couldn't write the plan file or call ExitPlanMode, because neither tool exists in this session.

## Findings

I found no HIGH defects that the current code can actually reach in either component. There is one MEDIUM finding, and it is a test that doesn't prove its claim, not a product defect.

### MEDIUM — the shutdown-cancellation test would still pass if cancellation were broken

**Where:** `crates/fluxwm-config/tests/coordinator.rs:137-176`, together with `crates/flux-config/src/supervisor.rs:137` and `crates/fluxwm-config/src/main.rs:147-158`.

**Trigger:**
- The helper is started with `RLIMIT_CPU` set to 2 seconds (soft and hard).
- The test script loops forever. Once the Lua instruction budget runs out, the hook raises an error every 1,000 instructions. That always happens inside the inner `while true do end` (a single jump instruction), so the outer `pcall` catches it and the script never ends.
- The kernel therefore kills the helper with SIGXCPU after about 2 seconds of CPU time.
- The test only requires `shutdown()` to finish in under 3 seconds.

**Path:** Suppose the worker stopped checking `cancelled` (lines 94 and 167 of `supervisor.rs` removed). Shutdown would still finish in about 2 seconds: the helper dies from the CPU limit, `Reap` collects it, the result is dropped because the coordinator is closed, and the join returns. Every assertion in the test would still pass: elapsed time under 3 seconds, the helper reaped, `Closed` errors, and no late notification. So the test shows the 10-second deadline is never reached, but it doesn't show that cancellation works. On a busy machine it may also be flaky in the other direction.

**Closure test:** Require the bound to sit well below the CPU limit, e.g. `started.elapsed() < Duration::from_millis(750)`. Optionally, add a unit test for `load_with_helper_cancellable` that sets the flag while the helper is running and checks that the result is `ConfigError::Cancelled`, not `Helper` or `Timeout`.

### LOW (noted, not escalated)
- **Superseding doesn't cancel running work** (`coordinator.rs:177`). The cancel flag is used only by shutdown, so an outdated helper keeps running until its deadline before the newest request starts. This matches the "one active helper" design, but it can roughly double reload latency, and only `test 1` relies on it.
- **Failure kinds can be misleading:**
  - If the deadline is longer than 2 seconds, a CPU-bound config is killed by the CPU limit and reported as `Helper`, not `Timeout`.
  - If stderr exceeds 16 KiB or the result exceeds 4 MiB, it's reported as `Invalid` ("schema or value validation").
- **Busy loop after the helper exits** (`supervisor.rs:194-197`). If the child has exited but its output pipes aren't closed yet, the loop restarts without sleeping. The deadline still bounds it, and the Lua sandbox has no way to start processes, so it isn't reachable now.
- **The key cap doesn't stop iteration** (`flux-input/src/lib.rs:128-134`). `key_for_each` still visits every keycode after the 4,096 limit; the callback just stops storing them. The keymap's memory has already been allocated by that point anyway.
- **Development controls differ slightly from the docs:**
  - On a US keymap, `Alt+plus` is inactive because `plus` isn't on a base level. The same physical keys still resize through `Alt+Shift+equal`, but every default US startup logs one inactive binding.
  - `Alt+Shift+{c,s,Return,equal}` are bound but missing from the table in `docs/development/runtime.md:28-36`.

## Controls checked and holding
- **Coordinator:**
  - Generations are issued with a checked `+1`. Oversized or non-UTF-8 input is rejected before the generation advances.
  - A result is published only if it is still the latest and the coordinator isn't closed, both checked under the lock.
  - `request` clears any waiting completion, and `take_completion` and `commit` both re-check the generation.
  - Candidates from another coordinator are caught with `Weak::ptr_eq`. Because the `Weak` keeps the allocation alive, a new coordinator can't reuse the same address.
  - Merely evaluating a config never changes `last_good`.
  - `notify` runs after the lock is released (`coordinator.rs:213`), and the unit test's `try_lock` actually checks this.
  - Shutdown sets `closed`, clears the waiting job and completion, sets the cancel flag, then joins the worker. A lock poisoned by a panic is recovered.
  - The helper PID is set and cleared under the same lock as `active`, so `await_helper` can't see a previous job's PID.
- **Generated diagnostics are fixed text** and never quote the source; an actual Lua error test covers this.
- **Bindings:**
  - Modifier tokens in the schema (`schema.rs:486`) match the compiler's handling exactly, so the `unreachable!` can't fire.
  - Case-sensitive keysyms with numeric or alias duplicates are rejected when the raw keysym is inserted.
  - Code-versus-symbol conflicts are checked for every key, layout and modifier set in use. Runtime ambiguity returns `None`.
  - Level3 and Level5 block matching (Smithay maps these to Mod5 and Mod3). Caps and Num Lock are ignored.
  - Keycode conversion is correct: winit reports `key + 8` (`upstream/smithay/src/backend/winit/input.rs:72`), and `input.rs:148` subtracts 8.
  - At runtime, Smithay's `raw_syms` reads level 0 of the active layout (`keyboard/mod.rs:499-503`), which matches the compiler checking every layout's level 0.
  - Consumed keys are tracked by XKB code until release and when focus is lost.

## Limits
- The winit code that fills `WinitKeyboardInputEvent.key` (scancode source, key-repeat filtering) isn't in the snapshot, so I couldn't check it.
- I didn't verify libxkbcommon's C behaviour (`key_for_each` range, group wrapping) or xkeyboard-config's Mod3 mapping.
- Not reviewed at all: Lua startup/reload/effective-config publishing, autostart, capabilities, keymap reload, physical devices, floating windows, global resource timing, and production readiness.
