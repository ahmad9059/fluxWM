# Author disposition: minimize/restore

[Claude's source review](claude-report.md) found no reachable high/medium defect within scope.
[Verified execution](verified-execution.json) records **15** Read/Grep calls over 20 frozen input files,
not the report's self-reported 13. The verifier checked snapshot hashes and request containment; no permission
denials occurred. Claude did not run tests or verify hardware.

The author strengthened the fixture after this review to check Suspended on the wire, real keyboard focus and
Activated recovery after all-hidden restore, and rejection of the old ID after actual arena-slot reuse. Only
the fixture and the documented reorder policy changed after the frozen source review. Those additions were
not independently re-reviewed; affected fixture checks and both private-display runs were repeated.

The pointer-focus assertion remains limited: the fixture does not establish prior pointer focus on a window.
Its all-hidden empty-pointer observation cannot independently prove input revocation. Dynamic scene filtering
was source reviewed; full pointer/active-grab and protocol-version matrix tests remain open. Suspended is now
asserted for the negotiated version, not every older binding version. Reordering swaps selectable neighbors
across minimized slots; this is explicit in the behavior documentation.

Existing lifecycle and scene fixture modes also passed with the compositor-wide Minimize capability check.
See [retained runtime evidence](../../evidence/nested-milestone/2026-10-19-window-minimize/README.md).
Native physical operation, task UI, full state combinations and phase-4 qualification remain open.
