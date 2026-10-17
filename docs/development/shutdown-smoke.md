# Bounded IPC quit drain

The nested runtime now gives IPC `quit` a response-drain interval instead of stopping its event loop inside the request handler. A successful quit action advances the sequence and queues its `accepted:true` response. The server then stops accepting connections, reading requests and invoking handlers globally, including later frames on that connection and other peers later in the same poll. It retains only response frames already queued. The listener remains bound until teardown: a kernel connect can still succeed during this interval without becoming an admitted request.

The absolute monotonic deadline is 250 ms from action acceptance. Successful writes may finish earlier. No progress or repeated drain request extends that deadline. At expiry, pending frames are abandoned and counted in final `ipc_drain_timed_out`. The event-loop adapter removes listener/read interests, retains writable interests and arms the absolute deadline even if no client reads or sends again. When no pending frame remains, the runtime stops the loop and performs existing child/socket cleanup.

Receiving `accepted:true` establishes acceptance, not completed process teardown or a peer receipt for other frames. A truncated/lost quit response remains an ambiguous outcome; check instance lifetime before retrying. Frames after quit deliberately receive no response and must never execute. Signal, host-close, fatal-error and exit-timer paths still stop immediately. Worker joins, child reaping, screenshot I/O and native/kernel teardown do not acquire a universal deadline from this IPC change. Full bounded shutdown remains a production gate.

The component API is `Server::poll_controlled` / `EventLoopDriver::poll_controlled` with `Dispatch::Continue(response)` or `Dispatch::Drain { response, deadline }`. Normal `poll` remains compatible. For a shutdown initiated outside the handler, call `begin_drain(deadline)` and reconcile registrations. `drain_state()` distinguishes accepting, pending (remaining bytes/connections/deadline) and drained. A drain transition cannot be reversed. Existing queued frames retain normal framing; unread buffered requests are discarded. An empty response queue means bytes left userspace, not that the peer read them.

Run after building the compositor:

```sh
cargo build -p fluxwm
python3 tools/smoke_shutdown.py
```

The harness uses unique nested instance/socket names, inherited host backend environment without mutation, explicit owned-process cleanup, a 12-second deadline per case, bounded logs/replies and normal shutdown stats. It creates graphical nested windows and injects no input. Run graphical suites sequentially so host tiling does not invalidate unrelated pixel tests. Select `--case pipeline` or `--case slow-peer`; `--binary PATH` selects a preserved executable and `--artifacts NEW_DIRECTORY` retains evidence. Pre/post hashes must match.

| Scenario | Independent oracle |
| --- | --- |
| Pipeline `version`, `quit`, mutating workspace-mode action in one write | Exactly the first two framed replies, no tail reply; final dispatch-after-shutdown counter=0; quit sequence is previous+1; normal process exit and endpoint removal |
| Non-reading peer plus controller quit pipeline | At least 64 KiB of requests sent to the unread peer; final timeout counter ≥1 proves a pending response survived to deadline; controller gets complete quit response; configured drain budget=250 ms, measured drain 240..750 ms (10 ms early measurement / 500 ms late scheduling tolerance), normal exit within the local 2-second tolerance and endpoint removal |

The saturation prerequisite is the observed timeout counter, not merely a sleep or a successful send. The 2-second exit tolerance is a local regression bound, not a hard production latency guarantee. The slow peer is never read, and its replies are intentionally incomplete after timeout. A separate pure transport test supplies a 256 KiB drain response, verifies every byte is flushed with the same ID/value, and proves its pipelined tail never executes. That test does not imply the tiny live quit response was itself backpressured.

## Observed evidence

The preserved pre-change executable SHA256 `703716141bc47e08383bf9227b26f6734b0bf748d247c617320041db8abd7793` returned all three pipeline replies, including `tail` after `quit`, and exited normally. This establishes the ordering defect; it does not reproduce a lost quit response. The first harness version terminated early on assertion failure; that artifact is retained separately, and the corrected harness waits for natural exit before asserting the reply oracle.

The corrected runtime passes both cases. In the initial slow-peer run, 219,264 request bytes were sent without reading; final `ipc_drain_timed_out` was 1, controller EOF arrived in 0.31 ms and normal process exit in 314.71 ms. These are observed timings for one local run, not guarantees. The committed [shutdown evidence bundle](../evidence/nested-milestone/2026-10-17-shutdown/README.md) preserves original failures, final results, exact executable hashes and component checks.

Component tests additionally cover the global same/other-peer dispatch boundary, full queued-frame flush, unextendable deadline, write-only drain interests and a calloop timer that wakes an unread connection and removes all socket/deadline registrations. Whole-loop fairness, repeated signals, host/GPU failure, shutdown under live Wayland resource pressure, descendant/session supervision and native teardown remain unqualified.

## Isolated execution and review corrections

For reproducible graphical runs, use the authenticated private Xvfb wrapper:

```sh
python3 tools/smoke_private.py --suites scene nested shutdown ipc
```

It reuses the [verified package preparation and authentication](private-input.md), launches an owned 3000×2000 X server with TCP disabled, verifies the lock/session PID and runs suites sequentially with a private runtime directory. Each suite has a 65-second wrapper limit and 4 MiB wrapper-log bound, in addition to its own deadlines. Package preparation and bounded teardown are outside those limits. The default server cursor starts outside the 1280×800 nested window, so the fixture can observe the compositor’s unchanged initial (0,0) pointer without any injection. The wrapper changes only subprocess environments and never opens the host display. Its evidence records cookie removal, server reaping, X socket removal and unchanged parent environment.

Archive only `result.json`, public package-verification metadata, per-suite public result/log files and known wrapper stdout/stderr. Do not archive the entire wrapper directory: it contains package/keyring caches and temporarily contains the private authority file. Final cleanup deletes the authority. Large PPMs can remain local with their recorded hashes.

[Claude’s independent source review](../reviews/shutdown-2026-10-17/disposition.md) correctly challenged the original read-only tail and loose exit-only timing checks. The corrected harness requires zero calls into IPC dispatch after shutdown, uses a mutating tail, checks measured drain timing separately from process cleanup, and attempts a mutation on an already-admitted additional peer after quit. Component tests cover previously buffered peer requests and invalid handler responses; the driver test observes the actual deadline callback. Older runs remain historical observations from weaker oracles.

The host scene rerun failed pointer-stationary/output assumptions; its original logs are preserved rather than promoted to passing. The first isolated scene suite passed, then the nested validator rejected null optional timing fields. The serialization fix omits unavailable timings and retains numeric-counter assertions. Isolated X11 results qualify that backend path only; they do not settle host Wayland, physical input or native behavior.
