# Phase 3: Native GPU backend and output lifecycle

Status: foundation in progress; native output/hardware gate unqualified. **Dependencies:** Phase 2.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Add seat/session integration, udev discovery, libinput devices, DRM/KMS device state, and EGL/GBM GLES renderer setup.
2. Discover connectors/modes/CRTCs/planes through supported abstractions. Allocate compatible render targets, submit atomic state, and process page flips.
3. Implement DMA-BUF imports/feedback based on negotiated formats/modifiers, plus SHM upload. Track client, texture, target, and scanout lifetimes independently.
4. Handle VT deactivate/reactivate, hotplug, mode failure, and renderer creation failure. Retain previous working output state on failed configuration.
5. Support outputless state and output return where feasible. Keep workspaces recoverable when outputs disappear.
6. Add raw timing spans, import/commit failure counters, backend capability reporting, and redacted diagnostics.
7. Build a hardware test script for a primary development GPU, second output, and repeated session transitions. Keep nested builds working.

## Required artifacts

Native backend, unprivileged session launch instructions, GLES presentation path, capability-based DMA-BUF support, output transaction logic, hardware test log, baseline CPU/GPU measurements.

## Verification procedure

Run on a physical VT as an ordinary session user. Exercise connect/disconnect, mode change, VT switch, app GPU rendering, client death, and repeated startup/shutdown. Inspect no early buffer reuse under stress. Confirm KMS errors are diagnosed and failed modes restore a valid output. Record exact driver/kernel/system library versions.

## Exit gate

FluxWM displays and accepts input on physical hardware reliably through tested lifecycle changes. Buffer ownership is reviewable, GPU-backed composition works, and nested and DRM builds pass. Other GPU vendors remain unqualified until phase 9.

## Engineering notes

Use rendering and desktop lifecycle specifications. Explicit sync is not advertised until its end-to-end path is implemented and tested; phase 9 completes the required capability work.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.

## Required follow-up review tasks

Implement the B17 minimal font/shaping/fallback spike and record candidate dependency/build results. Early display preview has a developer-only nonblocking stdin/TTY acknowledgement and 15-second safe revert; it does not require phase-6 fluxctl or phase-5 text UI. Phase-3 GPU-05 coverage is only the compositor-owned primary-plane variant; client direct scanout is phase 9.

## Implementation foothold

The [native session/device foundation](../development/native-foundation.md) now adds an optional libseat/udev/libinput/DRM/GBM/GLES layer, explicit broker descriptor ownership, epoch/generation gates and a separate initialization probe. Device-free tests and read-only inventory do not implement or qualify native output rendering, transactions, physical input or the exit gate. Main FluxWM remains nested-only.

[Output discovery and candidate routing](../development/native-outputs.md) now implements the discovery/planning portion of task 2. Device-free exhaustive small-graph and query-failure tests do not qualify allocation, KMS transactions, rendering or physical outputs; the remainder of task 2 and the exit gate stay open.
