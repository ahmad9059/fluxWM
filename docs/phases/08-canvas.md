# Phase 8: Full two-axis canvas, overview, search, and accessibility

Status: planned. **Dependencies:** Phases 4–7.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Implement persistent group world coordinates, independent x/y camera, insertion/snap policy, overlap/stacking, and directional navigation scoring.
2. Add mode-specific state, first-transition conversion, window membership reconciliation, and deterministic restoration between strip and canvas.
3. Add origin rebasing, large-coordinate tests, spatial visibility queries, prefetch margins, and bounded thumbnail caches.
4. Deliver overview with independent zoom, keyboard search across all windows, group/workspace drag, correct inverse picking, and overlay separation.
5. Add canvas gestures and edge-pan with ownership/cancellation, world-space drag offsets, configurable snapping, and focus/camera recovery.
6. Deliver scalable built-in UI, high-contrast focus, magnification/recovery, full keyboard navigation, and reduced-motion parity.
7. Evaluate the current accessibility protocol/service route, implement the supported integration, and record real screen-reader focus/navigation behavior and limitations.

## Required artifacts

True 2D canvas mode, strip/canvas conversion, overview/search UI, spatial index if justified, rebasing logic, accessibility support guide, visual/model/assistive regression evidence.

## Verification procedure

Pan far in both axes and to negative coordinates; switch modes repeatedly while opening/closing windows. Drag at non-unit zoom across scaled outputs; remove output mid-drag. Recover distant windows via search. Test 1,000 synthetic groups at fixed visible count. Complete navigation and window management entirely by keyboard, then with reduced motion and the selected screen reader.

## Exit gate

Canvas supports meaningful independent vertical and horizontal placement/panning, preserves every window across mode changes, remains numerically stable, and provides usable overview/search/accessibility flows. A horizontal strip with vertical workspace switching alone cannot satisfy this gate.

## Engineering notes

Use layout, animation/input, and Lua contracts. Accessibility limitations must be concrete; unsupported optional assistive features cannot substitute for demonstrating the chosen required screen-reader flow.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.
