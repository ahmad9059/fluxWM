# Phase 7: Desktop protocols, security, capture, and X11

Status: planned. **Dependencies:** Phases 3–6.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Implement layer-shell work areas, decoration negotiation, activation policy, foreign-toplevel integration, cursor shape, relative pointer, constraints, and shortcut inhibition.
2. Complete clipboard/DnD, primary selection, privileged data-control policy, and paired text-input/input-method integration. Test transfer backpressure and destruction.
3. Implement session lock with compositor safe cover, hotplug coverage, input isolation, capture lock-redaction (terminal stop only for revoke/source loss), and lock-client crash recovery. Integrate idle and suspend ordering.
4. Run explicit capture/portal and X11 feasibility spikes. Record decisions, required protocols, companion versions, and failure behavior before expanding implementation.
5. Deliver portal Screenshot/ScreenCast with chooser/consent and PipeWire backpressure. Keep sensitive-window policy consistent across capture paths.
6. Deliver selected Xwayland route with readiness, environment, focus, menus, clipboard, games, helper supervision, and crash cleanup.
7. Integrate and document panel, launcher, notifications, authentication agent, portal activation, lock/idle, and session environment. Establish a practical daily-use session.

## Required artifacts

Updated protocol inventory, tested desktop bundle, lock/security regression tests, portal/capture integration, X11 integration decision and implementation, IME/clipboard compatibility matrix, session startup documentation.

## Verification procedure

Run browser sharing and screenshot selection, copy large data between GTK/Qt/X11, input CJK text, use game pointer lock and recovery shortcut, and show panels on mixed-scale outputs. Lock during sharing; kill locker; add/remove outputs while locked; suspend and resume. Assert no desktop frames leak into output or capture. Kill portal/X11 helpers and verify graceful feature isolation.

## Exit gate

A tested companion session supports ordinary desktop work, native and X11 applications, secure lock/idle lifecycle, clipboard/IME, and permissioned capture. Every required integration has recorded versions and real app evidence, not only protocol registration.

## Engineering notes

Use protocol matrix and desktop/security document. This is a large integration phase; split internal issues by contract but keep all required gates before calling the phase complete.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.

Explicit protocol tasks also include tablet-v2, xdg-foreign, pointer gestures, virtual keyboard/lock OSK, ext-workspace/Waybar integration, output power management, xdg-dialog and foreign-toplevel management actions. Read-only list support cannot satisfy taskbar actions. Qualify normal activated host helpers and tagged sandbox denial under B16.

## Required follow-up review tasks

Implement wp-security-context-v1 listener lifecycle, trusted broker metadata, no nested contexts and per-context filtering before B16 sandbox-denial gates. Tagged-context tests cannot pass from peer UID or app_id alone. Qualify SURF-09:strip-native for native and selected X11 state behavior.
