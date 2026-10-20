# Independent Claude review

[Autostart review](autostart-2026-10-18/disposition.md) identifies a real medium retained-history budget defect in `72771fc`. The author reproduces a 12,533,976-byte prefix, corrects the accounting, and retains the failed mutation. The [focused follow-up](autostart-followup-2026-10-18/disposition.md) accepts corrected `85f030a` and finds no reachable high/medium defect within scope. All 72 hashes match in each snapshot; actual traces have 18/14 calls. Later low corrections and near-cap chain tests are author work; neither reviewer executed tests or certified production readiness.

[Configured-width review](default-width-2026-10-18/disposition.md) inspects frozen `ccfdb61`, finds no reachable high/medium defect in its scope, and identifies low comment/Canvas-contract/rounding issues. Later author corrections and stronger wire checks remain distinct from review. All 72 input hashes match; the actual trace has 18 calls. Real six-window and two deliberately failing mutation records are linked separately.

[Runtime configuration review](runtime-config-2026-10-18/disposition.md) reads frozen `96138f8` and identifies two medium oracle gaps: live-geometry rollback and prepared supersession. The [focused follow-up](runtime-config-followup-2026-10-18/disposition.md) reads `0d12c40`, accepts those corrections and finds a further binding-swap oracle gap. Later author-only three-to-four-entry testing detects a deliberately omitted reload swap. The reviews find no reachable high/medium product defect within their scopes. All 61/68 input hashes match; actual traces contain 20/16 calls. Neither reviewer executed tests or certified production readiness.

[Configuration/binding review](config-bindings-2026-10-18/disposition.md) inspects frozen `2a4cfb7`, identifies a cancellation-oracle gap and records later typed/sleeping helper tests with two independent failing mutations. Its 1,977 input hashes match and actual trace has 23 calls. The [focused follow-up](config-bindings-followup-2026-10-18/disposition.md) inspects `639f687`, finds no new high/medium issue within the corrections, and identifies low test gaps. Later author-only live-image and lock/group tests detect their named mutations. All 1,978 follow-up hashes match; actual trace has 15 calls. Neither reviewer executed tests or certified production readiness.

[Claude’s configure-stream review](configure-stream-2026-10-17/disposition.md) inspects frozen `0dba143` and identifies static rejected-buffer recovery. Its disposition records a real failure on that executable and later automatic admission without a new surface commit. The actual trace has 23 read/search calls and 1,662 matching input hashes. Author tests and later source changes are separate. The [focused follow-up](configure-stream-followup-2026-10-17/disposition.md) inspects final `6435fcb`, finds no new high/medium issue within that correction, and retains retry-cost/ordering/fairness limits. Its actual trace has 13 calls and all 1,662 hashes match.


## Latest host-focus reviews

[Initial Claude report](host-focus-2026-10-17/claude-report.md) reviews `9c4c733`; its [disposition](host-focus-2026-10-17/disposition.md) distinguishes a disputed DnD reachability inference from accepted ownership/popup test gaps. The [follow-up](host-focus-followup-2026-10-17/disposition.md) reviews `d91db8e`, accepts further ownership/denial controls and rebuts a redraw-path assumption with source and mutation evidence. Both frozen inputs have 1,188 matching hashes. Author tests and later changes remain separate from read-only review.

## Shell-health review

[Claude’s shell-health report](shell-health-2026-10-17/claude-report.md) inspects frozen `a5b48da`. Its [disposition](shell-health-2026-10-17/disposition.md) accepts three timing/disconnect/redraw oracle gaps and records later author verification. All 927 frozen file hashes match. Actual trace records 11 calls; no tests were executed by Claude.

## Shutdown review

[Claude’s shutdown report](shutdown-2026-10-17/claude-report.md) inspects frozen `a903508`. The [disposition](shutdown-2026-10-17/disposition.md) records two accepted oracle gaps, author corrections and remaining teardown limits. All 632 input hashes match; actual tool trace records 14 calls. Claude executed no tests.

