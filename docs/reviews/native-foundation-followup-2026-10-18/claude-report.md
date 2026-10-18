No HIGH issues. One MEDIUM remains: it's the partial-init KMS restore you already know about and have documented. The three earlier findings are addressed. The L1 fix added one small new bug, and there are some coverage gaps. This is from reading the code and the pinned Smithay in `upstream/smithay` only. I ran nothing, and I didn't write the plan file because you asked for no writes. The fixture evidence directory isn't in this snapshot, so "7 fixtures pass" and "native tests pass" are still only your reports.

## Earlier findings

**M1 (hotplug retirement restored stale KMS): fixed for every path that goes through `Kernel::retire`.**
- `retire_paused` calls `drm.pause()` before any field drops (`system.rs:447-462`).
- In the pinned Smithay, `pause()` calls `set_active(false)`, which flips the active flag the device shares (`device/mod.rs:414,482`). `AtomicDrmDevice::drop` only restores state while that flag is true (`atomic.rs:238-275`), and the legacy path has the same check (`legacy.rs:129-135`).
- Udev Changed (`system.rs:613`), Removed (`:593`), resume re-prepare (`:527`) and `stop` (`:470-473`) all go through `retire`.
- What's left is under the MEDIUM below.

**M2 (auditor gaps): fixed for everything you listed** (creat/open/openat/openat2, the `/dev` allowlist, calls 303/304, the `d E K V F` ioctl families, proc/fd aliases, the syscall-info ABI and entry/exit checks, PDEATHSIG plus EXITKILL). What's left is LOW below.

**L1 (batches continued after synchronous stop): fixed** in the session and udev callbacks (`system.rs:340-344,375-379`). The batches during `Driver::new` (`:385-402`) still use a plain loop. That's harmless because nothing can reach the driver to stop it before `new` returns. The fix did introduce a new LOW bug (L1 below).

## MEDIUM (accepted and documented, but still real)

**Active DRM device dropped without a pause in `create_gpu`/`prepare`.**
- Any error after `DrmDevice::new` (`system.rs:668`) drops `drm` while it's still active. That covers the resources, GBM, EGL and renderer steps, plus the `handle()`/`insert_source` failures at `:691` and `:717`. Smithay then commits the KMS state it captured at creation.
- `prepare` (`:629-630`) also drops a fully built `Ok(gpu)` without pausing it if `model.complete` fails. That can't happen today, since nothing changes the model during `create_gpu`. But it contradicts the doc's statement that all prepared-GPU retirement goes through `retire_paused`.
- The fix is cheap: wrap `drm` in a guard that pauses it on drop until `OwnedGpu` is built, or call `retire_paused(result.ok(), …)` there.
- `native-foundation.md:40` documents the partial-init case and says it hasn't been run. The `complete()` case isn't mentioned.

## LOW

1. **L1 fix drops the final `SessionActive(false)`.** After `Kernel::stop`, `Lifecycle::stop` sets `closed = true` (`lifecycle.rs:207`). `dispatch_events` delivers `Failure` and then breaks, so the `SessionActive(false)` that follows is never delivered. This happens in the pause error path (`:488-494`), the activate error and input-failure paths (`:501-522`), and the udev path when the model closes (`:603-608`). For the pause error path, the consumer was last told the session was active. The triggers there are counter exhaustion (rare) or input failure on resume (where the consumer was already told `false`).
2. **Resume emits no `GpuRemoved`.** `activate` retires and re-prepares GPUs without emitting `GpuRemoved` for the old generation (`:525-529`), while udev Changed does (`:615`). The event contract is inconsistent.
3. **Extra master-lock release on resume.** `retire` on resume pauses an already paused device. `pause()` releases the master lock again, and the fd's own drop does it once more (`fd.rs:23-31`), so expect an extra `error!` log or two. Only noise.
4. **The auditor only treats `/dev/` paths as devices** (`check_native_probe.py:81-87`). Device nodes elsewhere get through: bind mounts, `mknod` (133/259 aren't denied), `/sys/bus/pci/*/resource*`. The doc's "denies device paths" overstates this. Checking `stat` for `S_ISCHR`/`S_ISBLK` plus an rdev allowlist would fix it.
5. **Paths are resolved in the tracer's context.** Proc aliases are only rewritten when they're an exact leading prefix (`:70-74`). `/proc/./self/…`, `//proc/self/…`, or `openat(dirfd=/proc, "self/fd/N")` would resolve to the tracer's own fds.
6. **Some syscalls aren't denied:** `pidfd_getfd` (438), `sendto`/`sendmsg` with an address (44/46), and the mount-API calls (428-433). Most of these need privilege or are partly caught by the ioctl-family and fd-target checks. Fine for a trusted binary; worth noting.
7. **The harness hash isn't pinned.** `harness_unchanged` only compares the file's hash before and after the run (`:113,252`). Neither script checks it, or the fixture binary hash, against the committed `tools/` file.
8. **The fixture test is fragile.**
   - It matches `strstr` against the full `argv[0]` path, so an `--artifacts` path containing "creat", "handle", "console" and so on (e.g. `…/creation…`) breaks or masks cases.
   - It only checks `reason in observed`. It doesn't check that the child got `EPERM`, or that exactly one call was blocked.
   - It runs `tools/check_native_probe.py` relative to the current directory, without `-I`.
   - The compat case needs IA32 emulation enabled on the host.

## Test limits

**Native tests.** `retirement_pauses_before_drop…` checks the generic helper with a fake type. It doesn't touch the real `DrmDevice`, `Kernel::retire`, the `create_gpu` failure paths, or real KMS. `notification_shutdown_cancels_remaining_batch` tests only the helper, not the `Kernel` event batches, so it misses LOW 1. Kernel/Driver activation, udev, libinput pinning, real hotplug, VT switching and broker or provider faults have no executed coverage, as the doc says (`native-foundation.md:44,56`).

**Probe health check** (`probe.rs:96-131`). This is reasoning, not a run: the check requires an active, unclosed session, at least one ready GPU, zero failures and zero close failures. It errs on the strict side. A secondary GPU that fails, or a VT switch at exit, makes it report unhealthy. Its unit test covers only that predicate.

**Auditor fixtures.** They exercise creat, openat on a vendor path, openat on a console path, one `K`-family ioctl on `/dev/null`, `open_by_handle_at`, i386 `int 0x80`, and the normal pass case. Nothing exercises:
- `open(2)`, `openat2`, the `/proc/self` and `/dev/fd` aliases, or dirfd-relative paths
- the `d`/`E`/`V`/`F` ioctl families or the fd-target ioctl check
- `name_to_handle_at`, connect, clone/fork/clone3, a later exec, io_uring, or the x32 bit

The auditor covers `--check` only. `--session` has not been run. Nothing here shows production, hardware or display safety.
