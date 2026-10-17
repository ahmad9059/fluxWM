# Phase 5: Interruptible animation, gestures, and frame pacing

Status: planned. **Dependencies:** Phases 3–4.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Implement analytic springs and cubic Bézier sampling with monotonic fake-clock support, validation, settle criteria, and retarget continuity.
2. Animate camera, layout presentation, workspace transitions, fades, and overview-ready transforms while preserving target configure geometry.
3. Add per-output scheduling with estimated deadlines, presentation feedback, frame wakeup causes, and independent refresh clocks.
4. Implement damage tracking over old/new bounds, conservative clips, and surface content generations. Add damage/full-repaint diagnostic comparison.
5. Add gesture ownership, thresholds, inertial pan, snapping, cancellation, and pointer picking through animated transforms.
6. Provide reduced motion and instant-transition parity. Stop scheduling frames after settling.
7. Implement basic borders/corners and bounded shadow caching; postpone expensive blur tuning until measured work in phase 9.

## Required artifacts

Animation library, input gesture state machine, per-output scheduler, presentation instrumentation, damage diagnostics, reduced-motion option, visual regression scenes.

## Verification procedure

Replay at 30–240 Hz and irregular timing; compare equal-time positions. Retarget a pan repeatedly, close mid-transition, suspend/resume, and change refresh. Confirm static scenes quiesce. Check damage output against full repaint, including shadows and transformed subsurfaces. Test touchpad cancellation on real input hardware.

## Exit gate

Motion remains continuous and finite, input follows rendered geometry, reduced motion retains functionality, and mixed-refresh outputs schedule independently. Timing and repaint costs have a reproducible baseline rather than subjective smoothness alone.

## Engineering notes

Use animation/input specification and W01/W02/W05. A screen recording is useful visual evidence but does not measure physical input latency.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.

## Required follow-up review tasks

Complete B17 shaped text, fallback fonts, glyph atlases/cache budgets, color-emoji or declared fallback, and display-change confirmation UI. Semantic labels/nodes are in the data model now; AT-SPI/Orca qualification is phase 8. A11Y-02:text-fixture qualifies only rendering; search/task-list behavior remains phase 8.
