# Author disposition: autostart budget correction follow-up

Claude Opus 5.5 independently inspected frozen `85f030a`. The [unedited report](claude-report.md), [completion](stdout.json), [command](command.json), [prompt](prompt.txt), [execution](execution.json), [input manifest](input-manifest.json) and [author verification](verified-execution.json) retain the record. Exit was zero after 183.412 seconds. All 72 input hashes match; the actual trace contains 14 Read/Grep calls, no outside-snapshot paths and no permission denials. The reviewer executed nothing and did not independently verify author test or mutation runs.

The review finds **no reachable high- or medium-severity defect within its scope** and accepts the conservative prefix-budget correction. It checks maximum old/replacement accounting, removal before new starts, actual history in later preflights, retained progress after rejection/reaping, queue notifications, once-only suppression, shutdown history release, signal setup and logical-publication ordering. This does not certify the complete compositor or every integration path.

## Low observations and later author work

- **Idle without a submitted plan:** enabled autostart previously remained unsettled after failed startup config. The later author change treats a missing requested generation as idle/settled. The private disabled and invalid-startup cases assert the corresponding empty diagnostics explicitly.
- **Never-attempted reservations:** accepted plans reserve identities before executing. Supersession can consume history slots without an attempt. Documentation now states this, and the later near-cap test verifies that reusing such an identity still permits its first attempt.
- **Test reaping:** the original `waitpid(pid)` ownership assertion could consume a different parallel test child's status after PID reuse. The later helper uses `waitid` with `WNOWAIT`; it observes ownership without consuming another exit. PID reuse could still make an assertion fail, but it cannot steal that child from its owner.
- **Near-cap accepted chains:** a new author test accepts an eight-large-history prefix near 8 MiB and supersedes three successive plans across restart/removal/readdition. It checks every captured prefix remains bounded, once-only suppression and the first actual attempt of a reserved identity. This complements the reviewed rejection test.
- **Admission-error overwrite / blocking retirement wait:** retained as limitations. Non-QueueFull admission failure is not a normal increasing-generation path; worker-panic reporting remains unqualified. Kernel wait after SIGKILL has no universal deadline and is documented accordingly.

These low fixes and additional tests occurred **after** the frozen follow-up review and are author-verified rather than independently reread. No third review is implied.

## Remaining scope gaps

The reviewer did not read schema/validation or the full private autostart harness, and did not independently establish the executed shutdown/SIGKILL routes. The [author evidence](../../evidence/nested-milestone/2026-10-18-autostart/README.md) retains their actual commands, statuses and oracles separately. Root-level QueueFull/error branches, arbitrary notification/discard interleavings, worker panic and whole-compositor positional reorder remain unqualified. The controlled test harness expects a snapshot notification and is not a general discard-notification driver. Native/device/output, full transaction, desktop security, session/descendant, total resources/fuzz/soak and release gates remain open.
