# Oversized-buffer container render/input clip fixture

`fluxwm-fixture container-clip` retains a 1900×240 SHM buffer while shrinking its canvas group to 280 logical units. A delayed compositor screenshot proves pixels stop at the container edge; a later subsurface stage proves pointer picking stops at that same container boundary even where a real child surface still exists.

## Run both checks

```sh
cargo build -p fluxwm -p fluxwm-fixture
target/debug/fluxwm --backend nested --socket fluxclip-UNIQUE \
  --exit-after-ms 2500 --stats-file /tmp/NEW-clip-stats.json \
  --screenshot /tmp/NEW-clip.ppm --screenshot-after-ms 700 \
  -- target/debug/fluxwm-fixture container-clip --timeout-ms 4500

target/debug/fluxwm-fixture verify-container-clip /tmp/NEW-clip.ppm
```

Use a new socket and nonexistent output files. Capture the fixture stdout and the verification stdout separately. A passing `container-clip` JSON record proves the protocol/layout/input assertions; **it does not by itself prove pixel clipping**. The independent `verify-container-clip` result must also have `ok=true` against the delayed screenshot from that same run.

The live mode requires an isolated nested instance, runtime `FLUXWM_SOCKET` IPC, compositor/SHM/XDG/pointer/subcompositor globals, an output at least 320×240, and an initial stationary pointer at `(0,0)`. It injects no physical input or synthetic debug-input commands. Run without user interaction. Missing pointer entry or an unexpected pointer position fails the case rather than becoming an input-coverage pass.

`--screenshot-after-ms` is a compositor-owned debug capture option. The timer starts during compositor initialization; select a capture delay after the fixture's resize completes and before its post-capture input stage. The recorded run used 700 milliseconds; resize completed at 37 fixture-relative milliseconds and the fixture retained the group/buffer for a 1,000 millisecond settling interval. The actual pixel bounds independently identify the captured layout. A screenshot captured too early or after client disconnect will fail the expected pixel counts.

## Buffer and layout procedure

The original first 320 columns contain the same four 160×120 RGB quadrants as the lifecycle fixture. Every pixel from x=320 through x=1899 is cyan `(0,255,255)`. The client sets explicit XDG window geometry `(0,0,1900,240)`, acknowledges compositor configures, and deliberately retains that actual buffer/geometry while testing smaller ordinary canvas target sizes.

The fixture maps the root and waits for callback progress, invokes the supported IPC `workspace-mode` action, and confirms `workspaces.list` reports canvas. It reads the current target width from `windows.list` and invokes `resize` with `delta=280-current_width`. It requires target origin `(0,0)` and width exactly 280, then processes events for 1,000 milliseconds without replacing the oversized buffer. A second window query must still report width 280. An output resize or transform that invalidates these prerequisites causes failure rather than silently changing expected coordinates.

The stable screenshot must therefore contain:

| Color | Required bounds, inclusive | Required count |
| --- | --- | --- |
| Red | x=0..159, y=0..119 | 19,200 |
| Green | x=160..279, y=0..119 | 14,400 |
| Blue | x=0..159, y=120..239 | 19,200 |
| Yellow | x=160..279, y=120..239 | 14,400 |
| Cyan overflow | absent | 0 |

Column x=280 across y=0..239 must match the clear background. The client keeps real content through x=1899, so the expected edge cannot be explained by a buffer ending at x=280. The green/yellow quadrants originally continue to x=319; their exact smaller counts establish clipping inside those quadrants rather than merely hiding the cyan overflow.

The offline verifier reads only a regular P6 file, limits reads to 64 MiB, requires the compositor's exact P6/255 header form and exact payload length, and bounds dimensions to 320×240 through 8192×8192. It verifies all counts, all color bounding rectangles and the clear-background edge. It reports measured dimensions, counts, bounds and edge evidence as JSON. The unit regression accepts the expected crop and rejects a single cyan pixel leaked at x=280. This is checker validation, not replacement runtime evidence.

## Input clip parity after screenshot

