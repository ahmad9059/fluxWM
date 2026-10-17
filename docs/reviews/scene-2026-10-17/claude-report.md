# Review: shared virtual scene and popup lifetime at `e7059a7e…`

## Scope and limits
- I read only the frozen tree, using Read/Grep, in 12 calls. I ran no commands, tests, builds or hashes, and wrote nothing. The author's hashcheck afterwards is the only thing that ties these findings to the frozen tree.
- Files read: `docs/development/runtime-scene.md`, `crates/fluxwm/src/scene.rs` (all of it), `winit.rs::RenderContext::draw`, `input.rs` (pointer refresh, motion, button and axis handling), `state.rs` (popup dismissal, `relayout`, `cleanup_popup_grab`, `finish_dispatch`, `surface_under`), the `effective_clip`, `intersection` and `round_rect_to_i32` functions in `flux-scene`, the commit handler, `new_popup` and grab gating, and parts of `tools/smoke_scene.py`.
- Smithay is not vendored here. Anything I say about its behaviour (popup offsets, traversal order, `ClickGrab` and pending focus) comes from memory and is marked as inference.
- Reading source cannot verify physical presentation, runtime results or GPU behaviour. This is not a statement that the code is production-ready or complete.

## New high/medium findings

### M1. A scene published while the pointer is grabbed loses its pointer-focus refresh
**Status:** the flag handling is confirmed in source; what Smithay does with it afterwards is inference.

**Evidence:**
- `input.rs:17-27`, `refresh_pointer_focus`: the function sets `pointer_focus_dirty = false` (line 21) *before* it checks `if pointer.is_grabbed() { return; }` (line 25).
- Only two places set the flag back to true:
  - publication in `winit.rs:265`
  - the popup-grab end in `state.rs:650`
- Nothing sets it again when an implicit button grab (`ClickGrab`) ends. The release path in `input.rs:196` skips the refresh motion while grabbed.
- `PointerAxis` (`input.rs:227`) relies on `refresh_pointer_focus`. By then the flag is already false, so the refresh does nothing.

**How it goes wrong:**
1. The user presses a button over window B. `focus_window_id` runs and `relayout` moves or reorders windows.
2. While the button is still held, the next draw publishes the new scene and sets the flag.
3. `finish_dispatch` calls `refresh_pointer_focus`. The pointer is grabbed, so the function returns, and the flag is already cleared.
4. On release, Smithay (inference) puts back the pending focus from the last `motion()` call, which was computed from the scene *before* the relayout.
5. Pointer focus and scroll events stay on a surface that may no longer be under the pointer until the pointer physically moves.

This contradicts the doc's claim that "Publishing marks pointer focus for refresh".

**Suggested fix:** only clear the flag once the refresh has actually run. Either keep it set while grabbed, or set it again in the release path when `!pointer.is_grabbed()` becomes true after `pointer.button`.

**Closure test (fixture):**
- Two windows. Pressing on one triggers a layout change (focus or camera recentre) that moves a different surface under the stationary pointer.
- Hold the button for at least one composition, then release without moving.
- Assert: after release, exactly one `leave` for the old surface and one `enter` for the new one.
- Assert: a following `wl_pointer.axis` goes to the new surface.
- Also cover the case where the grab ends with no new publication.

### M2. Nothing tests the separate popup clip or popup paint order
**Status:** the gap in the oracle is confirmed in source.

**What the code and doc claim:**
- `scene.rs:101-113` gives popup trees `clip: viewport` and pushes them ahead of the root.
- The doc says "a root's clip therefore does not cut its menu at the tile edge", and that popups render and pick even when the root's own buffer is culled.

**What the tests check:**
- `smoke_scene.py:62-68` (popup lifecycle) checks `popup_done` counts, keyboard non-stealing and "frames ≥ 1". Frame callbacks only show that `scene.windows()` included the owner. They do not show where the popup was painted or that it can be hit.
- Only `container-clip` takes a screenshot (line 119), and that scene has no popup.

