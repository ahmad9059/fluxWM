# Review: native live KMS submission and always-reverting preview

I only read and searched the snapshot. I wrote nothing, ran nothing, and didn't execute any tests or a physical session. This review is not a certification.

## Findings

### MEDIUM 1 — The first live commit can fail even after both TEST_ONLY checks pass, when it touches a CRTC that stays off

- **Where:** `target/routes.rs:80-100`, `target_test.rs:347-351`, `atomic.rs:141-152`.
- **Cause:** `reconcile` adds a source CRTC to `disabled_crtcs` (ACTIVE=0, MODE_ID=0) without checking whether it is already inactive. Changing the connector's or plane's routing also pulls that old CRTC into the atomic state.
- **What the kernel does:** It creates a vblank event for every CRTC in the state when `PAGE_FLIP_EVENT` is set. `drm_atomic_crtc_check` then rejects any CRTC that requests an event but is off before and after the commit ("requesting event but off", `-EINVAL`).
- **Why preflight misses it:** TEST_ONLY can't be combined with `PAGE_FLIP_EVENT`, so neither TEST_ONLY run sees this check.
- **Trigger:** A connector or primary plane is routed to a CRTC that is DPMS-off, with routing kept but ACTIVE=0. This happens after fbcon console blanking or after a previous master DPMS-off. The assignment then picks a different CRTC.
- **Outcome:** The live commit always fails. It is recorded as Uncertain, so containment works: the frame stays owned, rollback runs at the timer, and the probe reports unhealthy. But the preview can never succeed in that state, and the docs don't list this preflight/live mismatch.

### MEDIUM 2 — "Retained ownership" doesn't survive process exit, and exit can disable restored planes

- **Where:** `restore.rs:235-271`, `scanout.rs:316-325`, `system.rs:698-705`, doc line 15.
- **Cause:** If CLOSEFB fails after a successful rollback (for example on kernels before 6.8, or on any CLOSEFB error), the cloned FBs stay registered to our file and are now on screen. Shutdown then retries, fails again, and `forget`s the state.
- **What happens at exit:** The probe exits right after this. When the last fd closes, the kernel's `drm_fb_release` removes every FB this file still owns that has extra references. That path disables the planes and CRTCs using them, which is effectively an RMFB of the restored scanout.
- **Outcome:** The restored console or foreign output goes dark at exit. The retention only lasts until the process exits, not a durable safeguard. The same exit-time cleanup applies to any state abandoned on pause or hotplug (that case blanks the leaked preview, which is benign). The docs should say this explicitly. A real fix would be one of:
  - check for CLOSEFB support before starting a live modeset and refuse otherwise, or
  - restore using the original foreign FB IDs, which needs a different lifetime argument.

### LOW — On a GETFB2 error, the code closes handle numbers the kernel already deleted

- **Where:** `restore.rs:207-226`.
- **Cause:** On error, kernel `getfb2` deletes the handles it created but still copies the handle numbers back to userspace (`drm_ioctl` copies out even on error). `Handles` then GEM_CLOSEs those IDs.
- **Outcome:** Usually this just produces EINVAL, which is counted as a cleanup error and surfaced as a capture failure. In theory, a Mesa helper thread sharing the same file description could reuse a freed ID in between, so the close would hit someone else's handle. The window is narrow. The fix is to install the guard only when `read` succeeds.

## Checked and found sound (statically)

- **Raw ioctl numbers and structs:** All match the supplied `drm.h`: GETPROPBLOB 0xAC, CREATEPROPBLOB 0xBD, ADDFB2 0xB8, GETFB2 0xCE, CLOSEFB 0xD0, ATOMIC 0xBC. Each call uses `read_write::<T>` with the matching repr(C) struct. Pointer arrays and the blob buffer stay alive through each synchronous call, and only the scalar cookie outlives the call.
- **Blob cloning:** The two-call GETPROPBLOB sequence and the size check are correct. `ValueType` mapping from the pinned drm-rs (`control/mod.rs:493-506`) classes MODE_ID as Blob, CRTC_ID as CRTC, and FB_ID as Framebuffer, so the allowlist and type filter behave as intended.
- **GETFB2 handles:** Shared-object handle deduplication matches the kernel. Handles are closed after ADDFB2, which keeps the GEM references inside the cloned FB.
- **Buffer reuse:** There is no early reuse. Every frame gets a fresh GBM buffer, FB and blob. The old front is released only when a cookie, ticket and CRTC all match a completion. A frame becomes pending before the syscall, every syscall error is Uncertain, and Rejected is only used before the ioctl.
- **Completion identity:** Cookies come from a global monotonic counter. `source_matches` checks device, generation, session_epoch and CRTC. Events from other CRTCs in the initial commit are ignored, and the probe's `preview_last_cookie` and `front_cookie` checks prevent double counting. Late events after rollback hit a closed or absent queue and are ignored. `RefMut` temporaries in the event callback drop before `notify`, so the probe's re-entrant `submit_preview_frame` does not hit a borrow panic.
- **Restoration:** Rollback is blocking with ALLOW_MODESET and no NONBLOCK, and the kernel waits for earlier commits, so `IdleProof` placement is justified. `mark_restored` comes before the ioctl, so an ambiguous error never leads to RMFB. Both restore and cleanup can be retried, and the shutdown retirement path retries them. If capture or preflight fails, RMFB only touches clones that were never scanned out. A clone MODE_ID with identical contents does not force a modeset.

## Remaining limits (already disclosed or inherent, not implemented behaviour)

- The rollback TEST_ONLY runs against the old state, not from the new state, so it doesn't prove the rollback will succeed after the commit.
- No restore happens on pause or hotplug. Because the seat lease is leaked, reopening the same device for the rest of the process will probably fail (logind/seatd may refuse a second take).
- Abandoned states permanently consume pool quota.
- Synchronous calls have no deadline. The timer is armed before `begin`, so a slow `begin` can cut short the three-frame sequence.
- The rollback request only covers properties in the request. One preview per GPU, no multi-output transactions, no compositor backend.
- No physical `--session` run has happened. Everything above is static reasoning against the pinned bindings and my reading of the kernel source.

## Verdict

I found no HIGH defects in buffer ownership, rollback-clone lifetime, ioctl encoding or ticket identity. There are two MEDIUM issues to fix or document before the first physical run:

1. The live commit can fail after both TEST_ONLY passes when a CRTC in the state stays off.
2. Without working CLOSEFB, restored scanout goes dark at process exit.

There is also one LOW cleanup-ordering nit in GETFB2 error handling.
