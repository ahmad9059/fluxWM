# Review: native RGB target allocation, render and atomic TEST_ONLY diagnostic (frozen snapshot)

I read and grepped files inside the snapshot only. I wrote nothing, ran no commands or tests, used no agents and no network. Plan mode asked for a plan file, but you ruled out writes, so there is no plan file and no edits. This is not a certification. Everything below comes from reading code; nothing was run on GBM, EGL, ioctls or hardware.

## MEDIUM

**M1. The GLES renderer keeps every bound target's dmabuf import after `test()` returns.**
- **Path:** `system/target_test.rs:233-236` calls `renderer.bind(&mut dmabuf)`. In pinned smithay, `Bind<Dmabuf>` first calls `import_dmabuf` (`upstream/smithay/src/backend/renderer/gles/mod.rs:1565`). That always inserts a `GlesTexture`, which holds an EGLImage that pins the buffer memory, into `dmabuf_cache` (`:1322`).
- **Cleanup never runs:** stale entries are only removed in `cleanup()` (`:842-844`). That is only called from `unbind`, `cleanup_texture_cache` and `invalidate_caches` (`:831`, `:2369`, `:2376`). Nothing in the adapter or `Driver::test_output_targets` calls any of them on success or on Render/Sync failure. Bind failure does call `unbind`, but that runs while the dmabuf is still alive, so its entry survives too.
- **Trigger:** any `--test-outputs` run that reaches bind. After the report returns, each successful target's buffer memory stays pinned in the long-lived `Gpu._renderer`, plus one per attempt that failed after import (up to 64 attempts, each up to 128 MiB nominal). The memory is freed only when something later cleans the renderer, which the probe never does.
- **Claims this contradicts:** "Targets and mode blobs … retire before the report returns" (`native-target-test.md:3`), "No resources escape this call" (`system.rs:413`), and the coordinator comment about retiring everything before return. Line 24 only lists "Renderer caches" as future qualification work, so the docs are vaguer than the code.
- **Fix:** right after `target::run` in `target_test::test` (`system/target_test.rs:362`), call `renderer.cleanup_texture_cache()`. At that point the dmabufs are dropped and their weak keys are gone. Treat an error as a cleanup failure that clears `ok`. Because this happens after `run`, the report has to be adjusted there.
- **Tests don't cover this:** the fake `Target` has no renderer at all.

**M2. A TEST_ONLY success can describe a different topology than the plan.**
- **Path:** `test_only` (`system/target_test.rs:286-339`) sets CRTC_ID only on the planned connectors and disables only non-primary planes on the tested CRTCs.
- **False success:** suppose connector X is still routed to a CRTC the plan gives to connector Y. X might be unassigned (disconnected, no valid mode, no route; see `output.rs:304-320`) or cloned there. The kernel then tests "CRTC drives X and Y", which may pass and be reported as `ok`.
- **False failure:** suppose a planned connector or primary plane currently sits on an untested CRTC. That CRTC is left active with no connector or primary plane, which typically fails the kernel's enable/connector consistency check.
- **Docs:** line 22 partly discloses this ("does not claim to solve every pre-existing connector/CRTC conflict"). It does not say that `ok` can be returned for a configuration that is not the plan.
- **Fix:** either add CRTC_ID=0 for currently routed connectors that aren't planned onto a tested CRTC, plus ACTIVE=0 for CRTCs that lose their last connector, or state that false-positive case explicitly in the docs and the probe's `output_tests_scope`.
- **Tests don't cover this:** none (the mock `test_only` ignores request contents).

## LOW

