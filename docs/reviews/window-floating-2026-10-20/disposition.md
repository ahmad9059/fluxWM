# Floating-window review disposition, 2026-10-20

The [initial report](claude-report.md) inspected the frozen worktree based on `9cb497f` in 19 Read/Grep
calls. [Execution verification](verified-execution.json) records 35 matching input hashes, no requests
outside the snapshot and no permission denials. Model metadata identifies Claude Opus 5.5.

Four MEDIUM findings led to corrections: ordinary tab selection survives floating/fullscreen focus;
floating edits no longer shift fullscreen pages or unrelated tiles; direct born-floating admission consumes
no ordinary slot until first explicit unfloat; floating from an overlay uses a constrained 60%-viewport
default instead of a committed overlay buffer. Model regressions cover tab selection, unchanged neighboring
rectangles, born-floating admission and atomic materialization failure. Real-client additions commit actual
overlay-sized buffers before toggling and compare ordinary neighbors around dialog admission.

M5's assertion that Resizing necessarily survives unmap is contradicted by the pinned Smithay reset in
`src/wayland/shell/xdg/mod.rs:1633`: role, cached pending state and server_pending reset to defaults. That
source was included in the initial manifest. The author also explicitly clears Resizing on unmap without
sending a configure, while cancelling the old mapping's grab. Actual unmap during an authorized resize
remains unexecuted; an explicit clear is not that runtime evidence.

LOW corrections use accepted model constraints for edge anchoring, fall back to viewport centering for
hidden/offscreen parents and defer popup-grab cleanup while a pointer callback holds its lock. Shared world
rectangles across independent strip/canvas cameras, cross-mode rebase, vertical strip reveal and same-client
cross-surface click authorization remain explicit qualification/policy limits.

The [focused correction review](../window-floating-followup-2026-10-20/disposition.md) and
[final execution evidence](../../evidence/nested-milestone/2026-10-20-window-floating/README.md) identify the
later source and checks. Actual authorized drag/resize, resize transactions, toolkits, complete popup
interactions, native input/hardware, output migration and scale/transform matrices remain open. No phase
gate is closed.
