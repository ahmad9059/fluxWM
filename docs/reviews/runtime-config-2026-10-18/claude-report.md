# Independent review of frozen commit 96138f8: runtime config integration

**Bottom line:** I found no HIGH and no reachable MEDIUM code defects in the paths I read. There are two MEDIUM test gaps: some claimed guarantees exist only in code and no test checks them. I used Read/Grep only (21 calls) and ran nothing, so none of this is runtime evidence.

## Inspected controls (code reading)

| Claim | Where it's enforced | Verdict |
|---|---|---|
| Stale or late completions can't publish | The worker's `publish` checks `closed` and `latest == generation` under the lock (`config.rs:282`). `completion()` only takes the slot if `.0 == c.latest` (`config.rs:107`). `request()` advances `latest` under that same lock. Superseded completions are dropped on the worker before the next `request_path` (`config.rs:197-203`). | Holds |
| Inner and outer generations line up | The inner `take_completion` only returns completions for the inner `latest` (`coordinator.rs:349-352`), and the outer code also requires `inner == result.generation`. | Holds |
| Notifications and locks don't race or deadlock | The inner `notify` runs outside the inner lock (`coordinator.rs:275`) and only takes the outer lock. The outer worker never holds its lock while calling `request_path` or `take_completion`. Wakeups aren't lost: flags are re-checked in the wait predicate, and a notify that arrives mid-iteration re-arms `evaluation_ready`. | Holds |
| Retired objects are dropped on the worker | There is one `retired` slot. `completion()` refuses while it's occupied. Only dispatch fills it, right after a successful `completion()`, so the `assert!` at `config.rs:120` holds. The worker pings after dropping (`config.rs:191-194`), so a completion held back behind the slot is re-driven. | Holds, with the limit below |
| Stop sealing (SIGTERM/SIGINT, `--exit-after-ms`, host close, `fail`, sequence exhaustion) | All of these go through `stop()` (`main.rs:306,338`, `winit.rs:135`, `state.rs:340,361`). `drive_config`, `request_config_reload`, `apply_action` and `drive_ipc` all check `stopping`/`shutdown_deadline`. Final `finish_dispatch` is sealed too. Shutdown joins happen after `event_loop.run` returns (`main.rs:385`). | Holds |
| Swaps happen only between input batches | Application waits while keys are pressed, keyboard/pointer grabs are active, or `pointer_presses` is non-empty. On host focus loss, pressed keys and buttons are cancelled (`input.rs:28-68`), and Smithay drops synthetic winit key events (`upstream/smithay/src/backend/winit/mod.rs:561`), so a key stuck in "pressed" can't block application forever. `suppressed_keys` doesn't depend on the binding table. | Holds |
| Path bounds and file handling | Path must be absolute, non-empty, ≤4096 bytes and contain no NUL. The file is opened `O_NONBLOCK\|O_CLOEXEC`, symlinks are followed on purpose, `is_file()` is checked on the opened descriptor (so FIFOs and directories are rejected without blocking), and reads are bounded by `len` plus `take(MAX+1)` with a re-check. | Holds |
| Gap limits | The schema allows 0–4096 and rejects non-finite values (`schema.rs:438,547`). The worker rejects >1024 (`config.rs:238`). `set_gap` re-checks the range (`flux-layout/src/lib.rs:326`). | Holds |
| Layout rollback | `reconfigure` builds a candidate clone, validates the strip extent and checks placements in both modes, and only then replaces itself (`lib.rs:257-281`). On failure, `drive_config` retires the clone and leaves the live layout and bindings untouched. | Holds by code reading |
| Native objects | Keymap text is serialized under the Xkb lock, and the worker builds its own context and keymap. No native keymap or device replacement happens. | Holds |
| Unsupported settings reported | The `effective_config` list matches your stated set exactly, and the smoke test asserts that set. | Holds |

## MEDIUM: test gaps

**M1. No test reaches the live-geometry rollback in `drive_config`.**
- **Path:** the `reconfigure().is_err()` branch at `config.rs:350-361` ("configuration layout would invalidate live geometry").
- **Why it isn't covered:** every smoke case runs with no mapped windows, and the 2048 gap case is rejected earlier by the worker's 1024 check. The oracle at `smoke_config.py:292` only checks that `last_error` is truthy, so it can't tell which check rejected it. Nothing tests the 1024/1025 boundary either.
- **Covered only by:** `flux-layout` unit tests (`lib.rs:1538,1562`).
- **Closure test:** map a fixture window with a wide fixed column, reload with a gap ≤1024 that breaks the strip extent, then assert the exact `last_error` string and that workspace geometry is unchanged. Add 1024 (accepted) and 1025 (rejected, with the nested-limit message).

**M2. No test checks the supersede and stale-publication gates.**
- **Path:** `publish` (`config.rs:282`), `completion()` (`:107`), and the stale-completion drop (`:197`).
- **Why it isn't covered:** each smoke reload waits for a terminal status before the next one starts, so a prepared generation N is never superseded by N+1. If either `latest` check were removed, the suite would still pass.
- **Partial coverage:** the unit test `termination_seals_ready_config_publication` uses a fake worker and an injected completion. It proves only the `stopping` gate in `drive_config`, not the worker-side `closed` check and not the real SIGTERM, timer or host-close event sources.
- **Closure test:** hold Alt so N becomes prepared, start reload N+1 with a different gap, release, and assert that only N+1's values are ever observed and N is never `applied_generation`. Separately, reload twice back-to-back with no wait.

## LOW and hypothetical (not reachable defects)

- **Inner worker death leaves a generation pending.** If the inner `flux-config` worker died mid-job, the outer generation would stay `pending` with no error: `completion()` only detects outer worker death (`config.rs:104`). The only panics on the production path are the piped-stdio `expect`s (`supervisor.rs:155-157`), which are effectively unreachable. The next reload would then fail with "could not be admitted".
- **A new reload doesn't cancel the running one.** `cancelled` is only set at shutdown, so reload N+1 waits for helper N to finish (bounded by `HelperLimits`). This is a latency cost, not a correctness problem.
- **Some layout drops still happen on dispatch.** `reconfigure`'s `*self = candidate`, and the candidate it discards on error, are dropped on the dispatch thread. That's O(windows) and fits your stated on-dispatch budget, but the comment at `config.rs:16` overstates what is destroyed on the worker.
- **No `O_NOCTTY` on the config open.** If the config path is a symlink to a tty and fluxwm is a session leader without a controlling terminal (the smoke runs it with `start_new_session`), the open could acquire that tty as controlling terminal. This needs a hostile or misconfigured user path. It's a hardening note only.
- **Untested deferral and file cases.** Held-pointer and grab deferral have no test; only held Alt does. Symlink, FIFO, directory, oversized-file and long-path cases have no test; only a missing path does.

## Scope limits

- I ran no commands, tests or builds, and didn't look at the author logs. Statements about the smoke test are about what its oracles would prove if it passes, not evidence that it passed.
- I only skimmed `ipc.rs` and `winit.rs` around the config, stop and focus call sites. I didn't read the `supervisor.rs` helper protocol, `BindingSet::compile`, or the schema beyond the gap and range checks.
- Smithay was checked only for `pressed_keys`, `is_grabbed` and winit's synthetic-event filtering. I didn't inspect xkbcommon.
- Not assessed, as you specified: full schema support, resource or timing bounds, filesystem consistency, kernel wall-time behaviour (blocking reads and joins can still delay shutdown, as `coordinator.rs:197` documents), native backend, soak testing, or production readiness. Phase 6 remains unfinished.
