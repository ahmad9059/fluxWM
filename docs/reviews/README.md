# Independent Claude review

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
