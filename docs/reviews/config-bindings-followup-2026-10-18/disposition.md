# Author disposition: cancellation and XKB trigger follow-up

Claude reviewed frozen `639f6876e1bbc3784d5be41ad4072f52cac57f16` using only Read/Grep. The [report](claude-report.md), [completion](stdout.json), [prompt](prompt.txt), [execution](execution.json), [command](command.json), [manifest](input-manifest.json) and [author verification](verified-execution.json) retain the actual record. Exit was zero after 168.11 seconds; **1,978 hashes match**, with no permission denials, outside-snapshot paths or error result. The actual trace has **15 tool calls**; the report self-describes 11. Claude executed no commands or tests and did not independently run the mutation experiments.

The reviewer found **no new high or medium issue within the corrections**. It checked typed cancellation/reaping, coordinator publication guards, modifier/group probes, keymap borrowing, effective modifier/base-symbol matching and fixed compiler budgets. It did not certify full runtime publication, native input, global latency or production readiness.

## Accepted low gaps and later author verification

The sleeping coordinator oracle could pass if its fake helper exited before shutdown. A later test-only correction, authored as `419aa43` in the isolated configuration worktree and integrated as `ad35841`, resolves and checks the canonical sleep executable, waits for `/proc/PID/exe` to match while the same generation/PID is active, and rechecks immediately before shutdown. Repeating the shutdown-flag mutation passes readiness and fails the sub-second assertion after ten seconds; restored source passes. The test requires Linux procfs and an executable `/bin/sleep`; missing prerequisites fail the test.

The probe tests lacked actions present only with Caps/Num lock or only in group two. Later test-only commit `1666484` adds actual XKB maps/states demonstrating those actions and requiring rejection. Two separate author mutations now prove these tests detect the specific omissions: replacing every lock variant with zero fails the lock test; probing every group as zero fails the second-group test. Sources are restored byte-for-byte and the repaired suite passes.

These test-only changes and their experiments occurred **after** the frozen follow-up. They were not reviewed by Claude. Production code is unchanged from the reviewed follow-up; the final compositor binary hash matches the executable used in all six private suites and all three focus cases. Final author checks pass **142 named Rust tests**, build, Clippy, rustdoc and formatting. See the [raw evidence and exact scope](../../evidence/nested-milestone/2026-10-18-config-bindings/README.md).

## Retained limits

Transition rejection remains fatal even with `RetainInactive`; that policy only tolerates absent base symbols. Unmodelled sticky/latched states and modifiers outside the four schema modifiers plus Caps/Num probes require explicit accessibility/custom-keymap policy. Compiling a binding does not authorize its action, qualify physical hardware or apply user Lua atomically. Those gates remain open.
