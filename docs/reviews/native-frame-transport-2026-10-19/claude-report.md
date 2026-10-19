# Independent review: native frame transport (frozen worktree snapshot)

I found no reachable HIGH or MEDIUM defects in the areas you scoped. This review was static only: I ran nothing, so no tests, build, Miri or hardware checks.

## Findings

**No HIGH or MEDIUM findings.** What I checked, and what closes each risk:

- **Resource lifetime and quota (`frames.rs`)**
  - The quota permit is created before `allocate` runs, so a backend error or panic gives the slot and bytes back (`frames.rs:128-132`).
  - The consuming flag is set and the frame is stored as pending before the platform callback runs (`frames.rs:344-346`). If that callback panics, the frame stays held and the old scene stays hidden. The test at `frames.rs:590` covers this.
  - An unexpected `Drop` leaks the payload, its Rc and the quota slots, as you intended. It only frees the output key (`frames.rs:187-190, 205-217`).
  - None of the `RefCell` borrows can re-enter: `Permit::drop` only borrows after the resource and scene have already been dropped.
  - The byte check `state.bytes > MAX - bytes` can't underflow, because the line before it already rejects `bytes > MAX`.
  - In `release_after_disable`, both frames are marked non-consuming before either is dropped, so a panic inside an `R::drop` can't orphan the other frame.
- **Cookie matching**
  - Matching needs the device, generation, session epoch, CRTC and the exact non-zero pending cookie (`frames.rs:370-379`).
  - The cookie counter is process-wide and never wraps. When it runs out, the queue faults before the platform callback is called.
  - An old output-epoch cookie can't complete a queue that reuses the same CRTC (test at `frames.rs:718`).
- **Untrusted event bytes (`drm_events.rs`)**
  - The cursor and length arithmetic is checked, and `end` can't pass the end of the buffer.
  - Flip events must be exactly 32 bytes, so every fixed-offset read is in bounds. Byte-wise `from_ne_bytes` means alignment doesn't matter.
  - A batch with any bad event delivers nothing, including any valid events before it.
  - The kernel's `drm_read` never splits events, and a 4096-byte buffer can't trigger its zero-length return for oversized events.
- **Event reader and callback lifetime (`system/events.rs`, `system.rs:770-883`)**
  - The fd is forced non-blocking (`system.rs:168`). Each dispatch does one bounded read. `EAGAIN`/`EINTR` just continue, and any other failure disables the source for good.
  - Smithay's notifier is dropped, so this reader is the only consumer of that DRM file's event stream.
  - `pin_lease` keeps the seat device alive for as long as the source exists. `_source` is the first field of `OwnedGpu`, so it is dropped first.
  - The callback releases its `kernel` borrow before calling `notify`.
- **Raw ioctl and ABI (`system/atomic.rs`)**
  - The arrays are local and stay alive through the synchronous call. Counts and values line up by construction.
  - Object and property limits are rejected without partially adding the entry (test at `atomic.rs:157`).
  - The opcode is IOWR('d', 0xBC) on a 56-byte struct. The only kernel call uses TEST_ONLY|ALLOW_MODESET with cookie 0, and no property makes the kernel write back to user memory.
  - The 352-object limit equals 64 connectors + 32 CRTCs + 256 planes. Going over the limit returns `TestStage::Budget` (`target_test.rs:83-99`).
  - The `connector_properties[...]` lookups can't panic, because `reconcile` rejects connectors it doesn't know (`routes.rs:58`).

## LOW findings and notes

1. **LOW – partial ABI coverage.** The decoder hard-codes `tv_sec` at 16, `tv_usec` at 20 and `sequence` at 24 (`drm_events.rs:61-77`). Neither `atomic.rs:179` nor `tools/check_drm_abi.py` checks those offsets; they only check the struct sizes, the `user_data` offsets and `crtc_id`. The kernel ABI is stable and these fields don't decide ownership, so the risk is low. But the docs say the ABI is "compared", and that's only partly true.
2. **LOW – events dropped while the ticket isn't ready.** The callback at `system.rs:840` throws away both completions and reader failures when `!is_ready(ticket)`. Two things keep this safe:
   - Every lifecycle change bumps the epoch (`lifecycle.rs:91,133,145`), so a ticket that stops being ready never becomes ready again.
   - `OutputKey` is tied to the session epoch, so a lost completion can't free anything. The frame just stays pending until an `IdleProof`.

   There is one cosmetic gap: a read failure in that window doesn't set `event_transport_healthy = false` on the stale GPU info.
3. **Design limit for the future submitter (not a defect).** Each `FrameQueue::submit` allocates its own cookie. One atomic commit covering several CRTCs has a single `user_data` and sends one event per CRTC. So combined multi-output commits will need an API change, or must stay one commit per queue.

## Test coverage and limits

- **Covered by fixtures:** event decoding (malformed input, truncation, fuzz-like input, the 64-bit maximum cookie, sequence wrap), dispatch over an owned socket, the ledger's admission, rejection, uncertainty, panic, exhaustion and stale-epoch paths, and the serializer's array order and limits.
- **Not exercised:** real DRM reads, the real atomic ioctl, any live submitter, and wiring the ledger to real scanout resources. Session transitions on hardware and cross-process cookie freshness also remain unverified. The last one depends on the platform caller, as you described.
- **Not checked:** I didn't confirm that the toolchain supports `AtomicU64::try_update` (`frames.rs:29`).

## Verdict

The transport primitives look internally sound for the scope you described. They fail closed on lifetime and identity, and the reader's byte handling and the raw ioctl look correct. This is not a production-readiness certification: the live submitter, the ledger wiring and hardware execution are still missing.
