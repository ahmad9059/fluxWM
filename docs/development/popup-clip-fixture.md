# Popup viewport clipping, paint order and real picking oracle

`fluxwm-fixture popup-clip` tests a non-grabbing popup that crosses its root's container boundary. It combines actual popup configuration/frame/pointer events with a delayed, independently checked compositor framebuffer. A popup callback alone cannot prove pixels survived the root clip or painted above the root.

## Run

```sh
cargo build -p fluxwm -p fluxwm-fixture
target/debug/fluxwm --backend nested --socket fluxpopupclip-UNIQUE \
  --exit-after-ms 2500 --stats-file /tmp/NEW-popupclip-stats.json \
  --screenshot /tmp/NEW-popupclip.ppm --screenshot-after-ms 700 \
  -- target/debug/fluxwm-fixture popup-clip --timeout-ms 4500
```

Use unique socket/output paths and an otherwise isolated nested instance. The fixture needs compositor, SHM, XDG shell, seat/pointer, runtime IPC, output dimensions at least 320×240 and the new seat's actual stationary pointer at `(0,0)`. No physical input or debug pointer-injection API is used. Unexpected initial coordinates or changed focus/geometry fail the case. The fixture's absolute deadline defaults to five seconds and accepts 1–5,000 milliseconds; it retains the final scene for 1,000 milliseconds. Supervise the compositor separately with its exit timer.

The delayed framebuffer must be validated independently with the scene smoke harness's popup oracle. The fixture's `ok` reports protocol/layout/picking assertions and emits exact image expectations; it does not inspect the P6 itself. Require both the same-run fixture success and the same-run pixel-check success. The recorded capture delay was 700 milliseconds; the popup was fully ready by 67 fixture-relative milliseconds, before the one-second stable interval.

## Geometry and actual event assertions

1. Map the fixed oversized 1900×240 SHM root: the first 320 columns retain four 160×120 quadrants, and every column beyond 320 is cyan. Keep explicit root XDG geometry `(0,0,1900,240)` and retain the buffer while acknowledging later size suggestions.
2. Require actual root pointer entry at `(0,0)` and callback progress. Switch to canvas through IPC and verify its mode. Query the initial width, resize the group to 280, move it right by 32, and require target geometry `(32,0,280,H)` through IPC.
3. Wait until the root no longer owns pointer focus. The pointer at world x=0 lies outside the moved container `[32,312)`. This establishes the root-only negative picking state before the popup exists.
4. Create a non-grabbing 80×60 popup. Its positioner uses a top-left anchor at `(0,0,1,1)`, bottom-right gravity, offset `(-64,0)`, and x/y sliding constraints. Record the **actual** popup configure; require parent-relative `(-32,0,80,60)`, which resolves with root x=32 to viewport `(0,0,80,60)`. A different configure fails; expected geometry is not inferred merely from the requested offset.
5. Acknowledge the popup's surface configure, attach its own opaque magenta buffer, and request a frame. Require a real popup frame callback and `wl_pointer.enter` targeting that exact popup surface at local `(0,0)`. This proves pointer targeting outside the root's container without an explicit grab.
6. Keep the root, popup and pointer selection unchanged for 1,000 milliseconds. Require the popup still owns pointer focus at `(0,0)`, its configure geometry is unchanged and no popup dismissal occurred. Then destroy the popup explicitly.

The popup must remain visible in x=0..31, which is outside the root's container, and paint above the root in their overlap x=32..79. It is clipped to the output viewport, rather than inheriting the root's tiling container crop. This test covers that concrete FluxWM scene policy for this geometry.

## Independent full-image oracle

The copied framebuffer uses the following exact RGB row regions. Every unspecified pixel must equal background `[15,18,23]` in the recorded baseline renderer. The image header determines the actual framebuffer width/height; require at least 320×240, rather than assuming the initially requested host window dimensions.

