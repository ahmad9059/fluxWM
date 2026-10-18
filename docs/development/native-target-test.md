# Native RGB target allocation and atomic test

The optional driver now has an explicit render-target diagnostic. It allocates compositor-owned GBM RGB buffers, exports DRM framebuffer handles, clears the targets through GLES, waits for rendering, and submits one **atomic TEST_ONLY** request for the selected routes. Targets and mode blobs remain alive through that joint test and retire before the report returns. This implements an allocation/render/testing component of phase 3; it does not present a frame or implement a native desktop.

```sh
# Manual, separately authorized ordinary-user physical VT only. Unexecuted here.
target/debug/fluxwm-native-probe --session --test-outputs
```

`--test-outputs` requires `--session`; it is rejected alone or alongside `--check`. Neither help nor read-only inventory initializes a session. The default `--session` path does not invoke this diagnostic. Existing libseat/DRM initialization can affect display ownership/state, so the whole command is not a display-preserving diagnostic even though the new atomic request is TEST_ONLY. No physical device/session operation was run for the device-free checks.

## Platform and ownership contract

The adapter rejects legacy devices before allocating. Pinned `DrmSurface::test_state(..., true)` on a legacy device can modeset, and dropping an active atomic surface can clear its CRTC. This implementation creates no DrmSurface, DrmCompositor or DrmOutputManager. It uses the pinned `drm-rs` atomic request API with private, fixed `TEST_ONLY | ALLOW_MODESET` flags. No caller selects flags, event mode, nonblocking mode or live commit. A future presentation implementation must add explicit commit/rollback ownership and must not remove TEST_ONLY from this diagnostic.

For each assignment, the adapter rechecks connection state, the exact retained kernel mode, encoder compatibility against fresh resource ordering, primary-plane compatibility and the exact format pair. It reads connector/CRTC/plane properties and rejects non-desktop connectors. Matching handles do not make this an atomic metadata snapshot. Hardware changes can still invalidate the request; the kernel's test result is authoritative only for that request at that moment.

RGB candidates are XRGB8888 and ARGB8888, with XRGB preferred. There is no code aliasing or substitution between implicit/INVALID and explicit modifiers. GBM may internally fall back from Linear to implicit allocation: the returned buffer's exact format and dimensions are checked, and a mismatch is rejected before framebuffer export. Export uses AddFB2 and the modifier flag appropriate to the actual buffer; no legacy AddFB fallback is attempted. Other render-target formats require a separate contract.

The renderer binds the exported DMA-BUF and clears the complete target to opaque black. It finishes the frame and waits on its SyncPoint before the test. Interrupted waits retry at most eight times; failure issues no atomic request. Each underlying kernel/vendor/fence wait may block without a universal deadline. SIGINT/SIGTERM handling and the fifteen-second event-loop interval begin dispatch only after initialization and requested tests complete; that timer is not an initialization watchdog.

The hypothetical request assigns connector CRTC, CRTC ACTIVE/MODE_ID, primary FB/CRTC, fixed-point source and integer destination rectangles, plus normal rotation/opaque alpha when those properties exist. It hypothetically disables other planes currently attached to the tested CRTCs, leaving unrelated CRTCs untouched. It does not claim to solve every pre-existing connector/CRTC conflict or global bandwidth limit. It tests the chosen rendered format combination once; it does not backtrack through all combinations after a failed joint test or choose fallback modes. Those failures are reported, not treated as working output configurations.

Framebuffer and mode-blob guards call explicit destruction before target DMA-BUF/GBM storage retires. Cleanup errors increment a bounded per-test counter, invalidate a successful test result and stop later allocation retries. A failed destruction can leave a kernel object until DRM-client teardown; a zero-error report is not a driver fault-recovery qualification. No raw target or framebuffer escapes the call, and no buffer is submitted for scanout. Renderer caches, kernel resource behavior after incomplete rendering and provider/device loss remain real-adapter qualification work.

## Bounds and diagnostic result

Preflight admits at most 32 targets, 16,384 pixels per dimension and 33,554,432 total pixels (128 MiB nominal at four bytes per pixel). The total nominal byte field excludes stride padding, compression metadata, renderer caches, kernel allocations and allocations attempted during failures. This is **not a hard GPU memory or RSS cap**. Allocation attempts have a global limit of 64 per device test. Candidate lists retain the output planner's 256-pair limit. Property sets and plane enumeration are admitted up to 256 entries each, after the upstream query returns. These limits are not measured target-hardware support limits.

The result reports attempts, nominal bytes, tested route/format metadata, cleanup errors and the failure stage/errno where available. Tested-target success metadata is cleared on any failure. An empty/missing plan and a legacy device fail explicitly. The manual probe tests only its initial ready-GPU snapshot and performs no automatic retest after hotplug/VT changes. Its final health check requires a nonempty successful test record set matching current device/generation/session-epoch keys; stale or missing records cannot report healthy initialization. A successful test says nothing about presentation timing, page flips, future feasibility or full native readiness.

## Device-free evidence and remaining work

The shared production coordinator has fixture tests for admission before allocation, legacy/outputless rejection, exact-pair format fallback, global retry limits, keeping all targets alive through a joint test, retiring partial batches before returning, atomic errors and cleanup failures. The actual private flag function has a TEST_ONLY/no-events/no-nonblock test. Parser and health tests cover explicit opt-in and current GPU tickets. These fixtures do not execute GBM, GLES, framebuffer/blob ioctls, property acquisition, fence waits or atomic testing on hardware. The full Driver callback path and actual resource-destruction failures remain unexecuted.

Before enabling a native desktop, implement front/pending buffer and page-flip ownership, renderer-to-scene integration, output publication/rollback, mode fallback and the required acknowledged fifteen-second safe preview. Preserve outputless recovery and workspaces; qualify startup/shutdown, VT/hotplug/suspend/device loss on physical hardware. No phase gate is closed by this component. See [phase 3](../phases/03-gpu-backend.md), [candidate planning](native-outputs.md) and [current production gaps](production-status-2026-10-18.md).
