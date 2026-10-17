# Phase 10: Hardening, packaging, and release readiness

Status: planned. **Dependencies:** All previous phases and their evidence.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Audit every requirement against design, code, runtime behavior, and acceptance evidence. Close missing protocol versions, error paths, and documentation gaps.
2. Complete conformance integration where practical, fuzz lifecycle/parsers, run long soak sessions, and repair critical crash, lock, sync, resource growth, and recovery issues.
3. Run the full real-user journey: native/X11 work, strip/canvas/overview, gestures, IME, accessibility, reload, capture, locking, docking, and suspend with reduced-motion repeat.
4. Produce packages/session entries for the chosen initial distributions, reproducible install/uninstall instructions, manuals, default config, completions, and diagnostics guide.
5. Evaluate Vulkan/HDR/DRM lease separately. Either deliver with capability tests and compatibility evidence or explicitly document deferred/unsupported status without weakening required GLES/SDR behavior.
6. Freeze schema/IPC v1, publish migration and compatibility policies, review dependency/source licenses and notices, and document vulnerability reporting and supported hardware.
7. Produce a release candidate, run clean-system installation validation, publish raw measurements/limitations, and sign off the traceability audit. Re-run relevant evidence after any release-blocking fix.

## Required artifacts

Release build/package artifacts, completed requirement audit, protocol/application/hardware matrices, soak/fuzz reports, user/operator docs, supported configuration example, known issues, release notes and maintenance policy.

## Verification procedure

Install on clean supported systems, select the session, use the whole workflow, upgrade/reload configuration, collect a redacted bug report, and uninstall cleanly. Run at least 24-hour session churn/soak and inspect memory/resource trends. Verify lock/capture failure injection after final changes. Check every advertised protocol version and required feature combination.

## Exit gate

Every required feature has authoritative evidence, no critical security or lifetime defect remains, packages reproduce a usable session, and performance/compatibility statements match measured support. Optional research outcomes are explicit. Only then call the compositor release-ready.

## Engineering notes

Use product definition of completion and traceability. Documentation completion today does not mean this future implementation gate is already met.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.
