# Floating correction follow-up, 2026-10-20

The [report](claude-report.md) independently reads 35 frozen input files.
[Verified execution](verified-execution.json) records **16** actual Read/Grep calls (the report says 13),
matching hashes, no outside-snapshot requests and no permission denials. Actual model metadata identifies
Claude Opus 5.5.

Claude accepted ordinary-tab preservation, stable fullscreen placement, direct born-floating admission,
overlay-sized buffer handling, accepted resize constraints, pointer-lock cleanup separation and grab
cancellation. It explicitly retracted the earlier Resizing-after-unmap claim after reading the pinned
Smithay reset.

One remaining MEDIUM identified stale canvas geometry on the first unfloat of a born-floating member.
Subsequent author code seeds a normal group beside the nearest ordinary saved column, with current default
width, constrained minima and normal viewport height. It also updates dormant canvas geometry when the
first unfloat happens in Strip. Both mode projections preflight materialization atomically. The model
regression moves/resizes a born float before first unfloat in each mode, checks normal dimensions and
adjacency and preserves saved neighbors. The real fixture additionally materializes a dialog in Canvas
and compares its dimensions and the unchanged neighboring tile. These final changes are author work
after the frozen review, not another Claude approval.

The author also clamps fitting initial floats into the viewport; oversized client minima retain size and
align at its leading edges. The retained strip position is its admission-time index, now explicit in
[behavior](../../development/floating.md). Shared floating coordinates across independent mode cameras,
cross-mode rebasing, vertical strip reveal and same-client cross-surface click authorization remain
qualification/policy limits.

[Final execution evidence](../../evidence/nested-milestone/2026-10-20-window-floating/README.md) identifies
the last source and binaries. Actual authorized drag/resize and full transactions, toolkits, popup/input
matrices, hardware, migration and transforms remain unqualified. No phase gate is closed.
