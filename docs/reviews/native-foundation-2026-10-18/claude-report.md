I found no HIGH defects. There are two MEDIUM findings: one in the driver and one in the auditor. Separately, several tests and oracles check less than they appear to. Everything below comes from reading the code only, within this directory. I ran nothing, wrote nothing (including no plan file) and used no agents.

## MEDIUM findings

**M1. A monitor hotplug during `--session` makes a KMS commit at runtime, not only at teardown.**
- **Where:** `crates/flux-native/src/system.rs:559-563` and `system.rs:586-591`; upstream `smithay/src/backend/drm/device/atomic.rs:238-273` and `legacy.rs:124-136`.
- **Trigger:** udev sends a `Changed` event for the DRM card on connector hotplug. Unplugging or replugging a monitor during `--session` is enough.
- **What happens:**
  1. `Kernel::udev` calls `model.changed` and then `retire(id)`.
  2. Retiring drops the `DrmDevice` while it is still active. The session is not paused, so the active flag is still true.
  3. Smithay's `Drop` then sends a restore commit of the CRTC/connector/plane state it saved when this `DrmDevice` was created. That state can be stale, for example when the connector it refers to has just been unplugged.
  4. Any restore error is only logged upstream. It never becomes a `Failure`, so the probe can still report `initialization_healthy`.
- **Why it matters:** `docs/development/native-foundation.md:37` only says Smithay may restore state "during teardown". The scope also says this stage implements no modesets.
- **Paths that do not trigger it:** on resume, `pause()` already ran `drm.pause()`, so the restore is skipped. Removal while paused is also safe.
- **Fix:** either `pause()` the `DrmDevice` before `retire` on `Changed`/`Removed` while active, or document that the probe commits KMS state on hotplug.

**M2. The auditor misses some ways to reach native devices, so a regression could still pass (`tools/check_native_probe.py`).**
- **Unchecked open-like syscalls:** only syscalls 2, 257 and 437 are checked (`:162`).
  - `creat` (85) opens an existing char device write-only. `creat("/dev/dri/card0")` is neither counted nor blocked.
  - `open_by_handle_at` (304) is also unchecked, though it needs `CAP_DAC_READ_SEARCH`.
- **Incomplete device list:** `privileged()` (`:67-70`) leaves out `/dev/console`, `/dev/fb*`, `/dev/hidraw*`, `/dev/uinput`, `/dev/vcs*` and NVIDIA's `/dev/nvidia*` / `/dev/nvidiactl`.
- **Incomplete ioctl check:** only ioctl types `'d'` and `'E'` are blocked (`:175`). VT ioctls (`'K'`, `'V'`, e.g. `KDSETMODE` or `VT_ACTIVATE`) on `/dev/console`, and NVIDIA's `'F'` ioctls, are allowed.
- **Concrete false pass:** on an NVIDIA host, a regression that made `--check` create an EGL display would open `/dev/nvidia*` and issue `'F'` ioctls. The audit would still pass. The same is true for a regression that opens `/dev/console` and switches the VT mode.
- **Scope:** the doc does scope the check to "ordinary open/openat/openat2", but it also claims the auditor "rejects native device/TTY access", which is broader than what is enforced.

## LOW findings
- **L1. Events still delivered after `stop()`.** The callbacks build their event list first and then deliver all of it (`system.rs:339-342`, `371-375`). If a handler calls `driver.stop()` or drops the `Driver` mid-loop, later items such as `GpuReady` for a GPU that was just retired are still delivered.
- **L2. Paths are resolved from the auditor's view, not the child's.** Opening `/dev/fd/N` or `/proc/thread-self/...` resolves against the auditor's own descriptors (`:59-62` only rewrites `/proc/self/`). This only matters once the child already holds a device fd, and that first open would already be caught.
- **L3. The auditor cannot detect 32-bit syscalls.** An `int 0x80` call shows up with i386 syscall numbers, which `GETREGS` cannot tell apart from 64-bit ones. This is acceptable for a trusted binary but is a real gap.
- **L4. `Driver::new` cannot be interrupted.** SIGINT/SIGTERM are blocked first (`probe.rs:46`), so a hang during setup can only be ended with SIGKILL. This matches the "no universal deadline" note.

