I found no new HIGH or MEDIUM issues in either correction. I worked only from the frozen snapshot with Read/Grep (11 calls) and ran nothing, so none of the tests or the author's mutation results were checked by running them.

## 1. Config cancellation: no new HIGH/MEDIUM

**Unit test** (`crates/flux-config/src/supervisor.rs:235`)
- The test sets the cancel flag inside `on_spawn`. That callback runs after `Command::spawn`, which only returns once the exec has succeeded, so the flag is set against a real running child. The `kill(pid, 0)` check confirms that.
- In the loop (`supervisor.rs:164-169`), the deadline check comes before the cancel check. If the cancel check were removed, the result would be a `Timeout` after 500 ms. The exact `matches!(…, Err(Cancelled))` would then fail. A CPU-limit exit or helper error cannot pass the test either.
- The ECHILD check is sound. The `waitpid(pid, WNOHANG)` targets one PID, and errno is read right after it. If a parallel test's child reused the PID, `waitpid` would return 0 and the test would fail, not falsely pass.
- One limit: cancellation is requested before the first loop pass, so the unit proves "typed result + reaped". It does not prove that a sleeping child gets interrupted mid-run. The coordinator tests cover that.

**Coordinator tests** (`crates/fluxwm-config/tests/coordinator.rs:137,191`)
- `shutdown` sets `closed`, clears `pending` and stores `cancelled` while holding the lock, then joins the worker (`coordinator.rs:304-318`). Publishing a result requires `!closed`, so a late commit can't land.
- In the real-Lua test, the evaluation deadline is 10 s and the CPU limit is 2 s, and shutdown must finish in under 1 s. If shutdown didn't set the cancel flag, the run would take about 2 s and the test would fail. That fixes the previous review's backstop problem.
- **Limit (LOW):** the sleeping-helper test never checks that the helper is still alive just before `shutdown()`. It doesn't check `/proc/{pid}` or that `status().active` is set. If the helper exits early (for example, `/bin/sleep` is missing, as on NixOS), the test passes without exercising cancellation. The real-Lua test still catches that mutation independently.

## 2. Input trigger probes: no new HIGH/MEDIUM

**The Alt+Shift_L gap is closed** (`crates/flux-input/src/lib.rs:170-210`)
- For each resolved chord, the probe holds the chord's exact modifiers down and locks the key's own layout. It then runs Down and Up through a real `xkb::State` and checks the combined changes for any modifier or layout change.
- Alt+Shift_L is rejected because pressing Shift_L adds Shift.
- A "self" trigger like Shift+Shift_L is also rejected. Pressing the key doesn't change anything because Shift is already held, but releasing it clears Shift, and the probe checks both directions.
- The shifted SetMods(Control) test only fails because the probe holds the chord's modifiers, so it really exercises that path. The F1 SetGroup test also checks a real state change.

**Ordinary chords still work**
- The development defaults use arrows, h/j/k/l, Return, c, s, equal, minus, plus and q (`input.rs:386-426`). None of these change modifier or layout state in standard layouts, including with Caps or Num locked.
- Lock modifiers, missing modifiers (`checked_shl` resolves an invalid index to 0) and absent chord modifiers are all rejected explicitly.

**Keymap ownership is consistent**
- `state.rs:248-257` compiles against the seat keyboard's keymap while holding its lock, and keeps only plain Rust tables.
- At runtime, `raw_syms` gives the base-level symbols of the current layout and the modifiers come from Smithay's effective state, which matches how bindings were compiled.

**Budgets are bounded**
- The scan covers 767 codes × at most 32 layouts × at most 16 modifier combinations.
- The probe runs only for owned chords, which are capped at 65,536 before probing, with 4 lock variants and 2 key updates each.

**Limits (LOW, not findings)**
- **Test gaps:** no test has an action that appears only under Caps/Num lock or only in group 2. A mutation that probes only the unlocked state, or always passes layout 0 instead of `layout`, would go undetected.
- **Fatal rejection:** a probe rejection is fatal even with `RetainInactive`, because `?` in `state.rs:257` stops startup. Missing symbols are only marked inactive. This is unlikely to trigger with the current defaults.
- **Unmodelled states:** the probe doesn't cover modifiers outside the schema (Mod3/Hyper, ScrollLock) or latched/sticky state. Sticky/accessibility policy is already listed as open.

## Out of scope
As stated, these remain open: full runtime Lua publication, native/device handling, keymap reload, sticky and accessibility policy, and global wall-clock bounds. None of this certifies production readiness.
