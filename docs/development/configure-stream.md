# Bounded nested toplevel configure stream

The nested runtime now limits ordinary toplevel configure traffic to one unacknowledged record per role. Previously, a mapped client that kept reading but never acknowledged could grow Smithay's pending configure vector on every size or activation change. The preserved binary emits 130 configures for 130 resize actions; the new binary emits one during the same stall. This is one resource bound, not a completed layout transaction coordinator.

## Ownership and ordering

`crates/fluxwm/src/configure.rs` gates ordinary sends on Smithay's actual pending list. Layout changes continue updating Smithay's single `server_pending` state, replacing intermediate intent with the newest size/state. An accepted acknowledgment frees the wire slot and the mapped role sends its latest pending changes. Returning to the already-sent state sends no redundant configure. No timer, independent serial ordering, configure history or extra strong surface owner is added.

An acknowledgment is not a surface commit. Smithay remains responsible for validating serials, consuming earlier outstanding records and applying the acknowledged state on a later commit. IPC observes both `last_acked_serial` and `committed_serial`; it must not treat the former as acquired content, successful rendering or presentation. The retry pass is bounded by the 1,024-role arena cap, but is not a qualified wall-clock fairness or request-work limit. Current scene placement still follows the live layout immediately, with existing buffer clipping. Atomic multiwindow presentation, readiness deadlines and acquire/consumer ownership remain open.

Null unmap resets the role's initial-configure state while Smithay retains in-flight serials. The next bufferless initial commit must receive a fresh configure even when one older record remains outstanding. That exception permits **two** records across the mapping-cycle boundary; ordinary mapped traffic admits no third record. A late old acknowledgment while unmapped does not cause this scheduler to send a premature configure. Protocol root-buffer presence is tracked separately from layout membership. A role whose constraints exceed the supported layout extent remains alive, reports `buffered: true`, `mapped: false` and an `admission_error`, and can retry with smaller committed hints. Every relayout also makes one bounded arena-order pass over live rejected buffered roles, so capacity freed by another member or viewport/model changes can admit a static client without another surface commit. Automatic recovery preserves the selected member (or selects the first member in an empty layout); it creates no polling timer or recursive relayout. Its null unmap still waits for a subsequent bufferless commit before sending a fresh initial configure. Role destruction drops its pending/coalesced state; a replacement receives a new generational window ID.

The send sites cover tiled layout changes and the existing legacy floating resize path. Floating mode is unavailable, so that path has source coverage only. Popup configure/reposition streams are outside this bound. Ping responsiveness does not establish acknowledgment progress: this fixture answers pings and reads events while withholding configure acknowledgments. No stalled client is killed by this scheduler.

## Diagnostics

Each `windows.list` row exposes `buffered`, `mapped` and nullable `admission_error` alongside a `configure` object. Here `mapped` means admitted to the layout; `buffered` means a root buffer exists, not that it is safe for a GPU read. The configure fields are:

| Field | Meaning |
| --- | --- |
| `unacknowledged` | Actual Smithay pending configure record count |
| `initial_sent` | Current mapping-cycle initial configure flag |
| `deferred` | Smithay's `has_pending_changes`, comparing pending state with the last configured state; it can be true while unmapped and does not promise an automatic send there |
| `last_acked_serial` / `committed_serial` | Latest accepted acknowledgment / acknowledgment attached to current cached surface state; nullable |
| `desired_size` / `sent_size` / `committed_size` | Pending intent / latest configured suggestion / committed acknowledged suggestion; nullable logical integer pairs |

Sizes describe configure suggestions, not actual buffer dimensions. Serial values are opaque protocol tokens; consumers must not sort them numerically. Acknowledgments advance IPC's observable state sequence without requesting a redraw. No client metadata or pixel logging is added by these fields.

## Reproduce

```sh
cargo build --workspace --locked
python3 tools/smoke_private.py --suites configure
```

The authenticated owned Xvfb runner isolates this from the user's desktop. Artifacts and short private runtime directories are separate. `smoke_configure.py` reuses the bounded readiness/process/IPC/cleanup runner from `smoke_health.py`; its fixture deadline is 5 seconds and the outer case budget is 20 seconds, with teardown separate. Use `--bundle /path/to/verified/Xvfb-bundle` to reuse an existing verified package cache. No package is installed into the system.

The two real SHM fixture cases verify:

- 130 resize actions, continued reading without acknowledgment, one wire configure and retained latest intent; a separate healthy Wayland connection completes eight requested frame callbacks during the stall.
- Acknowledging the outstanding state sends exactly the latest coalesced size without changing the committed serial. Acknowledging that new configure still does not change the committed serial until a subsequent buffer-preserving commit.
- Intermediate intent returning to the sent state produces no configure echo when acknowledged.
- An old acknowledgment after null unmap does not issue the next initial configure early. A subsequent bufferless commit and acknowledgment permit remapping.
- A new initial configure coexists with one older in-flight record; acknowledging the new serial retires both. Remapping produces another frame callback.
- Destroying a role with a configure outstanding removes it while its healthy sibling survives; a replacement maps with a different generational ID.
- A second fixture commits a valid billion-pixel minimum-width hint that exceeds the aggregate layout limit. The rejected role stays buffered, null unmap emits no configure and advances the diagnostic sequence, a fresh initial handshake with smaller hints maps the same ID, and another role in the same client receives a frame. The preserved binary emits a premature configure on null unmap.
- A 600-million-pixel minimum-width blocker plus a 500-million-pixel static client exceed the aggregate extent, though either fits individually. Destroying the blocker admits the same static role without another surface commit, clears its error, preserves background focus and completes its original queued frame. The intermediate pre-retry executable remains stranded in this case.

The baseline and a mutation disabling only the acknowledgment-triggered flush fail different fixture oracles. The mutation retains the one-record bound but cannot deliver the latest state, demonstrating that queue size alone cannot pass this test. [Raw evidence and qualifications](../evidence/nested-milestone/2026-10-17-configure-stream/README.md) retain failures, executable identities, cleanup and later review disposition.

## Remaining gates

This does not qualify arbitrary toolkit behavior, invalid/replayed/wrapped serials, every constraint/state transition, native backends, popup configure bounds, lock/activation policy, per-client allocation/byte accounting, hostile request fairness or long-running memory use. The eight healthy frames prove progress in this declared workload, not a global scheduler fairness bound. Shared-scene and phase-2 qualification remain incomplete.

The default private runner now includes both health and configure suites. Hosted CI still runs build/unit/documentation checks without these graphical cases; a configured workflow is not evidence of hosted graphical execution.

The [retained independent review](../reviews/configure-stream-2026-10-17/disposition.md) exposed static-client recovery; its [focused follow-up](../reviews/configure-stream-followup-2026-10-17/disposition.md) inspected the correction. Retry work scales with rejected candidates and layout membership; several member removals can trigger several passes. Arena-order retry is not FIFO, multiple recovered columns can reverse candidate order after the selection, and multi-candidate starvation/cost remain unqualified.