## Scene follow-up

[Claude’s follow-up report](scene-followup-2026-10-17/claude-report.md) reviews frozen `adfa7a7`. Its [disposition](scene-followup-2026-10-17/disposition.md) records a real oracle gap, a release-only mutation that exposes it after correction, and constructor cleanup fixes. Author hashes of all 331 frozen files match. Later corrections and test results are explicitly author work.

## Shared-scene review

[Claude’s focused scene report](scene-2026-10-17/claude-report.md) reviews frozen `e7059a7` using 12 read/search calls. The [author disposition](scene-2026-10-17/disposition.md) records a reproduced pointer-focus bug, a missing popup clip/paint/picking oracle, later fixes and exact real-test limits. All 327 frozen input hashes match. This review does not certify the complete compositor.

## Earlier runtime implementation review

[Claude Opus 5.5 runtime report](runtime-2026-10-17/claude-report.md) reviews frozen `4a87922` using 29 read/search calls. The [author disposition](runtime-2026-10-17/disposition.md) checks its four medium findings, records an actual pointer-feedback failure/pass and later source fixes, and preserves unqualified popup cases. All 185 frozen file hashes match. Neither review nor subsequent tests certify production readiness.

## Earlier implementation audit

[Claude Opus 5.5 report](claude-implementation.md) independently spot-checks frozen implementation snapshot `c2abdb5`. It broadly confirms the author audit and adds four medium findings, which remain open. Read the [disposition](implementation-disposition.md), [raw result](claude-implementation-result.json), [execution metadata](claude-implementation-execution.json), [prompt](implementation-focused-prompt.txt) and [frozen checks](implementation-frozen-check.json). Claude read source and author logs; it did not execute checks or certify production readiness. The first attempt timed out without a report and is recorded separately. Later author corrections are explicitly identified.

## Historical design-only reviews

The user requested independent Claude verification after the deeper audit. The locally installed Claude Code CLI completed a read/search/web-only review with no command/write tools, no permission denials, and no spawned subagents.

- [Exact review prompt](claude-review-prompt.txt)
- [Unedited initial report: 15 findings](claude-review.md)
- [Raw completion/model metadata](claude-review-result.json)
- [Finding-by-finding disposition](disposition.md)

The report records files inspected, actual sources fetched and source-access limits. It correctly challenged impossible phase gates and inconsistent trust/capture contracts. Those contracts were revised rather than waived. Its individual finding severities, not its inconsistent headline count, are used in the disposition.

The initial worktree changed during review; the reviewer disclosed that and reread noticed changes. A follow-up uses a file-hash snapshot of remediation input to avoid that ambiguity. Neither review can certify hardware behavior or production readiness without a compositor implementation.

## Completed follow-up and final outcome

Read the [final summary](final-summary.md), [follow-up report](claude-followup.md), [follow-up disposition](followup-disposition.md), and [final focused review](claude-final.md). The final reviewer found no remaining high/medium contradictions in its focused scope. Low edits made afterward are listed explicitly, with author verification and changed-file hashes.

## Optional native foundation review

[Claude’s native report](native-foundation-2026-10-18/claude-report.md) independently reads frozen `0647fb3` in 26 Read/Grep calls. The [disposition](native-foundation-2026-10-18/disposition.md) records two medium defects, later author fixes and explicit Driver/input/hardware coverage gaps. All 100 frozen hashes match. No native desktop or phase completion is certified.

The [native follow-up](native-foundation-followup-2026-10-18/disposition.md) reviews `846d9cf` in 20 calls, accepts the original fixes, and finds partial-initialization KMS restore still reachable. Subsequent author code adds guarded initialization and corrects terminal notifications; those later changes are explicitly outside the frozen read. An earlier API-failed attempt is preserved separately.

## Native output candidate review

