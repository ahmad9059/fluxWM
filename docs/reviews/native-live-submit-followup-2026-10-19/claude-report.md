The three corrections hold up in a static read of the snapshot, and I found no new reachable HIGH or MEDIUM defect. This is not a certification: nothing was run, and the native physical paths are still unexecuted. I used 12 Read/Grep calls and wrote nothing, not even a plan file.

## Correction checks

**M1: initial commit and rollback are now synchronous. Confirmed.**
- `apply_blocking` (`system/atomic.rs:107`) sends only `ALLOW_MODESET` with cookie 0, so there is no `NONBLOCK` and no `PAGE_FLIP_EVENT`. A unit test asserts those flags are absent.
- The initial modeset goes through it at `system/scanout.rs:182`, and so does rollback at `scanout.rs:314`.
- `FrameQueue::submit_using` takes ownership of the frame as uncertain before the syscall runs (`frames.rs:344-346`):
  - On `Accepted`, `complete_blocking` promotes it only if the cookie matches the exact pending one (`frames.rs:387-391`).
  - On any syscall error, `Uncertain` leaves the frame pending with `faulted=true`. Later submits are refused and only the restore path can release it.
- Later submits use a request that sets only `FB_ID` (`target_test.rs:416-424`) with `NONBLOCK|PAGE_FLIP_EVENT`.
- In the probe:
  - The 15 s timer is registered before `begin` (`probe.rs:212-223`).
  - The second frame is submitted only after a clean initial result (`last_failure.is_none()`).
  - Each flip is counted only when `front_cookie` matches the event cookie and is not a repeat.
  - The third frame is submitted from inside the callback.
  - Health requires exactly 3.
  - Flip completion runs before the probe's callback, and the kernel borrow is released before `notify` (`system.rs:1010-1045`), so the nested `submit_preview_frame` cannot panic on a double borrow.
  - There is no keep option, and the help and JSON both say `universal_shutdown_deadline:false`.

**M2: clones are tested with CLOSEFB before any is kept. Confirmed.**
- `qualified_clone` (`restore.rs:365-374`) does create probe → CLOSEFB on the probe → create the kept clone.
- If CLOSEFB fails, the probe is removed with RMFB. That is safe because the probe was never attached to a plane. If that RMFB also fails, the probe stays in `framebuffers` with `restored=false`, so Drop retries it as an RMFB.
- The error comes back as `Capture`, and `begin_output_preview` retries the next format only after `Prepare` or `Test` failures (`system.rs:526`). A capture or cleanup failure ends the whole start, with no live modeset.

**LOW: GETFB2 handles closed twice. Confirmed fixed.**
- The handle guard is installed only after `ioctl(0xce)` succeeds (`restore.rs:209-213`).
- `Handles` uses a set to de-duplicate handles and empties it as it closes them. The explicit close plus Drop therefore cannot close a handle twice, and a failed close is counted rather than retried.

## Remaining gaps (none is a new HIGH or MEDIUM)
1. **Late cleanup failure.** If `finish_restored` fails after a successful rollback, `armed` stays true and `finish_output_preview` returns before `gpu.output.take()`. `Drop` then forgets the state, so the device fd stays open until the process exits. At exit the kernel removes the restored clone and the restored output can blank. This is containment that lasts only as long as the process, as the brief describes. The docs match "blank / process exit / unqualified" (`docs/development/native-live-submit.md:15,19,23`), but those lines were too long to display, so I did not read the full wording.
2. **Rollback error after `mark_restored`.** The same lifetime-only containment applies, and preview health fails.
3. **The rollback `TEST_ONLY` check is weak.** It runs before the initial modeset is applied, so it checks the rollback against the current state, which it equals. It does not prove the rollback is valid from the new state. This gap predates these corrections.
4. **Kernel support.** Capture now needs CLOSEFB support (Linux 6.8+). Older kernels fail safely at `Capture`.
5. **Driver assumptions.** Treating a successful blocking commit as proof the old frame is no longer scanned out assumes the driver follows the helper-based commit path. No driver or hardware has been qualified.

**Verdict:** M1, M2 and the GETFB2 issue are correctly addressed in the snapshot. The remaining risks above are LOW or already disclosed, and the hardware behaviour is still unverified.