- **L1. Drop order on partial errors doesn't match the doc claim.** The `Target` field order is correct (`:103-107`). But inside `prepare`, locals drop in reverse: on Bind/Render/Sync/ModeBlob errors, `dmabuf` (declared at `:231`) drops before the `framebuffer` lease (`:226`), and `buffer` drops last. The kernel holds its own GEM reference for the framebuffer, so this is harmless. Doc line 24 ("guards … before target DMA-BUF/GBM storage retires") is only true for complete targets.
- **L2. Implicit (INVALID) candidates may always fail.** Smithay plane format sets always include INVALID (`upstream/smithay/src/backend/drm/mod.rs:399`). If GBM accepts `[INVALID]` on the modifiers path, `from_bo(..., false)` records the real modifier, which fails the exact check at `:211` and becomes a retryable AllocatedFormat failure. This produces false negatives and burns attempts, not unsafe behaviour. The Linear→implicit fallback is rejected correctly, and the `add_planar_framebuffer` assert (`upstream/drm/src/control/mod.rs:360`) can't fire because the format check runs first.
- **L3. Health is strict on multi-GPU systems.** A render-only or display-less GPU fails with `MissingPlan`/`NoTargets`, so `output_tests_healthy` is false (`probe.rs:65-69`). This looks intended; it's worth one sentence in the docs.
- **L4. A device-ID parse error skips the JSON report.** `gpu.device.parse()?` (`probe.rs:124`) returns early without printing a report. `Driver::drop` still stops the driver.

## Checked and found sound

- **Flags:** `flags()` is private, fixed to `TEST_ONLY|ALLOW_MODESET`, and is the only `atomic_commit` call. It has a unit test.
- **No surfaces:** there is no DrmSurface, DrmCompositor or OutputManager.
- **Legacy devices:** rejected before admission and before any allocation.
- **Coordinator (`target.rs:125-197`):**
  - Admission (counts, dimensions, pixel totals, uniqueness of IDs, format count ≤256) runs before any allocation.
  - The global 64-attempt limit is checked before each increment.
  - Retryable stages are Allocate, AllocatedFormat, Export, Bind and Render. Stale topology, properties, sync and mode-blob failures are fatal.
  - Cleanup is checked before every retry. The `targets` vector drops inside the closure before the final `cleanup_failures()` read.
  - Any failure clears `tested_targets`.
- **Revalidation:** connection state, exact raw `Mode` (`PartialEq` compares the full FFI struct), encoder, `possible_crtcs` against fresh resources, primary plane plus exact format, and non-desktop are all rechecked. The plan and the mode map come from the same discovery snapshot.
- **Framebuffer and blob:** AddFB2 gets the correct modifier flag. The mode blob is created only after a successful sync and is wrapped in a lease immediately. Both leases count destroy errors.
- **Render sync:** the fallback without ExportFence uses `glFinish` and returns signaled (`gles/mod.rs:2598`). The EGL fence wait has no timeout, which the docs disclose. Any EGL error counts as "interrupted", so retries are capped at 8.
- **Atomic property values:** rotation=1 (ROTATE_0), alpha=0xFFFF, SRC in 16.16 fixed point, CRTC rectangle in integers. Properties and the plane list are capped at 256 after the upstream query returns (disclosed). Property names that aren't UTF-8, or are duplicated, are rejected.
- **Probe:** the parser accepts exactly `--session --test-outputs` in either order. The final health check requires a nonempty set of successful records whose device/generation/epoch keys exactly match the final ready GPUs. `Driver` gates on `is_ready` and holds the kernel borrow for the whole test.
- **Test limits match the docs:** the fixtures cover only the coordinator, flags, parser and health. The listed negative mutations (dropping vector retention, ignoring cleanup, dropping the TEST_ONLY bit) look like they would be caught by the mock's live-count assert and the cleanup and flag tests. I did not run them. Nothing exercises the real `Adapter`, `Lease`, renderer cache, request contents or the Driver path.

## Correctly scoped as later work (not findings)

Live commit and presentation, page-flip and front/pending buffer ownership, rollback, mode fallback and backtracking after a failed joint test, the safe preview, display-preserving seat/DRM initialization, hotplug re-testing, an overall deadline, real memory caps, and hardware qualification are all explicitly documented as missing.

**Bottom line:** I found no HIGH issues. The main code issue is M1: the renderer cache keeps buffer memory pinned after `test()` returns, contrary to the "retire before return" claims. M2 is an oracle-accuracy gap that should be fixed or documented explicitly before anyone relies on `ok`.
