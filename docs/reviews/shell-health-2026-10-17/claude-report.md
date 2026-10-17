I've finished reading and am writing up the results. I found no new high or medium bugs in the health code itself. There are three medium gaps in the tests and docs: the live check can't tell the 2s/5s values apart from shorter ones, nothing tests disconnect, and nothing tests the "no redraw" claim.

**Not certified.** I ran no commands, tests, builds or hash checks. Everything below comes from 10 Read/Grep calls on the snapshot. I haven't verified the author's run results or anything in the evidence bundle. You'll recompute the frozen hashes.

## What I read

- `crates/fluxwm/src/health.rs`: all of it, including the unit tests.
- `handlers/xdg_shell.rs:38-50`: the callbacks for a new binding, a pong and a destroyed binding.
- `state.rs`: lines 680-747 (where the monitor is polled), `bump_sequence`/`request_redraw` (302-316) and the grep hits around where the monitor is created (185-188).
- `ipc.rs:55-115`: capabilities, `windows.list` responsiveness and diagnostics.
- Fixture: the ping handler (`main.rs:622-653`) and `shell_health` with its helpers (2427-2695).
- `tools/smoke_health.py` and `docs/development/shell-health.md`: all of both.

Not read: the main loop that calls `refresh`, `fail()`, the fixture's `until`/`observe_for`/`Diagnostics`, `smoke_private.py`, the evidence bundle, and Smithay source.

## New findings (medium)

**M1. The live check doesn't pin the 2s timeout or the 5s re-probe.** `fluxwm-fixture/src/main.rs::shell_health`
- The fixture never checks `timeout_ms == 2000` or `interval_ms == 5000`.
- It only checks after the fact: "unresponsive" after waiting 2200 ms (lines 2584-2604), and exactly 2 pings to the first binding over 5200 ms (2655-2672).
- **Trigger:** set `TIMEOUT` to about 1s or `INTERVAL` to about 3s. Both live cases still pass. A 3s interval gives recovery+3s, then the next ping would land at +6s, after the 5.2s window ends.
- The unit tests in `health.rs` use the `TIMEOUT`/`INTERVAL` names rather than the numbers, so they pass too.
- So the docs' "two-second deadline / five seconds after the pong" is only shown as "within the window". The ping and pong are real, but the timing values aren't proven.
- **Closure test:**
  - Assert the diagnostics fields are 2000 and 5000.
  - About 1.8s after the first ping is received, expect status `waiting` and `timeouts == 0`.
  - After the late pong, expect `health_serials.len() == 1` at about 4.7s and `== 2` by about 5.3s.

**M2. Removing records on disconnect is claimed but never tested.** Doc line 17, plus `smoke_health.py:77-78`
- The fixture explicitly destroys every window and binding before it exits (lines 2543-2549 and 2675-2679).
- So the smoke script's `after_disconnect` check, empty clients and no timer, only re-proves explicit destruction.
- **Trigger in the code:** this depends on pinned Smithay calling `client_destroyed` when a connection drops, and that code isn't in the snapshot. If it doesn't, the only cleanup is `poll`'s `retain(alive)`, and `poll` runs only when `health_requested` is set or the timer fires.
- If every remaining entry had already timed out, there is no deadline and so no timer. A dropped connection would then leave a stale `unresponsive` record in diagnostics, and a used capacity slot, until some unrelated shell event happens.
- **Closure test:** a fixture mode that exits or kills itself with live bindings while the first binding has already timed out. Then require `clients == []` and `timer_armed == false` within a bounded time, with no other client activity.

**M3. "Timer/pong handling requests no scene redraw" isn't tested.** Doc line 13
- Reading the code, the claim holds: the poll branch at `state.rs:707-716` calls only `bump_sequence`, and neither path touches `request_redraw` or `dirty`.
- But no oracle compares redraw or frame counters across the 2.2s silent window or the late pong. The only check is that `wayland_requests` didn't change.
- **Closure test:** snapshot `runtime` render/redraw counters before and after the timeout and the recovery, with no mapped windows, and require them unchanged.
- I'd rate this low-medium, since the code does match the claim today.

## Things I checked that hold up

- **Deadline boundary:** `expire` uses `>=`, and `pong` folds in a deadline that has passed but hasn't been serviced yet. The timeout is counted once, with no double count whether the timer or the pong gets there first. An expired probe has no deadline, so no re-ping until a valid pong.
- **Pong handling:** a pong only does something from `Pending`. It sets `Ready(now+INTERVAL)`. A pong in any other state is a no-op that doesn't change the sequence.
- **Single shared timer:**
  - The timer is re-armed only when the earliest deadline changes.
  - A timer that has fired is cleared through the `fired` flag before the deadline is recomputed.
  - The callback returns `Drop`, so a stale token is never removed twice.
  - With no deadline, no timer is armed.
  - Removing the old timer before inserting a new one means one timer at most.
- **Ownership is per binding, not per connection:** records are keyed by `ShellClient` (the `xdg_wm_base` resource). `remove` affects only that binding. The fixture shows two bindings on one connection being tracked independently.
- **Capacity:** dead entries are pruned before the 1024 check. A rejected binding is counted, never added later, and its role reports `unmonitored`. This limits only the monitor's own vector, and the fixture shows an overflow binding can still map and get a frame.
- **Loop handle and drop:** only a weak handle is kept. `Drop` removes the timer if the loop is still alive. The unit test covers the loop being dropped first.
- **IPC sequence:** `poll` runs in `refresh` before `drive_ipc`, and `bump_sequence` doesn't redraw. Capabilities report `unresponsive_clients_killed:false`, and nothing in the health path destroys clients or posts protocol errors.
- **Wrong-serial and replayed pongs:** the fixture tests these against live behaviour. Whether they're rejected depends on Smithay, as you said.

## Remaining limits (low, or already known)

- **Extra sequence bumps:** `add` sets `dirty` even when it rejects a binding for capacity, and a timer firing alone sets `changed`. Both line up with real changes in diagnostics or stats, so they're probably acceptable, but they're broader than "actual transitions".
- **The sequence oracle** (`main.rs:2606`) only checks that the sequence went up, not that health was the cause.
- **Timer errors are fatal:** a timer-insert or handle failure goes to `fail()`, which makes this diagnostic feature fatal to the compositor. In practice that only happens if the loop is dropped.
- **`Unavailable` is permanent** once a ping fails to send. That's fine if Smithay only fails sends for dead resources, which I couldn't check without its source.
- **The capacity mode doesn't check that monitored bindings were actually pinged** (`stats.sent`).
- **Already known and not counted as new:** quotas per protocol object or client, native/suspend, backpressure and fairness, serial wraparound, multiple connections, destruction exactly at a deadline, and production readiness.
