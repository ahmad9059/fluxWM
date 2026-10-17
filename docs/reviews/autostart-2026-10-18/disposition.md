# Author disposition: bounded autostart

Claude Opus 5.5 inspected frozen `72771fc` using Read/Grep only. The [unedited report](claude-report.md), [raw completion](stdout.json), [command](command.json), [prompt](prompt.txt), [execution](execution.json), [source manifest](input-manifest.json) and [author verification](verified-execution.json) preserve the record. Exit was zero after 242.778 seconds. All 72 input hashes match; the actual trace has 18 read/search calls, no outside-snapshot requests and no permission denials. Claude executed no tests and did not certify production readiness.

## Accepted medium defect

The retained-argv preflight counted replacement sizes rather than the maximum retained prefix. Starting new large identities before shrinking old histories could exceed the promised 8 MiB cap when superseded. The author reproduced this with actual worker publications: restoring the old accounting produces **12,533,976 retained bytes** and fails `superseded_prefix_cannot_exceed_retained_history_budget`. Original/mutated source, command, failed console and exact restoration hashes are in the [evidence bundle](../../evidence/nested-milestone/2026-10-18-autostart/README.md).

Correction `85f030a` counts `max(old_bytes, replacement_bytes)` per restarting identity plus new identities. Removed histories clear before starts. Thus any prefix is covered, including supersession before shrinking entries. This conservative bound can reject a transition whose final state would fit; a separate accepted removal/shrink plan releases history first. The regression pauses the real worker after publications and submits a newer generation before shrinking. It uses failed real exec attempts rather than a fake launcher. This bounds argv-history bytes, not total RSS.

## Diagnostic corrections and additional author checks

For L1, snapshots now distinguish `active_complete` from latest-observed `settled`: a rejected newer plan can settle while the previous active plan remains incomplete. The author also found that the old local settled flag could revert the rejected status after a child exit. `Progress` now retains settled/error consistently, and a controlled test supersedes a partial plan, rejects the newer generation at the identity cap, then verifies subsequent owned-child reaping preserves that rejection. `retained_argv_bytes` exposes actual history and becomes zero after joined shutdown.

For L2, a removed/reintroduced once-only entry reports `once-suppressed` with no deferred-argv claim when no attempted history remains. Both library and private runtime checks assert it.

Other author work after the initial frozen snapshot adds QueueFull/retry and sealed pending work during real TERM retirement, separate named `"0"`/index-zero ownership, failed replacement without retry, strict rejected-generation retention, two private shutdown routes with TERM/SIGKILL assertions, and a real 256-identity budget rejection/removal/recovery scenario. These changes are not relabeled as inspected by the initial reviewer.

## Remaining limits

Root-level QueueFull interleavings, positional insertion/reordering at the whole-compositor boundary, worker panic reporting, arbitrary resource/scheduling pressure, full descendant/session supervision and native/hardware behavior remain unqualified. The private shutdown fixture has one ignoring child; the component test independently uses six and requires one common grace interval. Neither interval nor the test timeout is a universal kernel wait bound. The reviewer did not inspect the complete argv validator, IPC/host-close path or private Case/display helpers. Those limits remain visible despite neighboring passing tests.

The [focused follow-up](../autostart-followup-2026-10-18/disposition.md) accepts the correction and finds no reachable high/medium defect within its read scope. Its low observations, later author changes and uninspected paths remain explicit. The initial finding is preserved unchanged.
