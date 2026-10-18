# Native session and device foundation

`flux-native` begins phase 3 with an opt-in Linux platform layer. The main `fluxwm` executable still accepts only `--backend nested`, and `capabilities.native_session` remains false. No native compositor, output frame, mode transaction or physical-input qualification is claimed.

The default crate builds a device-independent lifecycle model. Feature `system` adds pinned Smithay libseat, udev, libinput, DRM, GBM and GLES integration plus the separate `fluxwm-native-probe` executable. It does not change the nested runtime or advertise additional Wayland globals. Existing locked package versions are retained; native support adds dependencies and feature edges.

```sh
cargo test -p flux-native --locked
cargo check -p flux-native --features system --all-targets --locked
cargo test -p flux-native --features system --locked
cargo build -p flux-native --features system --locked
target/debug/fluxwm-native-probe --check
python3 tools/check_native_probe.py --artifacts target/native-probe-audit
python3 tools/test_native_probe_audit.py --artifacts target/native-probe-denial-tests
```

`--check` enumerates udev device IDs without opening a seat, DRM node, input device or GPU context. Tests set an invalid libseat backend and still require this mode to succeed. The Linux x86-64 ptrace auditor launches only its own child, uses kernel-reported syscall entry/exit and ABI information, resolves open/creat/openat/openat2 paths (including child-relative proc/fd aliases), and denies paths under `/dev` except null/zero/full/random/urandom. It also rejects file-handle access, DRM/input/VT/vendor ioctl families, connects, later exec, process/thread creation and io_uring. It checks binary and harness hashes, child reaping and the parent environment. The latter checks parent preservation, not child environment safety: the child inherits the invoking environment except for an invalid libseat backend. Run with a trusted environment and executable. Parent-death signalling covers the interval before ptrace EXITKILL is installed. Fresh artifact directories are required; reruns must use a new path. This is a regression auditor for a trusted binary, **not a general security sandbox** or proof about arbitrary executables/ABIs. A ptrace-unavailable environment fails the audit rather than recording a qualification pass.

## Resource and event ownership

A native `Driver` owns the seat/session dispatcher independently of its event-loop registrations. Event callbacks capture weak kernel ownership. Source-registration guards remove sources before their associated graphics objects retire. The driver retains a maximum of sixteen discovered devices; capacity rejection preserves existing devices.

A device has a monotonically increasing generation, while session activation/pause advances a separate epoch. Discovery is idempotent; Changed, removal/reuse, pause/resume and shutdown invalidate old preparation or ready tickets. Counter exhaustion seals admission instead of wrapping. Failed initialization retries only on explicit device change or a session transition. Preparing contexts and consuming their DRM events require current active ownership. Ready means **GPU context prepared**, not a mode, output, successful presentation or desktop readiness.

The adapter acquires devices through libseat, verifies node identity, enforces CLOEXEC/NONBLOCK before duplication, and checks the opened node is the expected primary DRM device. It retains the broker's original descriptor while Smithay owns private duplicates for DRM, GBM and EGL/GLES. The original is closed through `Session::close`; dropping an OS descriptor alone is insufficient for broker bookkeeping. Close failures have a bounded counter; they are not labeled successful cleanup.

The pinned libseat adapter ignores requested open flags. The pinned calloop dispatcher stores its source before its callback, and can retain a removed source through the current callback. A shared `pin_lease` helper therefore holds the broker/session owner in the callback until the notifier's descriptor retires. This also protects reentrant removal or driver drop. The mock resource test executes actual calloop self-removal and requires graphics cleanup, callback return, notifier retirement, broker close and seat-owner retirement in that order. Input callbacks similarly retain session ownership until libinput's dispatch-pinned context retires. A separate libinput session adapter applies the same FD flag enforcement, transfers the original descriptor to libinput, and accounts broker close failures when libinput returns it. An actual O_PATH descriptor supplies the kernel flag-setting failure test; no invalid OwnedFd is manufactured.

A pause-on-drop guard covers partial GPU initialization failures until prepared ownership is handed off. Failed lifecycle completion also retires the constructed GPU through a pause. Changed/removal/stop retire prepared GPUs through a pause-before-drop helper, suppressing Smithay’s implicit active-device KMS restore from stale connector snapshots. A device-free drop oracle checks that ordering; real hotplug and KMS effects remain unexecuted. Notification batches suppress later ready events after a callback seals the driver, while still delivering failure diagnostics and the final inactive transition. Resume emits removal before replacement readiness for each prior generation.

Pause gates work before suspending libinput and pausing DRM contexts. Resume recreates GPU contexts and renegotiates their available format sets; this foundation has no scanout buffers to preserve. A future output implementation must extend this ownership domain to pending/front buffers, page-flip completion, output globals and scene publication before it can claim safe hotplug/VT behavior. Metadata exposes device/generation IDs and counts, not EDID serials, native input contents or source paths. Raw import/render format set sizes are reported separately; they are not negotiated scanout formats or a DMA-BUF feedback policy.

