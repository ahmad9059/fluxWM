# Author disposition of the shutdown review

Claude reviewed frozen `a9035088df76475689b7277bf5fd3443b2cf467b` in 162.90 seconds. Exit zero, `is_error=false`, no permission denials. The saved trace records 14 Read/Grep calls; the report's own count says 11. Author recomputation of all 632 input hashes found no mismatches. Claude executed no tests, commands or hashes. Preserve the [unaltered report](claude-report.md), [raw result](stdout.json), [prompt](prompt.txt), [execution](execution.json) and [verified trace](verified-execution.json).

## M1: no reply does not prove no handler execution

Accepted. The original pipeline's `version` tail and unread peer's diagnostic replies exposed response ordering but could not distinguish silent execution from discarded replies. The runtime now counts entry into `dispatch_ipc` after shutdown acceptance in final `ipc_dispatches_after_shutdown`. The guard rejects such requests before method handling, and the live harness requires the count to be exactly zero. Its tail is now a mutating workspace-mode action. An additional already-admitted idle peer attempts a mutation after the controller's quit stream closes; it is either already disconnected or sends without dispatch. The counter covers all peer dispatch entries, including unread peers.

The initial baseline genuinely returned the tail reply after quit. Final baseline evidence reruns the stronger harness against the preserved binary. Component tests separately verify that a later peer with a previously buffered complete frame never reaches the handler and gets only its earlier 16 responses before EOF. This is author verification of the narrow dispatch boundary, not complete shutdown qualification.

## M2: two-second exit tolerance did not test 250 ms drain

Accepted. The runtime now records the configured budget from its actual start/deadline instants and measures elapsed drain time independently. The harness requires budget=250 ms and elapsed≤750 ms, explicitly allowing 500 ms local scheduling tolerance. The unread-peer case additionally requires elapsed≥240 ms and an abandoned pending frame. Overall process exit retains a separate two-second local tolerance. A 1.5-second configured drain can no longer pass because it violates both budget and elapsed assertions. These observed local thresholds are not hard kernel/native latency guarantees.

## Lower findings

- **Keyboard path:** still unqualified by real input. It uses explicit `begin_drain`; IPC tests do not prove keyboard quit with queued peers.
- **Previously buffered peer:** added a real Unix-stream component regression with 17 requests, 16 processed before quit and the final already-buffered request discarded. The earlier responses are read and EOF asserted.
- **Invalid handler response after shutdown effects:** accepted as defense. The global drain seal is now retained immediately upon the handler decision, before response identity/encoding checks. A wrong-ID drain response test proves a later peer cannot dispatch despite the failed response. No normal quit response defect was established in current dispatch code.
- **Timer observation:** the calloop test now explicitly observes the deadline callback's fired flag before reconciliation clears it; writable wakeups alone cannot satisfy this assertion.
- **Reset/backlog:** peer EOF may be a reset after queued data; the live harness tolerates it. The kernel can complete connects to the bound listener while application acceptance is disabled. Documentation states this distinction.

## Author corrections and test limits

The corrections above are in `d40a947` after the frozen review and were not independently re-reviewed. The [shutdown harness](../../development/shutdown-smoke.md) and [evidence bundle](../../evidence/nested-milestone/2026-10-17-shutdown/README.md) identify final checks and exact binaries.

A host scene rerun failed stationary-pointer/output assumptions and is preserved. Isolated X11 execution prevents host cursor/window-manager changes from affecting those oracles; it does not qualify host Wayland behavior. A later isolated nested run caught null optional duration fields violating its numeric-stats contract. Unavailable durations are now omitted, and final suites are rerun without relaxing counter assertions. Failed runs remain in evidence.

Claude found no high-severity regression in the source-inspected drain logic. No whole phase, complete shutdown state machine, worker/screenshot deadline, resource fairness, hardware/native teardown or production readiness is claimed. Parallel agents hit a workspace spend cap; this implementation and follow-through were completed by the root agent locally.
