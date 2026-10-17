# Author disposition of focused shared-scene review

Date: 2026-10-17. Claude Opus 5.5 inspected frozen `e7059a7e4c0a0d4c55632364105964a3a3758394`. The [unaltered report](claude-report.md), [prompt](prompt.txt), [raw result](stdout.json), [execution](execution.json) and [verified read/search trace](verified-execution.json) preserve scope.

The review finished in 143.88 seconds, with 13 turns, 12 Read/Grep calls, no permission denials and `is_error=false`. Author recomputation of all 327 frozen file hashes found zero mismatches. Claude did not execute tests, hash inputs, inspect pinned Smithay source or verify hardware. This disposition is author verification and is not a second independent review.

## M1: pointer refresh lost while grabbed

Accepted. `refresh_pointer_focus` cleared its dirty flag before returning for an active grab. The pinned Smithay ClickGrab forwards motion to its captured focus and releases the last button with `restore_focus=false`; it does not restore the pending target automatically. The earlier report's restoration description was explicitly inference. The author checked the actual pinned implementation in `src/input/pointer/grab.rs:359–394` and `pointer/mod.rs:539–554,771–797`.

The fix retains the flag while grabbed and marks it dirty after every button release. The latter also repairs a drag crossing surfaces with no intervening scene publication. The next refresh uses the last successful composition and suppresses unchanged target/origin motion. Fix commit: `f637c21`.

The [private-display input harness](../../development/private-input.md) independently reproduced both paths using real XTest input through an authenticated owned Xvfb and Wayland pointer events. With the same fixture, the preserved pre-fix binary fails both scene-pan and held-motion cases with `M1_missing_focus_after_release`; the fixed binary passes both, including B enter without further motion and subsequent axis/motion/button delivery. The four-case scene suite alone does not establish these results. No input was injected into the user’s host display. The [evidence bundle](../../evidence/nested-milestone/2026-10-17-shared-scene/README.md) archives sanitized journals and exact executable hashes.

## M2: independent popup clip/paint/picking oracle missing

Accepted as a test gap. Popup lifetime and frame callbacks alone did not prove popup pixels or picking outside the root's container. The new [popup clip fixture](../../development/popup-clip-fixture.md) uses a root container at x=32, width=280 and a configured popup with actual output rect (0,0,80,60). It observes pointer focus leave the root, then a real popup pointer enter at (0,0) outside the owner clip, plus popup frame delivery.

The [scene smoke harness](../../development/scene-smoke.md) independently compares every readback row. Its controlled image has 4,800 magenta popup pixels: some lie outside the root clip at x=0..31, and the overlap at x=32..79 paints above the root. It verifies all remaining clipped root quadrants and background. The four-case run after the fix passed against unchanged executable hashes. These are author-executed real tests, not reviewer-executed tests.

The gap is only partially closed: higher-owner occlusion, nested popup paint order and a visible popup of an entirely culled root still need their own pixel/input scenarios. They remain open; source/API correspondence is not substituted for those tests.

## Lower observations

- **Zero-sized output:** accepted. Suspension and a degenerate draw now clear the published scene and request pointer eligibility refresh. Source fix in `f637c21`; a synthetic resize/minimize/restore matrix remains required.
- **Pixel count metadata:** clarified. The exact-row oracle implies the counts; summaries now call them `expected_rgb_counts` and label `pixel_validation: exact-row-equality` rather than implying a separate counting measurement.
- **Stale root bbox on desynchronized subsurface commit:** not established by current source. The commit handler processes buffer state, resolves every nonsynchronized subsurface up `get_parent` to the root and calls `commit_window(root)`, which invokes `Window::on_commit`. The pinned Smithay `window.rs:419–426` explicitly requires that hook for a toplevel or any unsynchronized subsurface and recomputes the root bbox. Synchronized pending changes become current on the parent's commit. This rebuts the specific reported path; it is not full subsurface/concurrency qualification.
- **Readiness of the pixel fixture:** a later concurrent-run failure exposed an undeclared fixture minimum: a 632 px host output produced a 316 px half-width container, correctly clipping the 320 px quadrant buffer. The failed image had 18,720 green/yellow pixels. The lifecycle client now declares a 320×240 minimum, and too-small outputs fail the prerequisite. Exact color assertions were retained. Sequential graphical suites pass; host tiling can make concurrent graphical runs interfere.

## Checked controls and remaining work

The reviewer source-checked actual bbox geometry, independent root/popup clips, copied input regions, post-success publication, immediate liveness/mapped/ancestry checks, popup-tree dismissal and builder budgets. Upstream ordering/offset and ClickGrab claims were independently checked by the author against the pinned source, not fetched or inspected by Claude.

These changes do not close a complete phase. Native/hardware, physical presentation, fractional scale/zoom, animation/configure transactions, full popup/grab/input cases, import failure injection, upstream resource/fairness and complete desktop integrations remain open. Later fixture/minimum/focus/harness changes are post-review author work and were not independently re-reviewed by Claude.

A [later independent follow-up](../scene-followup-2026-10-17/disposition.md) challenged the release-specific test oracle. Its corrected composition-stability and mutation evidence supersedes any inference that the earlier held-motion run isolated the release-time assignment. Both original observations and later qualification are preserved.
