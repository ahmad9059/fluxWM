# State machines and ordering contracts

This document defines FluxWM design requirements. Source-backed protocol details are summarized in [source findings](18-source-findings.md); exact XML takes precedence over this shorthand. No transition table below is an existing implementation.

## Three different notions of readiness

1. **Layout ready:** a client has committed state suitable for a proposed size, or the layout deadline chooses to present an older valid buffer with a documented clip/scale.
2. **Acquire ready:** the particular committed buffer is safe to read. A layout timeout, client disconnect, frame deadline, or lock transition never turns an unsignaled acquire into permission to read.
3. **Consumer complete:** GPU work, scanout, and capture no longer read the buffer. CPU submission is not completion. Release depends on all actual consumers, not on whether the tile was removed from the model.

Represent these as separate fields and events. `layout_epoch`, `surface_commit_id`, `output_epoch`, `config_generation`, and `capture_session_id` serve different purposes. Never compare them as if one global serial answers every ordering question.

## Toplevel lifecycle

| State | Input | Action / next state |
| --- | --- | --- |
| Role assigned, unmapped | Initial permitted commit | Determine policy, issue configure, remain waiting |
| Configure pending | Acknowledge known configure | Update protocol acknowledgement state; no forced map yet |
| Ready for mapping | Valid buffer commit and acquire readiness | Map target tile and scene, then apply activation policy |
| Mapped | New configure/ack/commit | Reconcile latest valid size state; preserve older drawable buffer while necessary |
| Mapped | Null attachment/unmap | Remove input eligibility and mapped membership; retain role/history as protocol permits |
| Any | Resource/client destruction | Cancel dependent jobs, popups, grabs; retire consumer leases safely |
| Any | Invalid request | Let protocol handler send appropriate client-scoped error |

An ack can consume earlier configure serials; a later commit is not mechanically paired with exactly one configure. Serial numbers can wrap. Use the selected library's protocol handling rather than numeric less-than comparisons on arbitrary serials. New mapped membership occurs once; late jobs for destroyed generations cannot recreate a tile.

## Transaction coordinator

A layout transaction has states proposed → configures-issued → waiting → presented-with-ready-or-fallback-content → retired. A cancellation can occur from any nonretired state on model replacement; cancellation removes intent, not fences already submitted.

Record old and target rectangles, affected IDs, expected client state, time budget, and why fallback was chosen. A slow application cannot hold up camera movement or lock coverage. Subsurface synchronized commits may change eligibility; process the library's atomic surface-tree state instead of exposing half a tree. Burst commands can supersede target geometry while retaining the newest valid content. Coalesce repeated size changes but never discard role/lifetime events.

A transaction's fallback uses a previously acquired buffer. It does not force a client to produce a new size and cannot invent a successful presentation event for unpresented content. Buffer age/damage remain associated with the actual content selected.

## Buffer consumption ledger

Track a per-commit record: client/resource identity, surface generation, immutable commit state, acquire point, active consumers, release mechanism, and retirement state. Example consumers are render submission on GPU A, scanout on output B, capture copy on GPU A, and closing-snapshot source retention.

- A submitted GPU read retains a lease until the backend's completion synchronization proves the read is done.
- A scanout lease survives the frame showing the buffer and retires only when replacement/disable completion allows it.
- A copied snapshot may own a new texture; release its source only after the copy completes. A snapshot that references the original keeps its source lease.
- A discarded never-read commit may retire without waiting to read its acquire point when the protocol and selected backend allow it. Never use a blanket rule that release must always follow acquire signal.
- The same storage may appear in multiple commits or surfaces. A per-surface refcount alone cannot prove exclusive reuse safety.

Smithay's Buffer ownership helpers are part of this ledger, not a replacement for correct consumer lifetime. At the inspected revision, final reference retirement signals a release path; dropping the final reference too soon is an integration bug. Conversely retaining everything until shutdown causes client stalls and unbounded memory. Add debug retirement reasons and age histograms without logging pixels.

## Output epoch and preview rollback

