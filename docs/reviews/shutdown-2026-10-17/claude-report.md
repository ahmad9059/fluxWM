I found no high-severity correctness regression in the drain logic itself. The main problems are weak tests: two medium gaps in `tools/smoke_shutdown.py`, plus lower-severity test gaps and notes. I did not run any tests, commands or hash checks; this is source reading only and is not a certification.

## What I inspected (11 read/search calls)
- `crates/flux-ipc/src/lib.rs`: lines 480–1045 (`Dispatch`, `DrainState`, `Server`, `begin_drain`, `drain_state`, `readiness`, `poll_controlled`, `flush`, `process`) and the drain tests at 1744–1845. I only grepped the other tests.
- `crates/flux-ipc/src/driver.rs`: lines 1–330 (the wrappers, `reconcile`, `register_socket`, `Drop`, test fixture) and 575–621 (`drain_deadline_wakes_without_reader_and_removes_all_socket_sources`).
- `crates/fluxwm/src/ipc.rs` (whole file), `actions.rs` 1–96, `main.rs` 200–310, and `state.rs` `fail` (305–309) plus where `finish_dispatch` calls `drive_ipc` (694).
- `tools/smoke_shutdown.py` lines 25–152.
- Not read: `input.rs` beyond the Quit keybinding grep, the rest of `finish_dispatch`, the worker and screenshot teardown, calloop's timer internals, and the `IO_BUDGET`/`DEADLINE` constant values.

## Contract checks that hold (confirmed in source)
1. **Nothing runs after the quit response is queued.** In `process` (lib.rs:1020–1031) the response is put in `c.out` first, then the drain flag is set, then the loop breaks, so the rest of that pipeline never reaches the handler. Peers later in the same `retain_mut` pass take the flush-only branch (891–906), which never calls the handler. At the end, `begin_drain` (819–821, 653–664) discards every peer's buffered request bytes and drops peers with nothing queued. Peers processed earlier in the same pass ran before the quit, which matches the contract.
2. **No new reads or accepts during drain.** Accept is skipped when draining (753). In drain mode, `readiness` (686–702) offers only write interest and never the listener. `reconcile` removes any registration that changed (driver.rs:144–154).
3. **The deadline is fixed and absolute.** `begin_drain` can only shorten it (`min`, 648–651). `process` overwrites it at 1027, but that line is only reachable when no drain is active (891), so it cannot extend it. `apply_action` sets `Instant::now() + 250ms` once, and a second Quit is rejected at actions.rs:8.
4. **Peers that never read cannot keep the process alive.** `next_deadline` arms a calloop timer even with no fd activity (driver.rs:170–188). Once the deadline passes, `process` drops every connection and counts any unsent response in `drain_timed_out` (893–897).
5. **The loop stops once drained.** `drive_ipc` calls `loop_signal.stop()` when the state is `Drained` (ipc.rs:51–52). Timer and writable wakes set `ipc_requested` through the wake callback. A keyboard quit sets `ipc_requested` and goes through `begin_drain` (ipc.rs:36–40).
6. **Driver failures don't let a request slip through.** `driver.begin_drain` upgrades the loop handle before it calls `server.begin_drain`. If the upgrade fails, `poll_controlled` fails the same way before it polls, so no request is handled. Registration errors go to `fail()` and stop immediately, as the contract allows for fatal errors.
7. **Signals, host close and `--exit-after` still stop the loop immediately** (main.rs:226–230, 256–264; winit.rs:134).

## Findings

**M1 — medium, weak test (confirmed): the smoke test can't tell "not executed" from "response dropped".**
- Where: `tools/smoke_shutdown.py`, both cases, lines 78 and 108.
- The `tail` request is `version` and the slow peer's requests are `diagnostics`. Both are read-only, so the check `ids == ['before','quit']` only proves the `tail` response never arrived.
- A regression that ran `tail` (or a later peer's queued requests) after quit but threw the response away would still pass. In the slow-peer case, requests from the other peer after quit are never checked at all, because that socket is never read.
- **Fix:** make `tail` change state (for example a `pan` action, or anything that bumps the sequence) and check the final sequence or a dispatch counter in the stats file. Do the same for a second peer that sends its request after the quit.

**M2 — medium, weak test (confirmed): the 250 ms bound is not actually tested end to end.**
- Where: smoke line 112 (`process_exit_ms < 2000`) and line 114.
- `controller_eof_ms` is recorded but never checked. `drain_timed_out >= 1` proves the deadline was reached, not when.
- A regression that set the drain deadline to around 1.5 s, or let something re-arm it, would pass. The unit test `drain_absolute_deadline_cannot_be_extended_by_slow_reader` does cover "can't extend" and the `>=` boundary at the library level, but nothing ties fluxwm's 250 ms to the library.
- **Fix:** add a tighter limit on the time from quit to the slow peer's EOF (or process exit) with a stated tolerance. Or have the stats report the deadline and the time drain finished.

**L1 — low, test gap (confirmed): keyboard quit is untested.** The `begin_drain` path in `drive_ipc` (ipc.rs:36–40) is only exercised through IPC quit. Nothing covers a keyboard quit while a peer has pipelined or buffered requests.

**L2 — low, test gap (confirmed): the library test misses some peer states.** `controlled_drain_stops_pipeline_and_other_peer_dispatch` only covers a later peer whose bytes arrive fresh in the same poll. It doesn't cover a peer that already holds a complete frame (or a pending response) from an earlier poll. It also never reads `other` to confirm it gets EOF with no data.

**L3 — low, mostly unreachable (confirmed): quit can take effect without the drain starting.** If the quit response fails the check at lib.rs:999–1006, or the fallback `encode(...)?` fails, `process` returns an error before setting the drain. Quit's side effects (`shutdown_deadline`, sequence bump) are already applied, and later peers in the same poll still reach the handler. `ipc.rs:43` would then turn the next request into a Drain after running it, so one extra read-only request could execute. With the current `dispatch_ipc` (correct id and sequence) this looks unreachable. Setting the drain before validating, or having `poll_controlled` re-check `shutdown_deadline`, would close it.

**L4 — note (partly inference): peers may see a connection reset instead of a clean EOF.** On Linux, closing a Unix socket that still has unread incoming data makes the other side's next read fail with ECONNRESET once its queued data is consumed. That's inference from kernel behaviour, not this source. Drained or slow peers can hit this, which fits "kernel acceptance ≠ receipt". The smoke test already tolerates it (line 85).

**L5 — note (confirmed): the socket keeps accepting connections during drain.** The listener isn't polled while draining, but the kernel still completes new connects into the backlog. Those clients get no reply until the process exits and have to rely on their own deadlines. This fits the contract and isn't a regression.

**Inference only:** I assumed calloop's `Timer::from_deadline` doesn't fire before the deadline. If it did wake early, `deadline_fired` forces the timer to be re-armed (driver.rs:170), so it wouldn't hang; I didn't check calloop's source. The driver test can't tell whether the wake came from the timer or the writable source, though a client that never reads makes the timer much more likely.

## Limitations
- I did not check how many loop iterations draining takes or whether it is fair across peers (both known-open, and drain flushes at most `IO_BUDGET` per connection per poll).
- Worker, screenshot and native teardown bounds were out of scope and are not universally bounded.
- No hashes were computed, and none of the author's results were relied on.
