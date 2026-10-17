# Phase 1: Foundation and reproducible development

Status: in progress; exit gate unqualified. **Dependencies:** None. A pinned Rust workspace, bootstrap and component implementations now exist. See the [current audit](../24-implementation-readiness-audit.md) for remaining artifacts and evidence; a compiling bootstrap does not pass this phase.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Choose and record Rust stable version, edition, MSRV, target distributions, and project license. Inspect licenses before copying any upstream source.
2. Select exact Smithay revision and compatible wayland-rs/Calloop/protocol bindings. Record why a release or Git revision was chosen and required fixes. Save protocol XML hashes and system-library versions.
3. Build and run the pinned upstream minimal/sample compositor in a disposable nested session. Record commands, backend, a rendered client, and shutdown behavior; this is a dependency spike, not FluxWM implementation.
4. Create Cargo workspace with executable, pure layout/core library, and config/schema skeleton. Add explicit nested and DRM feature groups only when supported by actual dependencies.
5. Add formatting, linting, unit-test, and build CI; commit lockfile/toolchain. Document system dependencies from the selected upstream source rather than guess package names.
6. Define typed IDs, command/event vocabulary, coordinate units, error categories, fake monotonic clock, and a basic trace/log format.
7. Establish initial benchmark hardware inventory and test applications. Write a capability inventory template and record all proposed numerical budgets as unmeasured.

## Required artifacts

Pinned dependency manifest and lockfile; build/development instructions; initial Rust workspace; CI configuration; license/source inventory; sample spike log; hardware baseline; initial architecture decisions.

## Verification procedure

On a clean supported environment, follow the documented instructions to build the workspace and run the upstream sample with a real client. Run formatting/lint/tests for created code. Confirm no proposed FluxWM CLI is documented as already functional. Dependency revisions and protocol source hashes must be reproducible.

## Exit gate

A new contributor can reproduce the nested development environment and understand ownership/coordinate conventions. Every required native dependency and selected revision is recorded. No unreviewed source copying or floating Git dependency remains.

## Engineering notes

Read source register S01–S07 and S15. Resolve toolchain/dependency failures here before adding compositor policy. Do not equate a compiling skeleton with a working display server.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.
