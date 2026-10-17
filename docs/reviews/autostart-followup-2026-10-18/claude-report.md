I found no reachable HIGH or MEDIUM bugs in this snapshot. I used 14 Read/Grep calls inside the snapshot. I ran nothing and wrote nothing, so none of the author's test or mutation results are independently verified.

## MEDIUM-1 (prefix budget): fixed

The preflight at `crates/flux-process/src/autostart.rs:440-468` bounds retained bytes for every partially processed prefix:

- **Identities kept by the plan:** each counts `max(slot.bytes, entry.bytes)` if it will be restarted (`needs_start`), otherwise `slot.bytes`.
- **Removed identities:** count 0. Their history is cleared at `:492-503`, after retirement and before the first start or publish (`:504`).
- **New identities:** count `entry.bytes`.

The restart decision can't change between the preflight and the start loop (`:515`):
- Retirement only sets `restart_pending` on slots that were already being restarted or removed (`:479`, `:488-490`).
- `present` only changes for removed slots.

The only place a slot's bytes change during the loop is `:536`, and each slot then holds either its old or its new value, both covered by the max. So every snapshot published at `:504`, `:567` and `:571` stays within the preflight total, which is at most 8 MiB.

When a newer plan supersedes a partial one, its preflight starts from the actual `slot.bytes`, so supersession chains stay bounded. Rejection (`:463-467`) changes no slots, and SIGCHLD reaping (`:430`) never touches bytes. No overflow is possible: at most 8 MiB of history plus a 4 MiB plan.

The regression test (`:1091`) uses `a0..a3` once-only slots that keep large history while their requested argv shrinks. Under the old accounting that plan would be accepted and `b*` would start first. Now it must be rejected.

## Other checks that passed

- **Progress state:** a rejection sets `settled` and `error` but leaves `active`/`active_complete` alone. Reaping republishes the same progress without changing it. An interrupted plan publishes `settled=false, active_complete=false` (`:569-571`). The test at `:1164` exercises a rejection followed by a real child exit.
- **Admission backpressure:** QueueFull only happens when both the pending and discard slots are full. The worker then always drops the discarded plan and notifies (`:422-426`), so the compositor's retry (`fluxwm/src/autostart.rs:39-51`) can't stall.
- **Once-only readd:** `once-suppressed` is reported with `change_deferred=false` when history is `None` (`:519-527`). Index 0 and named `"0"` are separate identities.
- **Retirement:** after an exited child, `try_wait` keeps returning the cached status. On ECHILD, `ManagedChild::drop` does not signal (`lib.rs:87-98`).
- **Shutdown:** retire, then clear history, then publish the closed snapshot (`:573-582`). `shutdown_autostart` reads that snapshot after the join. Early `?` returns still join through `Drop`. `Fluxwm::stop` and `Action::Quit` both seal admission.
- **Signals:** `Signals::new` (`main.rs:300`) runs before the worker thread is spawned, so the thread inherits the mask. SIGCHLD only sets a sticky `reap` flag, so a wakeup can't be lost.
- **Config ordering:** logical config is published before `schedule_autostart` (`config.rs:444-456`). Generations only increase, so the stale-admission error can't happen.

## LOW observations (no fix required)

1. **`settled` stays false when no plan ever arrives.** If autostart is enabled but the initial config fails, no plan is scheduled and the worker never publishes. The default snapshot has `settled=false`, so `diagnostics.autostart.settled` (`fluxwm/src/autostart.rs:64`) stays false indefinitely. That's ambiguous against the docs' definition, and a harness waiting on it would hang.
2. **Admission error can be overwritten.** The error set at `fluxwm/src/autostart.rs:44-49` would be replaced by the next worker snapshot. In practice that only matters after a worker panic, and then no more snapshots arrive.
3. **Never-attempted identities use up the cap.** Identities are reserved (`:469-473`) before supersession can cut the start loop short. Superseded plans that add new IDs therefore consume the 256-identity session cap even though nothing was attempted. This is consistent with the documented tombstones, but not stated explicitly.
4. **Retirement wait can block.** After SIGKILL, `child.wait()` blocks (`:389`). A child stuck in uninterruptible sleep would stall shutdown. The docs already disclaim this.
5. **Test oracle can reap other tests' children.** `reaped(pid)` (`:642`) calls `waitpid(pid)` from the test thread. If a PID is reused by a child of another test running in parallel in the same process, it could reap that child and cause spurious ownership errors. That's test flakiness, not a product bug.

## Remaining test gaps

- No test where an accepted plan is superseded mid-way while sitting near the 8 MiB cap. The new test only proves the unsafe plan is rejected.
- No chain of three or more supersessions crossing restart, removal and readd.
- No Rust test for how the compositor derives `settled`, or for the non-QueueFull admission error branch.
- No test for the "no plan ever" diagnostic state (LOW-1).
- The controlled worker's `next()` would panic if it ever got a discard notification with no snapshot behind it. No current test triggers that, but it limits which interleavings the harness can cover.

## Limits

- **Docs:** `docs/development/autostart.md` keeps the native, transaction, resource (not an RSS bound), descendant and session disclaimers, and makes no production claim. Security and release gates aren't named explicitly, only implied by "no … production-session pass".
- **Not inspected:** the schema and validation code, and the body of `tools/smoke_autostart.py` (only grepped). I also didn't read every older test in full, and found nothing that executes the claimed private shutdown routes and SIGKILL checks.
