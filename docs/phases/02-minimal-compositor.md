# Phase 2: Minimal nested Wayland compositor

Status: planned. **Dependencies:** Phase 1.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Implement a nested backend using the pinned supported Smithay path. Create a unique display socket, publish readiness, and clean it up on exit.
2. Wire core globals, SHM, surface commits, subsurfaces, output metadata, seat, and xdg-shell. Keep pending/current state distinct.
3. Implement toplevel/popup map/unmap/destruction and configure serial handling. Add ping/pong and stalled-client diagnostics without blocking.
4. Render client surface trees through GLES in the nested window. Establish the scene/picking interface before adding tiling.
5. Route keyboard and pointer input with xkbcommon and correct serial/focus behavior. Add a minimal fixed terminal-launch and exit path for development.
6. Handle client disconnect during render, popup destruction during grabs, null-buffer unmap, and resize before first buffer.
7. Add synthetic client harness and deterministic lifecycle traces. Bound per-client event work and track resource counts.

## Required artifacts

A runnable FluxWM nested executable; surface lifecycle implementation; basic scene rendering; input routing; synthetic client tests; effective runtime global inventory.

## Verification procedure

Map SHM and GPU-backed native clients where the nested path supports them. Open nested menus and subsurfaces; resize, unmap/remap, and kill clients. Send invalid protocol sequences and verify only offending clients fail. Check press/release routing across focus switches and close the host window safely.

## Exit gate

A terminal plus GTK/Qt test clients can map, receive input, resize, open popups, and close under FluxWM without leaking server resources. Automated lifecycle cases pass and shutdown cleans socket/children. No native DRM compatibility claim is made yet.

## Engineering notes

Use architecture and protocol documents. If client startup stalls, inspect registry/configure/ack/commit ordering before adding workarounds. Screen screenshots and focus assertions should agree.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.

B16 design in this phase reserves authenticated host/sandbox context classification and privileged-operation filtering before registering sensitive globals. Implement and qualify wp-security-context-v1 in phase 7 before claiming tagged sandbox isolation. Untagged sandbox processes remain within the documented host-trust limitation.

## Partial implementation evidence

The [nested runtime](../development/runtime.md), [shared composed scene](../development/runtime-scene.md), [IPC quit drain](../development/shutdown-smoke.md), and [shell responsiveness monitor](../development/shell-health.md) record executable progress and exact remaining limits. Responsiveness now has real ping/serial/timeout/recovery/disconnect/capacity tests; this does not close task 3’s configure transaction work, the complete input/resource matrix, toolkit compatibility or this phase’s exit gate. Historical phase status remains planned until its full gate is proven.

[Host-focus recovery](../development/host-focus.md) adds real key/modifier/button and one valid popup-grab cancellation/re-entry trace. It is partial task-5/6 evidence; it does not qualify all keyboard/grab/activation behavior or change this phase’s exit status.

[Bounded toplevel configure scheduling](../development/configure-stream.md) adds partial task-3/6/7 evidence for a reading but non-acknowledging client, latest-state recovery, distinct ack/commit ownership, remap ordering and pending-role destruction. Complete configure/layout transactions, toolkit conformance and per-client resource qualification remain open.
