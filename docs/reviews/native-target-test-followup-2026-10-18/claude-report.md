I found no reachable HIGH or MEDIUM issues in c2e86ed. Both earlier medium findings look fixed by the code as written, within the limits below. I only used Read/Grep inside the snapshot, about 14 calls, and ran nothing. This is not certification: `--session --test-outputs` has still never been run, and no real GLES, GBM, EGL or kernel atomic path has been exercised.

## M1: GLES cached imports stayed pinned after the test (fixed)
- **Order:** in `crates/flux-native/src/target.rs:142-191`, `targets` and any partially prepared target belong to the inner closure. They drop on every return path, including early `?` exits, before `finish_cleanup` runs at `target.rs:193-197`. `attempts == 0` means `prepare` was never called, so skipping cleanup in that case leaves nothing behind.
- **What the cleanup actually frees:** `Bind<Dmabuf>` (`upstream/smithay/src/backend/renderer/gles/mod.rs:1565`, `:1628-1636`) adds entries to both `dmabuf_cache` (through `import_dmabuf`) and `buffers`, and both hold only `WeakDmabuf`. `cleanup()` at `gles/mod.rs:841-846` keeps only entries whose dmabuf is still alive, then empties the destruction queue in the same call. The adapter holds the only strong `Dmabuf` (`Target._dmabuf`), and nothing else seems to clone it. So the call after retirement in `system/target_test.rs:130-134` really does release the EGLImage, renderbuffer, framebuffer and texture.
- **Error handling:** an earlier failure is kept, a cleanup error is added to the failure count, `ok` becomes false, and `tested_targets` is cleared (`target.rs:198-211`). This is correct.

## M2: existing routes changed what a successful test meant (fixed for the stated scope)
I traced `target/routes.rs:33-106` through these cases, and each comes out consistent:
- Unplanned connectors still attached to a tested CRTC are detached (`:75-79`).
- A CRTC an output moved away from is disabled only if no connector is left on it after the plan (`:81-92`).
- Taking a primary plane from a CRTC that stays in use is rejected as `TopologyConflict`.
- Non-selected planes on tested or disabled CRTCs are cleared (`:93-100`).
- Swapping CRTCs or planes between two assignments works, because source CRTCs that are also tested are left out of the disable step.
- A selected primary plane taken from a CRTC that is being disabled moves correctly.
- A CRTC that is enabled but has `ACTIVE=0` is fully disabled.

In the adapter (`system/target_test.rs:278-387`):
- It reads each connector's `CRTC_ID` and the current CRTC of each plane fresh. With atomic drivers, `get_plane` reports the plane's state CRTC, so the two sources agree.
- It writes `CRTC_ID` for every known connector. Writing an unchanged value doesn't pull that connector's CRTC into the commit, because the kernel returns early when the value is the same.
- It sets `ACTIVE=0` and `MODE_ID=0` on disabled CRTCs and `FB_ID=0` and `CRTC_ID=0` on cleared planes.
- It checks `non-desktop` again on the fresh property map.
- Flags are fixed to `TEST_ONLY|ALLOW_MODESET`, and there is no live-commit path.
- The index lookups `connector_properties[...]` can't panic, because `reconcile` already confirmed every key exists.

## Remaining lows
1. **Inherited properties.** Properties the request doesn't set keep their current values. That covers connector `max bpc`, `Colorspace`, `HDR_OUTPUT_METADATA` and `Broadcast RGB`; CRTC `GAMMA_LUT`, `DEGAMMA_LUT`, `CTM` and `VRR_ENABLED`; and plane `zpos`, `pixel blend mode`, `COLOR_ENCODING`, `COLOR_RANGE` and `SCALING_FILTER`. A pass or fail therefore depends on whatever the previous DRM master left behind, for example an inherited HDR or high-bpc bandwidth setting causing a fail. Only `rotation` and `alpha` are set, and only where the plane has them.
2. **Side effects not in the report.** A successful report doesn't say which connectors would be detached, which CRTCs disabled, or which planes cleared. `tested_targets` alone can make it look like only the planned outputs changed. Unplanned outputs on CRTCs that aren't tested stay lit in the hypothetical.
3. **Conservative rejections.** A connector that is disconnected but still routed, or a non-desktop connector on a source CRTC, counts as "remaining". Taking a primary plane from that CRTC is then rejected as `TopologyConflict` even though it might work (a false negative). There is also no search for alternative routes, which matches what the commit claims.
4. **Large but bounded request-time work.** The worst case is roughly 64 connectors × (1 + 256) property calls, plus 256 `get_plane` calls, plus up to (32 + 256) × 257 more for disabled CRTCs and cleared planes. That is on the order of 80k ioctls, with no deadline.
5. **Encoder not enforced.** The encoder is checked in `prepare`, but the atomic request can't force it; the driver's `best_encoder` may pick a different one.
6. **Already documented:** freshness only holds at request time, with no hardware reservation. Other documented items are the drop order of locals in `prepare` on Sync/ModeBlob failure, false negatives from implicit allocation, strictness on multi-GPU systems, and the pixel budget being nominal rather than a real memory bound.

## Test gaps
- The reconciler has 4 unit tests. None cover swapping CRTCs or primary planes between assignments, an `ACTIVE=0`-but-enabled source CRTC, several sources being disabled at once, or a stolen plane whose source CRTC is also tested.
- Nothing tests how the adapter builds the request: which property names and values go out, what happens when `CRTC_ID` or `non-desktop` is missing, or the `Properties::read` limits.
- Cache lifetime and error handling are only tested with mocks (`target.rs` `Shared.cached`). Nothing tests that GLES `cleanup()` actually empties `buffers` and `dmabuf_cache` after a real bind.
- The flags test (`target_test.rs:412`) only checks a constant.
- Real property, framebuffer, blob, GBM, EGL, fence and atomic `TEST_ONLY` paths, and the manual probe, have never been run. Output presentation, page-flips, live commits and hardware qualification are all still out of scope.
