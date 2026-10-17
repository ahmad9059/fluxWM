# Independent review: FluxWM autostart (frozen snapshot, read-only)

I only read files inside the frozen working directory. I ran no commands or tests and wrote no files, so I didn't create the plan file that plan mode asks for. This review doesn't certify production readiness.

## Findings

### MEDIUM-1: The 8 MiB cap on retained argv history isn't enforced when a reload supersedes a plan mid-way

**Evidence:**
- **The admission check assumes every slot gets its new value.** For each slot that needs a start, the budget counts `entry.bytes` (the new argv) instead of the slot's current retained bytes (`crates/flux-process/src/autostart.rs:443-463`).
- **A superseded plan stops part-way.** The start loop updates `slot.history`/`slot.bytes` one entry at a time (`:521-523`). When a newer plan arrives it stops early (`:500-502`). Entries it hasn't reached keep their old, possibly large, history.
- **So the retained total can end up above what was checked.** After a partial run it can be any mix of "new history for processed entries" and "old history for unprocessed ones".

**Interleaving** (per-entry sizes match the test at `:887-892`, about 1 MiB each):
1. **Gen 1:** A0–A3 are large. **Gen 2:** A0–A3 become small with restart=false, so their large history is kept, and C0–C3 are added large. Retained is about 8.36 MB, which is the case the existing test accepts at `:913`.
2. **Gen 3:** `[B0..B3 large new, A* small restart=true, C* small restart=true]`. The check counts about 4.18 MB, so the plan is accepted. B0–B3 start first. A reload (gen 4) lands while the A/C entries are still waiting, and the loop breaks at `:500`.
3. Retained is now about 12.5 MB: A and C still hold their old large history, plus B.
4. **Gen 4** repeats the pattern with new D* large entries plus small restart=true versions of A/B/C. It is accepted again and superseded after D* runs. Retained is about 16.7 MB, and so on.

Growth is limited only by the 256-identity cap multiplied by the per-command argv limit, not by 8 MiB.

**Knock-on effect:** a later plan that keeps those identities with restart=false is rejected with a budget error (`:459-462`), even though the plan itself is small.

**Docs that are wrong as a result:**
- `docs/development/autostart.md:37`: "A session retains at most … 8 MiB of last-attempt argv".
- `production-status-2026-10-18.md:16`: "history have numerical caps".

**Suggested fix:** for slots that need a start, count `max(slot.bytes, entry.bytes)`. Then every partial outcome stays within the checked total.

**Missing test:** an accepted plan that gets superseded after its new large entries but before its shrinking restart entries, followed by an assertion on the retained bytes.

### LOW findings (diagnostics only, not reachable defects in behaviour)

- **L1: a partly applied plan is reported as settled after a budget rejection.**
  - **Sequence:** plan N is superseded mid-loop and published with `settled=false` (`:556-557`). Then plan N+1 is rejected for budget and published with `settled=true, active=N` (`:461`).
  - **Result:** N's entries that were never reached are never started until another plan is accepted. New identities from N stay at `never-started`, `desired=false`, `index=None`, because those fields are only set at `:509-511`.
  - **Compositor side:** it reports `settled=true` (`crates/fluxwm/src/autostart.rs:63`).
  - **Assessment:** this matches the documented "no rollback" rule, but `active_generation` suggests N was fully applied.
- **L2: `change_deferred=true` on a suppressed re-add.** When a restart=false identity is removed and then added back, the history is `None` (`:489`), so `:512-513` reports `change_deferred=true`. The docs (`autostart.md:25`) only define that flag for a changed argv. The smoke check at `tools/smoke_autostart.py:111` doesn't check it.

## Checked controls (no defect found)

- **Supersession during retirement:**
  - `restart_pending` is set before `retire` (`:481-483`).
  - It is cleared only on removal (`:488`) or on a start attempt (`:521`).
  - `needs_start` honours it (`:269`).
  - The test at `:843-884` uses a TERM-handler gate. If the timing window is missed, the test fails with attempts=3; it can't falsely pass.
