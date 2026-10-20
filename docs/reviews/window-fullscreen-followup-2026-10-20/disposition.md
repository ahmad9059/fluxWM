# Focused fullscreen follow-up, 2026-10-20

The [report](claude-report.md) inspected 33 frozen hashed files in 14 Read/Grep calls.
[Execution verification](verified-execution.json) confirms matching hashes, no outside-snapshot requests
and no permission denials; the actual model metadata identifies Claude Opus 5.5.

Claude accepted the canvas page bank, virtual strip columns and projected-edge guards. It found a further
MEDIUM: removing an unfocused rightmost group can move the focused fullscreen page without updating the
canvas camera. The author subsequently added a focused-canvas-fullscreen reveal helper to removal and group-geometry edits, with
removal and group-move regressions; explicit panning remains effective. Those final
changes are author work after this frozen review, rather than a further Claude approval.

The wire-remap LOW led to a stronger fixture precondition: before unmap the role must have Fullscreen on
the wire, Maximized suppressed on the wire and both logical intents set. Unmap then clears both logical
intents, and the same-role remap must configure neither wire flag. Pinned Smithay reset behavior was
confirmed by this review.

Coordinate-limit insertion can still reject atomically rather than finding another canvas location.
Nested edit validation costs and full-backdrop damage remain optimization opportunities.
[Final tests and private nested execution](../../evidence/nested-milestone/2026-10-20-window-fullscreen/README.md)
exercise the author corrections. Native hardware, toolkits and full phase/state qualification stay open.
