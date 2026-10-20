# Fullscreen review disposition, 2026-10-20

The [initial report](claude-report.md) inspected the frozen implementation based on `466c8b1`, with
[verified execution](verified-execution.json): 20 Read/Grep calls, 33 hashed files, no requests outside the
snapshot and no permission denials. The worktree hashes identify the reviewed bytes; this is not a review
of the base commit alone.

Both MEDIUM findings were corrected: canvas pages now occupy a separate bank beyond all saved groups,
including hidden/minimized groups; strip navigation traverses virtual columns and keeps vertical moves
inside their original column. Regression cases cover a two-stack neighboring column, forbidden vertical
jumps, multiple canvas pages, insertion and focus reveal. The randomized layout test checks page
disjointness. Ordinary projected right/bottom edges are now bounded too.

The new-buffer-per-capture LOW remains a performance limitation: each backdrop can cause full damage.
Unmapping does reset pinned Smithay role state; that source was included in the initial manifest despite
the report saying it was absent. The fixture additionally remaps the same role and checks the actual wire
Fullscreen and Maximized flags are clear. Dropping incompatible initial maximize intent is deliberate:
restore is conditional on current constraints, avoiding an impossible configure/admission.

See the [focused follow-up](../window-fullscreen-followup-2026-10-20/disposition.md) and
[final execution evidence](../../evidence/nested-milestone/2026-10-20-window-fullscreen/README.md).
Full native hardware, toolkits, output migration, transforms, transition animation and the complete state
matrix remain unqualified. No phase gate is closed.
