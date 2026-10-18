# Autostart ownership evidence, 2026-10-18

Final implementation source: `11fc6b2`, following initial review `72771fc`, independently reviewed budget correction `85f030a`, and later author low fixes/accepted-chain tests. Current runtime contract is [autostart.md](../../../development/autostart.md); [production gaps](../../../development/production-status-2026-10-18.md) remain open. This is a nested development milestone, not a production window manager or a complete phase-6 pass.

## Executed checks

`checks/checks.json` records eight exact commands and exit statuses: workspace tests, registered test inventory, Clippy with denied warnings, rustdoc with denied warnings, formatting, documentation, source whitespace and final workspace build. `checks/test-inventory.json` matches **175 registered tests to 175 individually observed successful names**, without counting repeated subprocess fixture tests again. Raw libtest bytes are preserved. The final build occurs after test/profile checks; actual executable hashes are in `checks/executables.json`.

`runtime/execution.json` records five serial private-display harnesses and unchanged executable hashes. All pass:

- Three autostart scenarios: repeated named lifecycle with SIGTERM and IPC quit, plus full 256-identity session-budget rejection/removal/recovery. Real companion files record launch identity, selected display/socket environment and unblocked signals. Shutdown requires TERM receipt and SIGKILL fallback for the ignoring companion, no surviving observed process identity, closed final snapshots, zero retained history and endpoint cleanup.
- Six existing configuration scenarios, including invalid candidate isolation and safe publication boundaries.
- Six real SHM windows with configured widths 400/760/320/600/600/480, retained IDs, wire configure sizes and initial frames.
- Six default private compositor suites: scene, nested lifecycle, shutdown, IPC, responsiveness and configure scheduling.
- Three host-focus cases with owned private input injection.

Every display is an authenticated owned Xvfb instance whose signed bundle is reverified by the harness. Recorded cleanup includes authority/server/socket removal, runtime endpoints before directory deletion and unchanged parent environment. These checks do not inject host-session input. ELF files, screenshot pixels, authentication cookies/authority files and full environments are excluded from this committed subset. Selected raw artifacts and their original paths/hashes are recorded in `public-origins.json`; producer scripts and relevant harness sources are retained.

## Deliberately failing proofs

`retirement-mutation/` removes only the pending-restart term from the strengthened source. A real TERM-handler gate admits newer plans during retirement and the test fails because the restored original command has no running PID. Source is restored byte-for-byte. This experiment predates the later review correction; its exact original/mutated source copies and hashes identify that producer rather than implying it used the final binary.

`budget-mutation/` restores replacement-only byte accounting in the corrected source. The controlled worker records a retained prefix of 12,533,976 bytes and fails the 8 MiB assertion, then restores the original source exactly. The normal corrected suite passes. The test's publication acknowledgment gate controls a real worker interleaving; it does not substitute a fake launch implementation. These failures are intentional fault proofs, not successful production qualification runs.

## Independent review and limits

The [initial Claude review and disposition](../../../reviews/autostart-2026-10-18/disposition.md) preserve the medium budget finding and later author correction. The [focused follow-up](../../../reviews/autostart-followup-2026-10-18/disposition.md) accepts the corrected prefix bound and finds no reachable high/medium defect within scope. Both snapshots have 72 matching input hashes and actual traces of 18/14 calls. Later idle diagnostics, observation-only test ownership checks and the accepted near-cap three-supersession test are author work. Read-only review is separate from author execution. Native/physical presentation, full protocol/toolkit/input matrices, device/output transactions, secure desktop integrations, process-tree/session policy, fairness/exhaustion/fuzz/soak and clean release packaging remain open. A small passing scenario set cannot establish completeness.

The manifest seals selected final bytes excluding itself. Raw-console whitespace exclusions, if any, are recorded separately rather than normalizing evidence.
