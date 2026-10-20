# Phase 4: Scrolling tiling, workspaces, and scale

Status: planned. **Dependencies:** Phases 2–3; pure model work can begin after phase 2.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Implement unique membership, column ordering, width policies, cached positions, and minimal focus reveal in a pure layout crate.
2. Add stack allocation with client constraints, tabs, column reorder, width presets, floating dialogs, fullscreen restore state, and cross-workspace/output transfer.
3. Separate workspace camera from tile target rectangles. Preserve anchors on insertion/removal so unrelated windows do not jump or shrink.
4. Implement independent per-output workspaces, empty workspace policy, monitor migration, reconnect preference, and window recovery commands.
5. Add viewporter/fractional scale integration, transformed output coordinates, shared-edge rounding, and usable-area calculations.
6. Introduce nonblocking resize transactions with epochs, readiness, deadlines, and explicit stale-buffer handling.
7. Property-test randomized commands and constraint conflicts; wire focus/scene picking to actual presentation geometry.

## Required artifacts

Usable horizontal scrolling layout, stack/tab/floating/fullscreen modes, multi-output workspace model, scale-aware transforms, transactions, layout property tests, navigation documentation.

## Verification procedure

Open 20 windows and confirm existing widths remain stable. Exercise oversized minimums, tiny usable area, tab close, fullscreen transitions, output removal, reconnect after explicit user reassignment, 1.25× scale, and rotated outputs. Use a client that never responds to configure and verify the desktop continues to navigate.

## Exit gate

All product layout invariants pass deterministic and randomized tests. Every window remains discoverable, focus is always valid or empty, and output/scale transitions preserve ownership. This is a strip milestone; genuine 2D canvas remains required in phase 8.

## Engineering notes

Use the layout algorithm specification and W02/W05 workloads. Keep layout logic independent of Lua and GPU APIs so failures shrink to model-level reproductions.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.

## Required follow-up review tasks

Implement minimize/restore, graceful close versus explicit client disconnect, maximize state and restoration in phase 4. Send wm_capabilities only for available actions; minimize capability begins with this implementation, not phase 2. Validate the XDG matrix's applicable native state rows, with full native/X11 variant qualification in phase 7.

## Incremental implementation

[Minimize/restore](../development/minimize.md) now retains mapped member identity and nonzero restoration
geometry, removes render/input/focus eligibility and exposes explicit IPC restore. Reserved slots do not
compact. Pure model and private-display protocol checks cover named cases; full phase verification,
maximize/fullscreen restore combinations, task UI and native qualification remain open.

[Maximize/restore](../development/maximize.md) adds viewport presentation with retained base layout geometry,
bounded required configure replies and committed-state geometry validation. Fullscreen pages, migration,
full native/toolkit qualification and the complete phase gate remain open.

[Fullscreen pages](../development/fullscreen.md) now project saved layout members into output-sized pages,
retain maximize/minimize intent and compose opaque backdrops independently of client opacity metadata.
Local pixel/lifecycle checks do not close native/toolkit, scale/output or complete state-matrix qualification.

[Graceful close and explicit client disconnect](../development/client-close.md) now distinguish a refused
window-close request from connection-wide recovery through a live role ID. Local multi-connection checks
do not close native/toolkit, grab or full desktop protocol cleanup qualification.

[Floating windows](../development/floating.md) now leave active tiling allocation while retaining restoration
metadata. Parent-dialog admission, exact-ID discovery and model-owned interactive geometry are integrated.
Real authorized drag/resize, resize transactions, output/scale and full phase qualification remain open.
