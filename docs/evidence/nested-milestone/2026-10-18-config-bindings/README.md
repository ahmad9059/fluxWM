# Config coordinator and keymap binding evidence — 2026-10-18

This bundle qualifies reusable preparation components and the nested compositor's development binding table. **It does not apply user Lua at startup/reload, implement effective configuration endpoints, complete phase 6 or establish production readiness.** See [input contract](../../../development/input-bindings.md), [coordinator contract](../../../development/config.md), [initial review disposition](../../../reviews/config-bindings-2026-10-18/disposition.md) and [focused follow-up](../../../reviews/config-bindings-followup-2026-10-18/disposition.md).

## Source and execution identity

[execution.json](execution.json) records final source `ad35841`, component author/integration commits, exact source hashes and command outcomes. Configuration work ran independently in `/home/ahmad/Documents/Projects/fluxwm-config` and was cherry-picked into `impl/input-bindings`. Component branch logs are author records; integrated workspace checks are authoritative for the combined source.

Final build, **142 named Rust tests**, test enumeration, Clippy with warnings denied, rustdoc with warnings denied and formatting pass. Raw [checks](checks.json), [tests](tests.log), [enumeration](test-list.log), [build](build.log), [Clippy](clippy.log), [rustdoc](rustdoc.log) and [format](fmt.log) logs are retained without whitespace normalization. Generated helper subprocess test lines are not additional named workspace tests.

The [whitespace check record](diff-check.json) records eight raw console logs excluded from the otherwise passing staged source/documentation check. Their terminal blank lines and libtest subprocess trailing spaces remain unchanged; the full raw check and exact exclusions are retained.

All six default private suites pass: scene/lifecycle, nested input, shutdown, IPC, shell health and configure scheduling. All three host-focus cases pass: keyboard, pointer and popup. [Regression command](regressions-command.log), [focus command](focus-command.log) and their `regressions/` and `focus/` directories preserve suite records. These ran on source `639f687`; final source changes only two test files, and the rechecked production executable and fixture hashes match their qualified binaries. No GUI rerun is inferred from the later test-only commit. `execution.json` records both identities.

Tests use an owned authenticated private Xvfb, never the host desktop. The runner revalidates its retained signed Xvfb bundle. Cleanup records report removal of owned authority files, server and X socket, runtime endpoint cleanup and unchanged parent environment. This is Linux nested qualification, not DRM/libinput/device/session/seat qualification. `public-origins.json` preserves origins of the initially copied runtime/component records; later correction records have their own source/command metadata. No executable, screenshot pixels, authentication cookie or authority file is included.

## Reproductions and mutation proofs

- `modifier-trigger-baseline/` applies the archived nine-test binding source to compiler `2a4cfb7`. The real `Alt+Shift_L` regression fails because compilation returns `Ok`; source restoration is recorded. This is an actual old-code failure, not a simulated expected error. Later lock/group tests are separate in `final-test-source/`.
- `cancellation-proof/` contains the first two independent cancellation mutations, restored production source hashes and exact archived pre-readiness source/test files. Removing the supervisor's active-loop check returns `Timeout` rather than `Cancelled`; removing the coordinator's shutdown flag waits the ten-second deadline. Both exit 101 at the intended assertions.
- `cancellation-ready-proof/` repeats the coordinator flag mutation with the final live-image readiness test. Readiness succeeds, then the shutdown assertion fails after ten seconds. Repaired component checks pass. The test source in `final-test-source/` is identical to the integrated final test.
- `probe-mutations/` replaces lock probes with zero and, separately, replaces the probe's current group with zero. Each new actual-XKB test fails with `Ok` instead of the required rejection. [Mutation metadata](probe-mutations/mutations.json) identifies exact replacements and source/test hashes; [restoration](probe-mutations/restored.json) and repaired tests pass.
- `parallel-component-checks/` preserves initial separate-worktree test/build records; these do not substitute for combined final checks.

Mutations establish detection of these specific defects. They are not exhaustive fault injection, scheduling proofs or universal shutdown deadlines. Earlier compiler source and test source identities are explicitly distinct from the final snapshots.

## Independent review and remaining gates

The initial Claude read-only review found a medium cancellation-oracle gap; author tests and mutations close that named gap. The focused follow-up found no new high/medium issue within the corrections. Its low live-image and lock/group-only test gaps were fixed **after review** and tested by the author. Raw reports, manifests, actual tool traces and dispositions are retained in the linked review directories; Claude ran no tests. File manifests in each bundle exclude themselves and preserve raw bytes.

Runtime source acquisition, atomic user-config publication/rollback and snapshot retirement, coherent keymap/device replacement, autostart/effective-settings semantics, unsupported actions, native session/input/teardown, custom accessibility policy, global fairness/resource timing, toolkit/protocol conformance, clean-machine CI/packaging, fuzzing and soak remain production gates. The coordinator's one pending request/completion and the compiler's admission budgets are local bounds, not whole-compositor safety or performance claims.