So none of these is tested:
- the popup extending past the container edge
- the popup appearing in front of its root and behind windows stacked above its owner
- the popup being pickable outside the container clip
- the popup of a culled root

**Closure test:** a container-mode fixture with a popup positioned to cross the 280 px edge. Check:
- exact pixels on both sides of x=280
- that a stationary pointer outside the container but inside the popup gets `enter` on the popup surface
- that a window stacked in front of the owner covers the popup in both pixels and picking

Optionally add a nested popup to check order. Smithay's `popups_for_surface` iteration order decides whether a child menu is painted above its parent; that order is inference and is not checked in this tree.

## Lower-severity observations
- **Stale scene after a zero-size draw.** `winit.rs:191-193` returns `Ok` when the window size is zero or negative but leaves the previous `state.scene` published, so input keeps using the older viewport. This only matters on a degenerate host; consider clearing the published scene.
- **Hard-coded readback values.** In `smoke_scene.py:55-57`, `rgb_counts` and `right_edge_x280_matches_background` are hard-coded, not measured. They are implied by the exact row comparison, so this is not wrong, but the artifact reads like measured evidence.
- **Stale bounding box can cull a root.** Culling uses Smithay's cached `window.bbox()` (`scene.rs:70`), which is refreshed only through `commit_window`, i.e. on non-sync commits that resolve to the root. Capture walks the live tree. A desynced subsurface that moves into view while the cached box is stale could have its whole root culled until the next root-path commit. The extent depends on Smithay internals (inference).

## Checked against the actual claims
These match what the code does:

- **Root geometry** uses the committed bounding box: `origin = location − geometry.loc`, `visual = origin + bbox.loc` with `bbox.size`. No configure target is substituted (`scene.rs:69-84`).
- **Root clip** is container ∩ output. `clipped_root_input` intersects in f64, culls before the local snapshot limits, then rounds the edges together. The tree clip is `effective_clip()`, documented as container ∩ viewport and not limited to the visual rect (`lib.rs:159-162`).
- **Popup clip** is the viewport only. The popup origin `location + offset − popup.geometry().loc` matches Smithay's window popup placement as I remember it (inference).
- **Input regions** are copied: `region.clone()` of `current().input_region` at capture (`scene.rs:302-318`). The hit test checks the half-open clip, then the bounds, then the floored region, using only the copied data.
- **Publication happens after a successful render.** `state.scene = Some(scene)` is set only after `render_elements` and `render_output` succeed (`winit.rs:220-264`). Import errors and a missing element both propagate to `state.fail`, which fits the documented policy of failing the owned nested instance.
- **Order:** owners are reversed into front-to-back order, with popups before their root. The surface-tree order relies on `with_surface_tree_downward` being front-to-back (inference). The container fixture's child `enter` at a point overlapping the root is consistent with that.
- **Hit eligibility:** liveness, the window's `mapped` flag, popup membership in the current `popups_for_surface`, and a mapped ancestor chain up to the anchor (`still_mapped_in_tree`, bounded at 128).
- **Popup dismissal** happens on unmap (`commit_window`), on `remove_toplevel`, on dead-window cleanup in `finish_dispatch`, and on becoming invisible in `relayout`. `new_popup` and grab requests are refused for roots that are not visible.
- **Budgets** on depth, visit count, surface count and region rectangles are checked before descending. Overflow stops the traversal and returns `Err`.
- **Frame callbacks** go to owners whose captured surface overlaps its clip, popup nodes included.
- **Resize** drops the texture (`winit.rs:114`).

## Not verified
- Smithay's popup iteration order and offsets, its traversal order, how `ClickGrab` restores focus on release, and whether `Window::send_frame` covers popups.
- Runtime behaviour of the smoke and fixture modes, pixel output, and GPU/import failure handling.
- The other items you listed as known open work: native/hardware outputs, fractional scale and zoom, animation and configure transactions, the authorized/nested popup input matrix, and upstream resource and fairness qualification.
- Why `state.rs:295` sets `pointer_focus_dirty`. I did not read its context.