After the stable screenshot interval, the fixture creates an 80×80 synchronized subsurface above the root at parent-local `(-64,0)`. It explicitly repeats the root's XDG geometry `(0,0,1900,240)` so the negative child does not silently change the root's geometry origin. It commits the child and parent, then requires a child frame callback and an actual `wl_pointer.enter` on that child at local `(64,0)`. This confirms the stationary world pointer is at `(0,0)` while the actual child spans `[-64,16)` horizontally.

The supported canvas `move right` action shifts the group to x=32 while retaining width 280. The fixture verifies that target through IPC. The actual child now spans `[-32,48)`, so the pointer at x=0 remains inside the child's real buffer but lies outside the container `[32,312)`. The fixture requires a real pointer leave and no remaining surface focus within 400 milliseconds. It rejects changed child-local motion as contamination/inconsistent routing. No physical movement or newly invented input API is used.

A successful JSON record includes `child_enter_xy:[64,0]`, `after_input_move.windows[0].geometry.x:32`, and `input_clip_proof` with `ok:true`, the before/after child bounds, `pointer_still_inside_actual_child:true`, `pointer_outside_container:true`, `pointer_focus_after_move:null`, and `child_motion_events:0`. Event evidence records child callback → child enter → child leave. The subsurface and its buffer are then explicitly destroyed.

The input stage runs after the delayed screenshot, so its later magenta child and shifted root cannot contaminate the quadrant pixel proof. These two stages exercise the same container contract through independent render and input observations.

## Actual successful run

On 2026-10-17 the combined fixture and the standalone P6 verifier both passed. Root exited 0. Retained local artifacts are at `/home/ahmad/Documents/Projects/fluxwm-layout/target/container-clip/a9583d6282fb47eea10d911f7d30efe8/`: exact command/binary metadata, fixture stdout, compositor stderr, readiness, stats, delayed P6, `pixel-proof.json`, and image hash.

| Provenance | SHA-256 |
| --- | --- |
| Root executable | `66758af47ee2b11d3c507707fdbc86fc14c339f898cf185a8ad26a01d006cd89` |
| Fixture executable | `37dc27d8d99c68c2d669294e36bbc856b0433e3c96ef0e32bfcf7ee178798e52` |
| Delayed P6 framebuffer copy | `4e9df4c1d5908d39a8e501b6780defc43ce77772b26a0c815877a305d3a4f371` |

The host-assigned nested framebuffer was 1270×1396. IPC confirmed canvas mode and target width 635→280, with resize completed at 37 fixture-relative milliseconds. The retained buffer remained 1900×240. Exact P6 counts and bounds matched the table; cyan count was zero, and x=280 matched background RGB `[15,18,23]` for all 240 content rows.

After the one-second stable interval, the child entered at `(64,0)`, the target shifted to x=32, and the child emitted a pointer leave despite its actual surface still covering x=0. No child motion event occurred. The combined mode completed in 1,056 milliseconds. Separate earlier pixel-only and combined exploratory runs remain under sibling `target/container-clip/` directories; they are not substituted for this final-mode result.

No live before-fix compositor comparison is claimed: the earlier binary lacked the delayed-capture control needed for a reliable post-resize screenshot. The checker has a one-pixel negative test, and model/crop unit tests remain distinct from this positive real-client/render/input result.

## Scope and checks

The live sequence is bounded by a five-second maximum fixture deadline, one root, one child, a fixed 1900×240 root SHM allocation (1,824,000 bytes), an 80×80 child allocation (25,600 bytes), bounded IPC replies and the existing 1,024-event evidence ceiling. Normal completion took about one second. The offline verifier bounds bytes and dimensions; its caller should also supervise command time when using untrusted storage.

`cargo test -p fluxwm-fixture` passes eight tests, including preserving the original quadrants inside the oversized pattern and rejecting a one-pixel clipping leak. `cargo clippy -p fluxwm-fixture --all-targets -- -D warnings` passes.

This is evidence for the tested nested, unit-scale virtual scene. The debug readback is the compositor's offscreen composition, not host presentation or a public screenshot/capture service. It does not qualify fractional/output transforms, zoom, arbitrary negative geometry, popup clipping policy, lock/overview ownership, acquire fences, native scanout or physical hardware. The retained binary hashes identify a historical development snapshot; final integrated revisions need new runs and provenance.
