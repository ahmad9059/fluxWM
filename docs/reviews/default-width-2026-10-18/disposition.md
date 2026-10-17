# Author disposition: configured future-admission widths

Claude inspected frozen `ccfdb61c012430adfc2cd5b9d36cddfd56e4a00f` using Read/Grep only. The [report](claude-report.md), [completion](stdout.json), [prompt](prompt.txt), [command](command.json), [execution](execution.json), [manifest](input-manifest.json) and [author verification](verified-execution.json) retain the actual record. Exit was zero after 103.4 seconds; all **72 frozen hashes match**, with no permission denials, outside-snapshot paths or error result. The actual trace contains **18 calls**; the report self-describes 17. Claude ran no tests, builds or commands and did not independently confirm author execution claims.

The reviewer found **no reachable high or medium defect in the focused default-width implementation**. It checked copied admission policies, common preflight/actual width selection, model publication/viewport validation, one-based preset lookup, default compatibility, desired/effective state and the six-step fixture/harness. It did not inspect the complete layout, all Case helpers or all config tests, and did not certify production readiness.

## Accepted low corrections

A copied fixture comment incorrectly described admission rejection; it now describes the six live width admissions. The Canvas contract now specifies that an admission while already in Canvas seeds the group from the constraint-adjusted client strip rectangle, rather than promising the raw policy size in every case. Established mode-conversion behavior is unchanged.

Python's ties-to-even `round()` could disagree with Rust's positive half-away rounding on a quarter-viewport value ending in .5. The harness now uses positive `floor(width + .5)` within the model's coordinate bound and clamps the fractional expectation to the model's one-pixel minimum. The qualified viewport is still 1280 pixels wide, so this correction is source-derived portability hardening, not an executed alternate-viewport claim.

Later author-only checks also require every nonzero wire configure width to match the member's retained policy and every nonzero height to match the viewport, not merely the final width. Each width publication/recovery must report the accepted applied generation. A new model test exercises failed admission, retry under a smaller default without focus theft, and removal/re-admission under a later fractional default. These low corrections occurred after the frozen review and were verified by the author, not reread by Claude.

## Evidence and retained scope

The real private fixture creates six SHM-backed windows with widths 400/760/320/600/600/480, checks stable existing IDs/widths, initial frames, wire sizes, invalid-preset retention and recovery. A deliberate column-creation mutation keeps the old half-viewport width while desired/effective metadata reports the requested policy; the real harness fails on member0's actual width. A separate zero-based preset lookup mutation fails the correct-policy unit assertion. Both original sources are restored; exact producer snapshots, commands and failure/pass records are retained in the [evidence bundle](../../evidence/nested-milestone/2026-10-18-default-width/README.md).

Preset99 fails schema validation before the new lookup; it qualifies invalid-schema retention, not a reachable native width-conversion error. Defensive lookup/model errors have component coverage. Remap/rejected-buffer retry, viewport resize and client-size constraints are source/model contracts with component tests; the width harness does not establish those combinations at the whole-compositor boundary. Neither frames nor configures establish physical presentation. Full rules/preset cycling, device/output behavior, resource timing, native sessions and soak remain open.
