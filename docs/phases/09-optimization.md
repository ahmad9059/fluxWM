# Phase 9: Optimization, explicit sync, and GPU qualification

Status: planned. **Dependencies:** Phases 3, 5, 7, and 8.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Implement end-to-end explicit sync with capability-gated advertisement, acquire readiness, release accounting, and destruction/reset paths. Add delayed GPU/consumer stress tests.
2. Qualify Intel, AMD, NVIDIA, and hybrid configurations. Resolve cross-device import/modifier negotiation and document supported copy fallbacks and costs.
3. Implement/qualify direct scanout, hardware cursor and plane eligibility/fallback, output management, VRR, and mixed-refresh behavior.
4. Profile visible-scene extraction, allocation churn, overdraw, import copies, damage, and per-output scheduling before changing algorithms or threading.
5. Complete bounded blur/shadow effects, cache eviction, memory telemetry, battery profiles, and optional adaptive quality with hysteresis.
6. Run workload suite W01–W11 with reproducible settings, CPU/GPU/presentation instrumentation, percentile reports, and idle power observations.
7. Fix target misses or record evidence-based target revisions with rationale and impact. Keep correctness/security gates strict during optimization.

## Required artifacts

Hardware qualification matrix, explicit-sync implementation/tests, scanout/VRR/output-management support, optimized effects and caches, raw benchmark records, performance report and regression thresholds.

## Verification procedure

Exercise fullscreen/capture/scanout transitions, fence delays, unsignaled/destroyed timeline scenarios, multi-output buffer use, dGPU removal, GPU reset where safe to reproduce, hybrid offload, and suspend. Compare damage and full repaint. Verify cache resources retire only after consumers finish. Measure static idle and sustained scrolling with documented drivers and power profiles.

## Exit gate

Required GPU capabilities work on published tested configurations, explicit sync never releases early, mixed-GPU fallback is observable, and performance claims have reproducible evidence. No universal vendor support or Vulkan speed claim is made from library features alone.

## Engineering notes

Use rendering and performance specifications. A software-rendered CI pass is not physical hardware qualification. Do not broaden protocol advertisement to hide unsupported paths.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.

## Required follow-up review tasks

Run W11 in addition to W01–W10: private associated surfaces, blur, output/window sharing, overview/magnifier and retained black lock streams; account separately for redacted render cost.