[Claude’s candidate-planning report](native-outputs-2026-10-18/claude-report.md) reads `341ebbf` in 22 Read/Grep calls. The [disposition](native-outputs-2026-10-18/disposition.md) records a conditional, unmeasured format-pair-cap risk, subsequent failure isolation and reference validation corrections. The initial snapshot omitted the upstream free plane helper; the follow-up includes it. No native hardware or production qualification is claimed.

The [native output follow-up](native-outputs-followup-2026-10-18/disposition.md) inspects `99e3dac`, now with the free upstream plane helper. It finds no reachable high/medium defect within scope, accepts failure isolation, and retains the unmeasured format-pair cap and actual adapter/hardware gaps. Later bounded-reference and distinct-pair fixture changes are identified as author work.

## Native RGB target diagnostic review

[Claude’s target-diagnostic report](native-target-test-2026-10-18/claude-report.md) reads `abadd20` with 23 Read/Grep calls. The [disposition](native-target-test-2026-10-18/disposition.md) records two medium cache-lifetime/topology-oracle defects and subsequent source corrections. Actual GBM/GLES/DRM testing remains unexecuted.

The [target-diagnostic follow-up](native-target-test-followup-2026-10-18/disposition.md) independently reads `c2e86ed`, accepts the two medium corrections and finds no reachable high/medium issue in that scope. Inherited properties, incomplete side-effect reporting, conservative route failures, actual request-builder/renderer behavior and hardware remain open.

## Native frame transport review

[Claude’s frame-transport report](native-frame-transport-2026-10-19/claude-report.md) inspects 24 frozen input files in 17 Read/Grep calls and finds no high/medium issue within scope. The [disposition](native-frame-transport-2026-10-19/disposition.md) retains partial ABI coverage, stale-ticket diagnostics and combined multi-output submission limits. No live native presentation or hardware qualification is claimed.

## Native live submission review

[Initial Claude review](native-live-submit-2026-10-19/disposition.md) found initial-event and process-exit framebuffer hazards. The [focused follow-up](native-live-submit-followup-2026-10-19/disposition.md) accepts blocking initial modesets, CLOSEFB preflight and GETFB2 error cleanup, finding no new high/medium defect within scope. Late cleanup/crash recovery, physical hardware and main compositor integration remain unqualified.

## Optional native compositor review

[Initial review](native-compositor-2026-10-19/disposition.md) found a medium topology-retirement issue and low
startup/logging/cursor issues. The [focused follow-up](native-compositor-followup-2026-10-19/disposition.md)
accepts the corrections and restart boundary, finding no high/medium defect within scope. The author then
preserved failure diagnostics with a one-line correction and documented the kernel dependency-wait caveat.
Neither source review nor nested GUI regression qualifies native hardware or session recovery.

## Minimize and restore review

[Claude's focused source review](window-minimize-2026-10-19/disposition.md) found no high/medium defect.
The author strengthened protocol hints, focus recovery and reused-slot ID checks afterward and repeated
private-display runs. Pointer preconditions, older protocol versions and native hardware remain unqualified.

## Maximize and restore review

[Initial review](window-maximize-2026-10-20/disposition.md) found owned-click and incompatible-size issues.
The [correction follow-up](window-maximize-followup-2026-10-20/disposition.md) and
[caller check](window-maximize-caller-2026-10-20/disposition.md) accept the source corrections and event-loop
ordering. Final nested regressions cover fixed-size declines, late hints and focus stacking. Real clicks,
toolkits, the remaining bounded popup-cleanup delay and native hardware are not fully qualified.

## Fullscreen pages review

[Initial Claude review](window-fullscreen-2026-10-20/disposition.md) found canvas overlap and strip navigation
defects. Their corrections and projected-edge bounds have regression tests; the focused follow-up records
the final source verdict. Real nested checks cover lifecycle and exact backdrop pixels. Hardware and complete
window-state qualification remain open.
