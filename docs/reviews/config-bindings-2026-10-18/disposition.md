# Author disposition: configuration coordinator and compiled bindings

Claude reviewed frozen `2a4cfb794bb8952436a070ef4017669fae04a90b` with Read/Grep only. The [raw report](claude-report.md), [completion](stdout.json), [prompt](prompt.txt), [command](command.json), [execution](execution.json), [input manifest](input-manifest.json) and [author verification](verified-execution.json) preserve the distinction between review and execution. It exited zero after 290.84 seconds, with **1,977 matching input hashes**, **23 actual tool calls**, no permission denials, no outside-snapshot paths and no error result. Claude executed no tests. This is a focused component review, not production certification.

## Accepted medium finding: cancellation oracle

The original real-Lua shutdown test allowed three seconds while the helper had a two-second CPU backstop. Removing cancellation could still satisfy that assertion. The reviewer correctly identified this masking mechanism. Its reference to a particular termination signal is unnecessary: equal soft/hard CPU limits need not deliver the signal it names.

Later author changes require the real-Lua helper to retire in under one second, add a sleeping-helper supervisor test requiring the exact `Cancelled` result and `waitpid`/`ECHILD`, and add a separate sleeping coordinator test with a ten-second evaluation deadline and a sub-second shutdown assertion. The final sleeping coordinator test verifies the canonical sleep executable through `/proc/PID/exe`, the active generation and PID before shutdown, so early exec failure cannot satisfy the test.

Deliberately removing only the supervisor-loop cancellation check makes the typed test fail with `Timeout`. Removing only the coordinator shutdown flag store makes its sleeping test fail after ten seconds. Both mutations restore production sources in `finally`; repaired tests pass. The retained [evidence](../../evidence/nested-milestone/2026-10-18-config-bindings/README.md) identifies the exact earlier test sources for the first mutations and the later readiness test for the repeated coordinator mutation. Timings qualify this machine, not arbitrary kernel behavior.

## Other findings and limits

The non-aborting native-key iterator was replaced by queries over the schema's fixed evdev domain 1–767 in `5772040`. This bounds compiler enumeration; native keymap construction and global memory/time remain separate limits. Runtime docs now explicitly describe shifted development bindings and the US map's one inactive base-`plus` entry, with `Shift+equal` providing that physical shortcut.

One active helper is allowed to finish before the latest pending source begins. Supersession does not cancel it. Failure kinds are intentionally coarse and source-free: CPU termination can be `Helper`, and excessive result/diagnostic output can be `Invalid`. The post-exit pipe-drain loop remains deadline-bounded but has no universal scheduling/fairness guarantee. These are retained API limits, not silently claimed fixes.

A separate parallel author review identified that `Alt+Shift_L` compiled as reachable although Smithay updates Shift before matching. Author reproduction with real XKB state led to modifier/group-transition rejection in `774a295`, including shifted actions and release-side changes. This finding did not originate in Claude's first report. [Claude's focused follow-up](../config-bindings-followup-2026-10-18/disposition.md) reviews that correction and the cancellation changes.

Runtime Lua startup/reload publication, coherent keymap/device transactions, source acquisition, native inputs, sticky/accessibility behavior and global resource bounds remain open. Review success does not close phase 6 or the production gate.