## Explicit manual initialization mode

```sh
# For a separately authorized physical-VT test, not the active host desktop:
target/debug/fluxwm-native-probe --session
```

This mode opens a real libseat session, libinput context and primary-node DRM/GBM/EGL/GLES contexts. It registers session, udev, input and DRM event sources and counts aggregate events. It creates no compositor output surfaces. **Native initialization and release still exercise real DRM ownership**, so this is not a display-preserving diagnostic command. It requires an ordinary session user on an appropriate separate VT and a supported seat provider. Reported initialization or cleanup failures produce a nonzero exit after the diagnostic JSON. Healthy initialization additionally requires an active, unclosed session and at least one current prepared GPU; an empty or inactive inventory fails. This is still not presentation readiness or hardware qualification. It has not been executed as part of the device-free checks.

The exit timer provides a fifteen-second event-loop interval after initialization; kernel/provider/vendor operations have no universal deadline. SIGINT/SIGTERM are blocked before GPU initialization and remain handled through owned cleanup. No acknowledgement or safe mode-preview transaction exists yet: the phase-3 preview/revert requirement remains open before implementing a display-changing preview.

Pinned libseat initialization/notifier code contains internal `unwrap` calls. Provider-loss/panic behavior, backend errors, actual broker-close failure and native device revocation remain hardware/fault-injection work. The local mock tests do not qualify those upstream paths.

## Build and qualification limits

The local build uses existing libseat/libudev/libinput/GBM/libdrm development libraries. No host packages are installed by these checks. The separate Ubuntu 24.04 CI job installs corresponding development packages and runs only device-free tests and `--check`. Package names are documented by Ubuntu's [libseat](https://packages.ubuntu.com/noble/armhf/libseat-dev), [libinput](https://packages.ubuntu.com/noble/libinput-dev), [GBM](https://packages.ubuntu.com/noble/libgbm-dev) and [DRM](https://packages.ubuntu.com/noble/libdrm-dev) indexes. This configures CI; it does not establish an executed hosted build or Ubuntu native-session support.

The following remain required: integrating the native driver into `fluxwm`; output/connector/CRTC selection; primary-plane allocation and composition; mode test/commit/fallback and fifteen-second safe preview; page-flip/scene/frame ownership; DMA-BUF feedback and buffer release; native keyboard/pointer/device policy; outputless recovery; multi-output/hotplug/VT/suspend; physical error/recovery measurements; and the font/shaping spike. Full [phase 3](../phases/03-gpu-backend.md), the [production gaps](production-status-2026-10-18.md) and all other phase gates remain open.

## Independent review and remaining test gaps

Claude independently reviewed frozen source `0647fb3` using only read tools. It found stale-state restore on hotplug retirement and incomplete auditor device/syscall coverage. Both received code changes and regression checks; the reviewed snapshot predates those changes. Later author checks do not retroactively qualify that snapshot. The audit fixtures exercise actual owned-child syscalls for creat, vendor/console paths, VT ioctl, file-handle access and the i386 compatibility ABI; a normal fixture must pass.

The pure lifecycle, descriptor adapters, source-removal helper and notification/retirement helpers have device-free tests. Full `Kernel`/`Driver` activation, udev handling, libinput dispatch pinning and their integration still lack a fake provider capable of driving the real callbacks. In particular, the existing ready-epoch mutation does not prove that deleting a DRM callback admission check fails tests. The reentrant GPU test does not qualify dropping the real Driver or session dispatcher during an input callback. These are explicit coverage gaps, alongside the physical and provider-fault gates above. JSON scope fields are declarations; the syscall audit supplies independent evidence for the read-only command's observed accesses. Hosted CI artifact retention and an executed hosted build remain unverified.

The frozen follow-up accepted the earlier fixes but found partial-initialization restore still reachable and final inactive notification suppression. Later author code guards partial initialization and failed completion, preserves terminal events after sealing, avoids pausing already inactive devices on retirement, and emits generation removal on resume. These later changes have focused helper tests but were not part of that frozen read. Fixture dispatch now matches the executable basename so artifact directory names cannot accidentally select extra cases.

The auditor does not qualify arbitrary device nodes outside `/dev`, bind mounts, sysfs resources, noncanonical proc aliases, namespace tricks or every privileged/descriptor-transfer syscall. Its fixtures assert audit failure and reason, not every child-side errno; many syscall families remain unexercised. Compatibility-entry testing depends on host kernel support. Hash comparisons establish within-run stability, not an independently pinned trust root. These limits are consistent with trusted regression auditing and prohibit treating it as a security boundary.

The next component, [native output discovery and candidate routing](native-outputs.md), now integrates bounded connector/mode/encoder/CRTC/primary-plane discovery and route planning into optional GPU preparation. It adds no surface, allocation, KMS commit or native desktop capability.
