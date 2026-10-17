# Author disposition of the shell-health review

Claude reviewed frozen `a5b48da6f9872c31d25c552caef2d1ca1c2a169b` in 134.24 seconds, exit zero, `is_error=false`, no permission denials. The actual trace records 11 Read/Grep calls; the report says 10. Author recomputation of all 927 frozen file hashes found no mismatches. Claude executed no tests, builds or hashes. Read the [unaltered report](claude-report.md), [raw result](stdout.json), [prompt](prompt.txt), [execution](execution.json) and [verified trace](verified-execution.json).

## M1: live timing bounds were insufficient

Accepted. The original observations after 2.2 and 5.2 seconds did not distinguish the documented constants from some shorter policies. The fixture now asserts `timeout_ms=2000` and `interval_ms=5000`, observes no timeout/still-waiting after 1.8 seconds, then real timeout after another 0.4 seconds. After recovery it requires only the original primary ping at 4.7 seconds and exactly one fresh primary ping after a further 0.5 seconds. The assertions use independent numeric expectations rather than the implementation constants. Controlled local execution passes; these windows are test tolerances, not production scheduling guarantees.

## M2: explicit destroy did not qualify disconnect cleanup

Accepted. A third fixture now exits with a still-live unresponsive binding after its timer is gone. It sends no shell destroy request. The parent waits for fixture process exit and then uses only IPC observations to require empty monitor/no timer within 500 ms, including time spent waiting on IPC responses. In the first corrected run cleanup was observed in 0.4 ms. Since there is no remaining health deadline or unrelated shell activity, this covers the callback cleanup path rather than relying on a periodic prune. It does not cover every destruction/deadline ordering, abrupt crash or multiple connections.

## M3: no-redraw claim lacked an oracle

Accepted. The fixture compares `redraws`, `compositions`, `submissions` and `frames_sent` across the no-window timeout interval. It still proves a responsive sibling maps and receives a frame while the primary is unresponsive, then destroys that role, waits for the scene to settle and compares the same counters before/after the late pong. All counters remain unchanged across those named health transitions. Missing counters fail the oracle. This is virtual nested-render evidence, not physical host presentation.

## Other observations and limitations

- The capacity case now requires 1,024 actual sent pings and matching pongs, alongside its record/rejection counts and a usable unmonitored overflow role.
- Pure probe tests still cover exact deadline equality and a late pong before timer servicing. They supplement, rather than substitute for, live timing tests.
- The source review confirms one shared timer, weak loop ownership, per-binding identity, sequence changes without requesting redraw and nonfatal timeout policy. Invalid/replayed pong behavior is author-checked against pinned Smithay source and real protocol tests; Claude did not inspect upstream source.
- A full Rust test build after the frozen review found two fixture unit initializers missing the new diagnostics sequence field. `4a510b2` fixes them. The reviewer did not build the snapshot. This compiler failure is retained separately from final checks.
- A ping-send failure remains unavailable rather than retrying blindly. Timer registration failures still fail this owned nested instance. Production recovery policy remains open.

Later fixture/harness corrections are author work after review and were not independently re-reviewed. The [feature documentation](../../development/shell-health.md) and [evidence bundle](../../evidence/nested-milestone/2026-10-17-shell-health/README.md) preserve exact final source, command and binary identities. No whole phase, global resource cap, hostile-client fairness, suspend/native policy, serial-wrap/destruction matrix, actual application health or production readiness is certified.