- **Stale plans:** there's no `current()` check between `:434` and `retire`, so a plan that was published and then quickly superseded can still terminate children. That is consistent with "publication precedes effects". It's a limit, not a bug.
- **Once/restart/removal semantics:** `needs_start` (`:266-272`) matches the table in `autostart.md:21-31`. Named and positional identities are distinct enum variants (`:26-29`), so a named `"0"` can't collide with index 0. Schema validation of IDs is at `schema.rs:734-743`.
- **Queues:** there is one pending plan plus one worker-owned discard slot, and anything older is a QueueFull retry (`:176-182`). A ping wakes the retry through `drive_autostart` (`crates/fluxwm/src/autostart.rs:25-52`). Snapshots work as latest-wins.
- **Main-thread disposal:**
  - `autostart_pending=None` on the main thread is never the last reference, because `active_autostart` holds a clone (`config.rs:445`).
  - Retired plans are dropped on the config worker (`config.rs:120-131, 233-234`).
  - Discards and snapshots are dropped outside the lock on the worker.
- **Signal ownership:**
  - `Signals::new` (`main.rs:300`) runs before the worker thread is created (`:363`), so the worker inherits the blocked mask.
  - `pre_exec` unblocks INT, TERM and CHLD (`lib.rs:124-137`).
  - SIGCHLD only sets a reap flag (`main.rs:308-309`). Reaping is targeted `try_wait`.
  - Grepping `state/actions/ipc/config/main.rs` found no `waitpid(-1)`, `SIG_IGN` or `SA_NOCLDWAIT`.
  - `ManagedChild::drop` skips killing on ECHILD (`lib.rs:87-98`).
- **Shutdown:**
  - Quit and `stop()` seal admission (`actions.rs:83`, `state.rs:367-373`).
  - The worker retires all children with one shared grace period (`:559-565`).
  - Join and the final snapshot happen outside dispatch (`main.rs:391`).
  - The signal dispatcher outlives `state` on early error returns.
  - If the worker panics, slots are dropped and their children killed; the compositor sees `Stopped`.
- **Privacy:** errors are static strings and errno; no argv or path is exposed. The SECRET oracles are in both the unit test and the smoke harness. IDs are public by design, as documented.

## Missing tests / oracle gaps (not bugs in themselves)

- The MEDIUM-1 supersession budget scenario.
- A budget rejection that follows a partly applied plan (L1).
- Positional (ID-less) identities at the worker level: insertion shifting identity, which kills the old child and starts the new one. All worker and smoke cases use named IDs.
- `try_install` QueueFull and retry, both in the process crate and through `drive_autostart`.
- The worker-panic → `Stopped` → compositor error path.
- Removal of a restart=false identity during a superseded retirement.
- Smoke harness gaps:
  - No supersession, budget or positional cases.
  - Only one TERM-ignoring child (`b`), so the shared grace is only checked weakly through `shutdown_seconds<2`.
  - The `:107` oracle is permissive.
  - `:129` relies on `Case` pre-initialising `ok`, which I didn't verify.

## Documentation accuracy

- `autostart.md` is accurate apart from the 8 MiB claim (MEDIUM-1). It explicitly says it doesn't cover descendants, readiness, crash respawn, backoff, session bus or logout, and that there is no universal deadline.
- The production-status table (`:9-19`) lists open gates for native/session/device, outputs, render/presentation, transactions/window policy, configuration, autostart/session, security, resources, IPC, shutdown and release. I didn't find any claim that overstates the work, apart from the history-cap wording in row 16.

## Limits of this review

- I executed nothing, so the author's test, clippy, rustdoc and smoke results are unverified.
- I didn't read `validate_argv` limits, `ipc.rs` beyond the call site, the host-close path, `smoke_input`/`smoke_config` helpers, or anything outside the frozen snapshot.
- The SIGCHLD reaper search covered only five compositor source files plus `flux-process`.
- I make no claims about native platforms, hardware or kernel wait behaviour.
