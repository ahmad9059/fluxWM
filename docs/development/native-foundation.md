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
```

`--check` enumerates udev device IDs without opening a seat, DRM node, input device or GPU context. Tests set an invalid libseat backend and still require this mode to succeed. The Linux x86-64 ptrace auditor launches only its own child, traces the native syscall ABI, resolves ordinary open/openat/openat2 paths, and rejects native device/TTY access, DRM/input ioctls, connects, later exec, process/thread creation and io_uring. It checks the binary hash, child reaping and parent environment. This is a regression auditor for a trusted binary, **not a general security sandbox** or proof about arbitrary executables/ABIs. A ptrace-unavailable environment fails the audit rather than recording a qualification pass.

## Resource and event ownership

A native `Driver` owns the seat/session dispatcher independently of its event-loop registrations. Event callbacks capture weak kernel ownership. Source-registration guards remove sources before their associated graphics objects retire. The driver retains a maximum of sixteen discovered devices; capacity rejection preserves existing devices.

A device has a monotonically increasing generation, while session activation/pause advances a separate epoch. Discovery is idempotent; Changed, removal/reuse, pause/resume and shutdown invalidate old preparation or ready tickets. Counter exhaustion seals admission instead of wrapping. Failed initialization retries only on explicit device change or a session transition. Preparing contexts and consuming their DRM events require current active ownership. Ready means **GPU context prepared**, not a mode, output, successful presentation or desktop readiness.

The adapter acquires devices through libseat, verifies node identity, enforces CLOEXEC/NONBLOCK before duplication, and checks the opened node is the expected primary DRM device. It retains the broker's original descriptor while Smithay owns private duplicates for DRM, GBM and EGL/GLES. The original is closed through `Session::close`; dropping an OS descriptor alone is insufficient for broker bookkeeping. Close failures have a bounded counter; they are not labeled successful cleanup.

The pinned libseat adapter ignores requested open flags. The pinned calloop dispatcher stores its source before its callback, and can retain a removed source through the current callback. A shared `pin_lease` helper therefore holds the broker/session owner in the callback until the notifier's descriptor retires. This also protects reentrant removal or driver drop. The mock resource test executes actual calloop self-removal and requires graphics cleanup, callback return, notifier retirement, broker close and seat-owner retirement in that order. Input callbacks similarly retain session ownership until libinput's dispatch-pinned context retires. A separate libinput session adapter applies the same FD flag enforcement, transfers the original descriptor to libinput, and accounts broker close failures when libinput returns it. An actual O_PATH descriptor supplies the kernel flag-setting failure test; no invalid OwnedFd is manufactured.

Pause gates work before suspending libinput and pausing DRM contexts. Resume recreates GPU contexts and renegotiates their available format sets; this foundation has no scanout buffers to preserve. A future output implementation must extend this ownership domain to pending/front buffers, page-flip completion, output globals and scene publication before it can claim safe hotplug/VT behavior. Metadata exposes device/generation IDs and counts, not EDID serials, native input contents or source paths. Raw import/render format set sizes are reported separately; they are not negotiated scanout formats or a DMA-BUF feedback policy.

## Explicit manual initialization mode

```sh
# For a separately authorized physical-VT test, not the active host desktop:
target/debug/fluxwm-native-probe --session
```

This mode opens a real libseat session, libinput context and primary-node DRM/GBM/EGL/GLES contexts. It registers session, udev, input and DRM event sources and counts aggregate events. It creates no compositor output surfaces. **Smithay can restore prior DRM state during teardown**, so this is not a display-preserving diagnostic command. It requires an ordinary session user on an appropriate separate VT and a supported seat provider. Reported initialization or cleanup failures produce a nonzero exit after the diagnostic JSON; absence of failures alone is not readiness or hardware qualification. It has not been executed as part of the device-free checks.

The exit timer provides a fifteen-second event-loop interval after initialization; kernel/provider/vendor operations have no universal deadline. SIGINT/SIGTERM are blocked before GPU initialization and remain handled through owned cleanup. No acknowledgement or safe mode-preview transaction exists yet: the phase-3 preview/revert requirement remains open before implementing a display-changing preview.

Pinned libseat initialization/notifier code contains internal `unwrap` calls. Provider-loss/panic behavior, backend errors, actual broker-close failure and native device revocation remain hardware/fault-injection work. The local mock tests do not qualify those upstream paths.

## Build and qualification limits

The local build uses existing libseat/libudev/libinput/GBM/libdrm development libraries. No host packages are installed by these checks. The separate Ubuntu 24.04 CI job installs corresponding development packages and runs only device-free tests and `--check`. Package names are documented by Ubuntu's [libseat](https://packages.ubuntu.com/noble/armhf/libseat-dev), [libinput](https://packages.ubuntu.com/noble/libinput-dev), [GBM](https://packages.ubuntu.com/noble/libgbm-dev) and [DRM](https://packages.ubuntu.com/noble/libdrm-dev) indexes. This configures CI; it does not establish an executed hosted build or Ubuntu native-session support.

The following remain required: integrating the native driver into `fluxwm`; output/connector/CRTC selection; primary-plane allocation and composition; mode test/commit/fallback and fifteen-second safe preview; page-flip/scene/frame ownership; DMA-BUF feedback and buffer release; native keyboard/pointer/device policy; outputless recovery; multi-output/hotplug/VT/suspend; physical error/recovery measurements; and the font/shaping spike. Full [phase 3](../phases/03-gpu-backend.md), the [production gaps](production-status-2026-10-18.md) and all other phase gates remain open.