States: discovered → probed → configured → active → inactive/suspended → reactivating; disconnected/failed are terminal for an epoch. Every queued frame, page flip, input mapping, and capture source includes the output epoch. A disconnect increments epoch and rejects stale completions while still cleaning resources associated with old work.

For a user display change: validate requested state → test feasible atomic state → apply → show confirmation on an active usable output → confirm or revert after timeout. The timeout is a proposed 15 seconds; tune and document. If the old topology disappeared, rollback selects a currently valid safe mode rather than replaying dead connector handles. Preview rollback is disabled as a way to reveal desktop while locked: all candidate output states remain safely covered.

Hotplug storms are coalesced for probing, while removal immediately invalidates ownership. Missing EDID, duplicate EDID, MST connector renumbering, and docks with changing identities require ambiguous-match handling. Prefer preserving user's explicit placement over aggressive reconnect restoration.

## Secure lock transitions

| Event | Required design result |
| --- | --- |
| Authorized lock request | Choose one owner, reject competing authority, cancel ordinary interactions, transition retained captures to opaque black, prepare safe coverage |
| Lock pending | Clear old scanout/planes/cursor as necessary; cover every active output with a safe frame |
| Safe coverage presented everywhere | Confirm protocol lock; only then allow the session policy to treat lock as established |
| New output during pending/locked | Its first visible frame is safe coverage or authorized lock content |
| Lock client dies | Preserve locked state, cover safely, optionally start explicitly trusted replacement |
| Replacement authenticates/unlocks | Validate current owner; old helper and stale requests have no authority |
| Authorized unlock | Remove coverage in a controlled transition, restore eligible focus, re-evaluate capture grants and input state |
| Output presentation fails during lock pending | Remain blocked/covered where possible; do not report successful locking while sensitive content may remain visible |

Use the disabled-state/no-wake lock policy from [review resolutions](21-review-resolutions.md) for powered-off/no-output/session-inactive cases; separately qualify any leased-output route; prove it against the selected backend and lock protocol. These cases are B08/B13, not presumed solved. Unprivileged IPC cannot unlock, arbitrary app metadata cannot nominate a locker, and a graphics failure cannot fall back to drawing desktop content.

Keep physical key/button ownership across transition: end normal focus/grabs correctly, suppress compositor shortcuts from keys held during lock/unlock, and require a fresh physical chord where needed. Do not synthesize arbitrary application actions to compensate for lost releases. Restore protocol keyboard state coherently when focus returns.

## Capture session transitions

States: requested → authorized → source-bound → constraints-negotiated → streaming / lock-redacted → stopped → destroyed. Authorization binds requesting principal, selected source generation, cursor mode, purpose, permission lifetime, and optional restoration data. Window closure terminates that source; it never silently switches to the whole desktop. Output removal cannot reinterpret an old node/source ID as a new display.

On lock, retain authorized sessions in internal lock-redacted state and respond only with opaque black, excluding cursor and lock UI. Do not send terminal stopped for lock. On actual revocation/source loss, stop/fail terminally. In both paths cancel unsent sensitive jobs and retire source AND capture-destination consumers safely. A delivered frame cannot be erased from a consumer's memory; security claims concern future production/delivery and must state the revocation boundary. Sensitive-surface policy must take effect before the first protected commit enters capture.

## Config helper transitions

States: idle → helper-starting → evaluating → converting → validating → candidate-ready → applying → committed/rejected. Only one active evaluation and one coalesced latest reload request are allowed by default. Parent deadline terminates/reaps the helper and keeps the last good config. An old candidate is discarded after a newer generation, shutdown, or lock-sensitive policy change.

Logical config is committed only after validation; asynchronous hardware operations report desired/effective/pending state separately. A validated snapshot cannot claim unsupported hardware changes succeeded. Helper exit output is bounded and contains only schema data or redacted error, not process-control commands.

The [review resolutions](21-review-resolutions.md) specify concrete buffer guards, acquire/transaction-blocker decisions, capture destination writes, power-off locking and helper launch semantics. Conceptual layout readiness may not be independently observable before acquire through Smithay's blocked-commit path. No implicit-sync claim is closed without the phase-3 backend spike.
