# Immutable logical scene geometry

`flux-scene` builds one bounded immutable logical geometry snapshot for drawing and candidate picking. It owns copied `WindowId` and rectangle values, with no Wayland surfaces, camera state, renderer resources or device access. Root can share a completed snapshot with `Arc<SceneSnapshot>` and replace it atomically; the crate has no interior mutation API.

## Construction and coordinate ownership

```rust,ignore
let mut input = flux_scene::SceneInput::new(window_id, actual_visual_rect, container_clip);
input.z_index = if focused { 1 } else { 0 }; // Root selects its stacking policy
input.focused = focused;
input.visible = mapped_and_visible;
let snapshot = flux_scene::SceneSnapshot::new(viewport_rect, [input])?;
```

Every supplied rectangle must already use the **same logical viewport coordinates**. The crate does not apply a camera offset, zoom, output translation, surface geometry offset or device scale. Root must transform the layout's world-space container clip and actual committed client geometry into that coordinate system before construction. Passing a world-space clip beside a viewport-space visual rectangle is a caller error that numeric validation cannot detect.

`visual_rect` describes the actual committed window geometry at its mapped position, including any required client geometry offset conversion. `container_clip` describes the allocation's cropping boundary. An oversized minimum-constrained client keeps its actual visual size; clipping does not rewrite it to the layout target size. `effective_clip` is the container intersected with the viewport. `visible_rect` additionally intersects that clip with the actual visual rectangle.

Scene construction counts and validates every input, including hidden or offscreen entries, before filtering. The maximum is 1024 inputs; the constructor consumes at most one additional input to detect an overflow. Every full generational window ID must be unique across those inputs. Hidden entries, disjoint visual/container pairs, offscreen allocations and zero-area intersections are omitted from the finished `entries()` slice. A valid empty snapshot still has a positive viewport; an outputless or zero-size backend must handle that state separately.

Rectangles require finite x/y/width/height and finite right/bottom endpoints. Width/height must be strictly positive, and adding them must produce representably greater endpoints; a tiny width lost at an enormous origin is invalid. Scene origins and endpoints must lie within ±1,000,000,000 logical units. Consequently the worst axis span is 2,000,000,000, within signed 32-bit renderer extents. These are explicit implementation limits, not Wayland protocol limits or a claim that infinite world coordinates fit directly into a frame snapshot. Camera rebasing/filtering must occur before reaching this view-space boundary.

Failures return `SceneError::InvalidViewport`, `InvalidEntry { window, field, source }`, `DuplicateWindow`, or `CapacityExceeded`; no partially built snapshot escapes. `GeometryError` distinguishes invalid extents, the scene coordinate limit and checked integer overflow. Root must decide how an invalid contribution affects a frame/session and report it without treating untrusted malformed client geometry as permission to panic. This crate does not authorize IDs or prove the corresponding objects remain live.

## Draw order, hit candidates and clips

`entries()` returns back-to-front ascending `z_index`; equal z values preserve input order. `focused` is metadata and does not implicitly override that explicit stacking order. Root can place a focused tiled window above peers by selecting its z value, and can use another policy for floating/overview layers.

`SceneEntry` exposes copied getters: `window()`, `visual_rect()`, `container_clip()`, `effective_clip()`, `visible_rect()`, `z_index()` and `focused()`. A snapshot's `viewport()` is also copied. Mutating original construction inputs or a returned rectangle cannot change a completed snapshot.

`hit_candidates(point)` iterates front to back and requires the point to lie within the visual rectangle, container and viewport. `hit_test(point)` returns the first such rectangular candidate. `clip_candidates(point)` instead requires only the effective container/viewport clip, allowing Root to evaluate actual subsurface/input-region geometry outside the toplevel visual rectangle. Nonfinite pointer coordinates produce no candidates.

Candidate selection is geometric broad-phase only. Root must resolve the live generational ID and test the actual committed surface-tree input region, owner/grab/lock policy and coordinates. If the top candidate's surface input region has a hole, continue down the iterator rather than treating its rectangle as an opaque input region. Hidden/offscreen owners were omitted, so this snapshot is not an inventory of every toplevel or an authority for independent popup layers. Root's popup policy can use an independent viewport-only clip; `clip_candidates` does not mandate clipping popups to their parent's tile.

All containment uses half-open intervals: left/top included, right/bottom excluded. Two adjacent visual rectangles therefore have one owner at their shared edge. Edge-touching rectangles have an empty intersection, returned as `None` by the standalone `intersection(first, second)` helper. That helper validates finite representable positive rectangles but does not impose the narrower scene coordinate limit.

## Checked integer rounding

`round_rect_to_i32(rect)` returns `Result<Option<IntegerRect>, GeometryError>`, where `IntegerRect` carries signed `x`, `y`, `width`, `height`. It rounds each of the four edges to nearest integer, with halfway values away from zero, then computes extents from those rounded edges. Equal floating shared edges use equal rounded edges. Rounding position and width independently would not provide that property.

The helper validates the input first, checks each rounded edge before casting, computes differences in `i64`, and rejects extents that exceed `i32`. It never uses float-to-integer saturation to hide overflow. A positive subpixel rectangle whose rounded width or height is zero returns `None`; zero/negative input dimensions are errors. No output scale is applied. Root must choose when physical scaling occurs and keep render/pick coordinate conversions coherent; this helper is not a complete fractional-scale rasterization policy.

## Verification and limits

```sh
cargo test -p flux-scene
cargo clippy -p flux-scene --all-targets -- -D warnings
```

The tests exercise overlapping draw/pick order and stable z ties, focus metadata, actual oversized client geometry with a smaller container clip, offscreen/hidden exclusion, disjoint clip rejection, allocation versus visual candidates, negative coordinates, adjacent half-open edges, nonfinite pointer rejection, NaN/infinity/nonpositive dimensions, arithmetic overflow/lost extent, coordinate endpoints and maximum span, generation-safe duplicate identities, bounded infinite input iteration, snapshot ownership, checked shared-edge rounding, subpixel collapse and integer overflow.

This evidence covers pure geometry only. Root still needs external pixels/input tests for the Smithay renderer's actual crop behavior, committed surface geometry offsets, popup/subsurface transforms and input holes, output scale changes, and the relationship between submitted/presented scene snapshots. Geometry snapshots do not retain GPU or client buffer leases or prove secure lock/capture behavior.