| Rows, inclusive | Required horizontal bands |
| --- | --- |
| 0..59 | magenta x=0..79; red x=80..191; green x=192..311; background x≥312 |
| 60..119 | background x=0..31; red x=32..191; green x=192..311; background x≥312 |
| 120..239 | background x=0..31; blue x=32..191; yellow x=192..311; background x≥312 |
| y≥240 | all background |

Required global color counts are magenta 4,800; red 16,320; green 14,400; blue 19,200; yellow 14,400; cyan zero. Red loses 48×60=2,880 pixels where the popup overlays it. All 80×60 popup pixels survive, including 32×60 pixels outside the root container. The right edge of root content remains x=311; retained actual root content beyond its container must not leak through.

Comparing complete rows proves both root/popup clipping and popup-above-own-root paint order. Counts alone could accept misplaced regions; callbacks alone could accept incorrect rendering. The separate harness tests its checker with positive image data and one-pixel clipping, wrong-order and overflow corruptions before using real output.

The fixture emits `expected_rgb_counts` and `expected_image_regions`, together with `actual_popup_configure`, `actual_popup_world_rect`, `root_after_move`, `after_settle`, `initial_root_pointer_xy`, `pointer_outside_root_before_popup`, `popup_pointer_enter_xy`, `popup_receives_pointer_outside_owner_clip`, `popup_frame_callbacks`, and `popup_done_before_cleanup`. Its event trace includes popup geometry, configure acknowledgement, buffer commit, callback and pointer enter. Callback versus enter ordering may differ within the same composed update; both must occur after the mapped popup commit.

## Actual observed result

On 2026-10-17 the mode and an independent full-row P6 comparison passed against the frozen root binary. Root exited 0. Evidence is retained at `/home/ahmad/Documents/Projects/fluxwm-layout/target/popup-clip/095f74a7b6aa454db4c34f179c91f683/`, including exact command/provenance metadata, fixture stdout, compositor stderr, readiness, final stats, delayed P6 and `pixel-proof.json`.

| Artifact | SHA-256 |
| --- | --- |
| Root executable | `a47f2df60790425a06f2f64176f27ebbffc41d6c4f02d3cda5a17eea6f9ad239` |
| Fixture executable | `7aa60332d31b013b37753e61a0d043288e540bf65e9a6f08e2e16aa9de2e72dd` |
| Delayed P6 | `b19d2bcb89f5c080affaea928d3c73ebc5549a584533131483cb7e32e69367a3` |

Actual configuration was `[-32,0,80,60]`; computed popup world geometry was `(0,0,80,60)`. The root had target x=32 and width 280. Actual popup frame and pointer entry `(0,0)` occurred at 67 milliseconds; the popup remained selected without dismissal through the stable interval. The fixture completed in 1,068 milliseconds.

The captured framebuffer measured 1270×1396. Every row matched the table exactly; measured counts were magenta 4,800, red 16,320, green 14,400, blue 19,200, yellow 14,400, cyan zero. The initial output query had reported 1276×1400 before the host completed a subsequent resize; the later root target height and P6 header reflected 1396. A harness must not treat an early output-size snapshot as immutable host geometry.

This is a positive real-client observation for the recorded binary, not a claimed old-binary failure comparison or release qualification. Final source revisions require their own fresh run and evidence.

## Bounds and open coverage

The sequence creates one root and one popup. SHM payloads are fixed at 1,824,000 root bytes plus 19,200 popup bytes. Popup buffer helper dimensions are validated within 1..512 per axis. IPC frames retain the 64-KiB diagnostic ceiling and monotonic deadlines; event evidence retains the existing 1,024-entry ceiling. Eight fixture unit tests and `cargo clippy -p fluxwm-fixture --all-targets -- -D warnings` pass after the shared map helpers were reused.

This closes one actual popup-vs-root clip/paint/pick scenario at unit scale. Higher-owner occlusion ordering, nested popup trees, fully offscreen owners with visible popup content, rotated/fractional outputs, animated snapshots, grabs, lock/overview layering, acquire/retirement behavior and native hardware remain separate open cases. Debug P6 readback proves virtual composition, not host presentation or capture-service security. Successful popup configuration and pointer entry cannot replace those missing cases.
