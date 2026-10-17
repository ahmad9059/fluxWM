# Configure-scheduling review of snapshot 0dba143

I used only Read and Grep, 18 calls, on the frozen snapshot and the copied Smithay source. I ran no tests and relied on no earlier review or author test results. Everything below comes from reading the code.

**Verdict: no HIGH bugs found. One MEDIUM gap is proven by reading the code, and two LOW limitations are noted.** This does not certify production readiness. Full layout transactions, the acquire/native backend, popup configure bounds and global resource qualification are still open.

## What I checked

**Ownership of the one send slot.** `configure::send_pending` (`crates/fluxwm/src/configure.rs:14-24`) sends only when `initial_configure_sent && pending_configures().is_empty()`. It then calls Smithay's `send_pending_configure`, which sends only when `has_pending_changes()`. It reads under the role mutex and sends after releasing it, which avoids a deadlock.

Every ordinary send goes through this function:
- `state.rs:508` (relayout)
- `xdg_shell.rs:64` (ack)
- `xdg_shell.rs:172` (resize request)
- `resize_grab.rs:148,183` (resize grab)

The only direct `send_configure()` is the initial one at `xdg_shell.rs:311`, and it is guarded by `!initial_configure_sent`. Grep found no other toplevel send sites in `crates/fluxwm/src`.

The resize sites can't currently run: `resize_request` requires `item.floating`, and nothing ever sets `floating` to true. They are guarded anyway, because `send_pending` won't send on a reset role.

**Serial, ack and commit ordering.** Smithay's `ack_configure` (`upstream/.../mod.rs:221-240`) takes the matching configure as `last_acked` and drops all older ones (`retain(|c| c.serial > serial)`). Smithay updates its state before it calls the handler, so `send_pending` in fluxwm's `ack_configure` sees the slot already freed.

An ack is not treated as a commit: the snapshot's `committed_serial` comes from cached `current().last_acked`. The fixture checks this directly (`fluxwm-fixture/src/main.rs:2742-2760`).

**Coalescing back to the sent state.** `has_pending_changes` compares `server_pending` with `current_server_state()`, which is the last pending configure if there is one, otherwise `last_acked`. If the layout returns to the state already sent, nothing is resent. The fixture covers this at `main.rs:2763-2784` with resizes of +16, +16, −16 followed by an ack, and asserts no echo.

**Queue boundedness.**
- A mapped role has at most one configure from fluxwm in flight.
- After a null-buffer unmap, Smithay (`mod.rs:1645-1647`) keeps the old pending list. The new initial configure is sent only on the next bufferless commit. That allows at most old + initial = 2, and acking the initial one retires both. The fixture checks this at `main.rs:2830-2863`.
- Repeated unmap/remap cycles can't grow the list without limit. Each new initial configure needs a commit first, and once a role is mapped, `send_pending` stays blocked until the list is empty.

**Mapped/unmapped remap race.**
- `compositor.rs:39-46` computes `just_unmapped` from `buffered` before and after `commit_window`.
- `handle_commit` (`xdg_shell.rs:304-305`) suppresses the initial configure on the unmapping commit itself.
- `ack_configure` only flushes for `item.mapped` roles, so an old ack after unmap can't send the next cycle's initial configure. The fixture checks this at `main.rs:2788-2805`.
- `relayout` skips roles that aren't mapped (`state.rs:499`).

**Rejected admission.**
- `commit_window` (`state.rs:435-446`) records `admission_error`, leaves `mapped=false` and `buffered=true`, and bumps the sequence.
- A null unmap of a rejected role: Smithay sees `had_buffer_before` (because `last_acked` was set) and resets the role. `just_unmapped` is true, so no early initial configure is sent. The `was_buffered` branch (`state.rs:474-478`) bumps the sequence.
- The next bufferless commit sends a fresh initial configure.

Sequence and initial-configure timing after a rejection look correct. The fixture oracles (`main.rs:3002-3051`) test this on the real wire, checking the configure count before any IPC fields.

**IPC snapshot.** `ipc.rs:89` keeps `mapped`, `buffered`, `admission_error` and the configure snapshot as separate fields. `deferred` is `has_pending_changes()` and `desired_size` falls back to the sent state. These fields mean what they say.

**Runner and defaults.**
- `smoke_configure.py` runs both `configure-stream` and `configure-admission` through `run_case`.
- `run_case` fails if the fixture exit code isn't 0, if `report['ok']` is false, if the JSON can't be parsed, if the compositor dies, on a 20 s deadline, or if endpoints survive cleanup. The fixture gets 5000 ms and the compositor exits after 18 s.
- `smoke_private.py:23` includes `configure` in the default suites.

## Findings

### MEDIUM (proven by reading): a rejected role is only retried when that client commits again
- **Paths:** `state.rs:435-446` and `remove_toplevel` / `relayout` (`state.rs:395-411`, `494-528`); `flux-layout/src/lib.rs:322-328` and `980-1010`.
- **Why:** Admission fails in `check_strip_extent`, which adds up the minimum widths of *all existing columns* plus the new one and compares the total with `MAX_COORD`. So whether a window is rejected depends on the other windows. But the only retry is the `has_buffer && !was_mapped` branch, which runs only on that root's own commit. Nothing retries when another window is destroyed or unmapped, or when the viewport changes.
- **Trigger:** Map windows whose combined minimum widths come close to `MAX_COORD`. Then map a client whose large but valid `min_width` pushes the total over, and which never commits again (for example a static image). Now close the other windows. The rejected window stays buffered, unmapped and invisible forever, with a stale `admission_error`.
- **Closure test:** A fixture case where the rejection is caused by sibling extent rather than one absurd hint. Destroy the siblings and assert the rejected role maps without committing again. Alternatively, write down "client-driven retry only" as the contract. The current `configure-admission` case uses a self-sufficient `1e9` minimum width and recovers only by unmap + new hints, so it can't tell these behaviours apart.

### LOW (inferred from Smithay code, not tested): an old-cycle ack lets the client attach a buffer without a new initial configure
- **Paths:** Smithay `mod.rs:1617-1626`, together with keeping old serials (`1645-1647`).
- **Trigger:** The client unmaps with configure S1 in flight, acks S1, then commits with a buffer *before* any new initial configure. `role.last_acked = S1` gets past the `UnconfiguredBuffer` check.
- **What fluxwm then does:** It maps the role. `relayout`'s `send_pending` is a no-op because `initial_configure_sent` is false, but `handle_commit` then sends the initial configure in the same dispatch. So fluxwm recovers and stays bounded.
- This is the library serial-retirement behaviour the contract says to keep. Fluxwm doesn't reject this protocol violation and no fixture covers it. A closure test would assert either a protocol error or a single, correctly sized configure after mapping.

### LOW (unreachable today): grab state could leak into the next cycle
If resizing ever becomes reachable, a grab that outlives an unmap would write `Resizing` and size into the reset role's `server_pending` (`resize_grab.rs:143-146`). The next initial configure would carry it, and `relayout` never clears `Resizing`. This is dormant because `floating` is never set.

## Limits of this review
- I only checked the cached/committed side of commit ordering through Smithay's pre-commit hook. I did not trace the post-commit or `Window::on_commit` paths.
- I did not check fixture helpers such as `state.map`, `configure_window` and `protocol_roundtrip`.
- I did not check whether the timing of the `configure-stream` case (130 IPC actions in 5 s) holds up under load.

I didn't write a plan file or edit anything, as you asked.