## Test and oracle gaps
1. **The `--check` JSON fields are hardcoded literals** (`probe.rs:36`). So `tests/probe.rs:16-18` and the auditor's checks at `:200-202` only prove which code path ran. They do not observe that no session or GPU was opened; only the syscall trace does that.
2. **`initialization_healthy` can be true with nothing initialized** (`probe.rs:96`). It does not require an active session or at least one GPU, so an inactive seat with zero GPUs still reports healthy.
3. **The `Kernel` and `Driver` logic has no tests.** `udev`, `activate`, `pause` and `stop` need libseat, so none of them run under test. These mutations would all pass the suite:
   - removing `pin_lease(seat_owner, …)` from `init_input` (`system.rs:524`);
   - removing the `is_ready(ticket)` check in the DRM callback (`:673`);
   - removing the `model.active()` check in the input callback (`:528`).

   The author's "ready epoch guard" mutation only covers `Lifecycle::is_ready` (`lifecycle.rs:274-279`), not these callback checks.
4. **The reentrant-removal test only covers part of the shutdown path.** The test at `system.rs:962` does use the production `OwnedGpu`, `pin_lease`, `Registration` and `SeatDevice`, so it is not a copy of the controls. But it does not cover the session dispatcher being removed by itself, the input/libinput pin, or dropping the `Driver` from inside a callback.
5. **Auditor evidence is not verified or kept.** `harness_sha256` is recorded but never compared against anything. `parent_environment_unchanged` checks almost nothing, because the child's environment (e.g. `LD_PRELOAD`) is never examined. The CI job does not upload `target/native-probe-audit`, and `mkdir` without `exist_ok` raises outside the try, so a rerun gives no `result.json`.

## Controls I checked that hold
- **libseat adapter** (`libseat.rs:118-158`): it ignores the flags. It holds only a weak session reference, and once the notifier is gone, `close` returns `SessionLost` and the fd is closed by plain drop. Keeping the owner alive is therefore needed, and it is done:
  - `Kernel.seat_owner` is declared last, so it drops after `input` and `gpus`;
  - `SeatDevice._owner` holds it too;
  - the input source's `pin_lease` holds it.
- **calloop pinning** (`loop_logic.rs:267-287`, `516-603`; `sources/mod.rs:301-379`): if a source removes itself during its own callback, the dispatcher stays pinned until the callback returns. It is unregistered after dispatch, and the source drops before the callback because of field order. `pin_lease` relies on exactly this. The upstream dispatch loop calls `seat.borrow_mut()` only as a temporary, so `Kernel::activate`/`pause` running inside the callback cannot double-borrow the seat. Kernel borrows are released before `notify` in every callback.
- **libinput fd handoff** (`input/src/context.rs:58,77`): open moves the fd to libinput with `into_raw_fd`, and close takes it back with `from_raw_fd`. `InputSession::open` takes the fd out of its temporary owner, so the fd is not closed twice. `Kernel::stop` runs `suspend()` before dropping the context, so devices close while the session is still alive, even if a caller still holds `Libinput` references.
- **Fd flags:** `SeatDevice::open` (`system.rs:122-146`) closes through the broker when flag enforcement fails. The O_PATH test really gets `EBADF` from `F_SETFL`.
- **Field order and partial failures:** `OwnedGpu` field order unregisters the source before graphics objects and the broker fd. Errors partway through `create_gpu` release resources in reverse order, with the `SeatDevice` dropping last.
- **Lifecycle model:** generations and epochs never wrap. Exhausting them stops admission (seals) and `Kernel::udev` then stops the kernel. Capacity rejection does not consume a generation. Resume resets Failed devices to Discovered.
- **Probe:** `--check` only calls `device_ids` (udev enumeration). `--session=bad` is rejected.
- **Auditor:**
  - It traces only its own child. Syscall enter/exit tracking stays in step across the initial SIGSTOP and the exec event.
  - Blocking is done before the kernel runs the call: `orig_rax = -1` on entry, then `-EPERM` on exit.
  - Any denied call fails the audit, so an added nonexistent `/dev/dri/...` open would fail it.
  - The kill in cleanup runs only after `waitpid` shows the child has not exited. An exited child stays unreaped as a zombie, so its PID cannot be reused and the SIGKILL cannot hit another process. If ownership of the child is lost (`ECHILD`), it reports `ownership_lost` and sends no signal.
  - Exec failure, an unexpected exit code and bad pointers all make the audit fail rather than pass.

## Limits
- This was a read-only review. I did not run, build or test anything and did not verify the author's mutation results.
- Still open:
  - real libseat/DRM/libinput behaviour, including provider loss, revocation, the upstream `unwrap()`s and a broker close failing for real;
  - NVIDIA and vendor EGL paths;
  - the VT/KMS effects of teardown, beyond reading the code;
  - a hosted CI pass.
- Missing later features (outputs/modesets, page-flip ownership, input policy, integration into `fluxwm`) are out of scope as stated and are not counted as defects. Nothing here amounts to production certification.
