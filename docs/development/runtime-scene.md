# Shared composed scene in the nested runtime

`flux-scene` supplies [immutable validated window geometry](scene.md). The executable's `scene.rs::RuntimeScene` adds Wayland surface ownership, committed surface-tree origins, logical destination sizes and copied input regions for one virtual-output composition. This is current implementation work, not a completed phase 4 or a physical presentation guarantee.

## Capture and publication

The builder takes mapped owners in Space's established stacking order. Root visual bounds come from the committed surface-tree bounding box, positioned using the window geometry offset; target configure sizes are not substituted for the client's current buffer. Container clips come from layout placements transformed into output coordinates. Endpoints are rounded together before intersection with the output, so adjacent half-open clips share their rounded edge.

For each eligible owner, popup trees precede its root tree, matching the pinned Smithay window render ordering. Popup eligibility follows mapped scene ownership independently of whether the root buffer itself intersects the viewport. Root trees use container∩output clipping; popup trees use output clipping. A root's clip therefore does not cut its menu at the tile edge. Root unmap, scene removal and role retirement separately dismiss popup trees; a popup resource remaining alive after dismissal is not proof that it should receive input.

The builder traverses each mapped tree in the same front-to-back order used by Smithay picking/render elements. It copies each surface's committed origin, destination bounds and ordered add/subtract input-region rectangles. Picking later uses these copies rather than rereading live surface attributes or target layout rectangles. Half-open surface/clip bounds are checked before floor-converting the surface-local point for the input-region test. Resource liveness, current mapped ancestry reaching the saved tree anchor, and authoritative mapped ownership remain immediate input eligibility checks. Popup anchors must still belong to the current popup tree; dismissal invalidates picking even if the client keeps its wl_surface alive. Cached geometry/input regions are not replaced by target state during these eligibility checks.

Rendering creates an element for each captured surface and applies its corresponding crop. Import errors propagate, and a captured mapped surface that produces no render element fails this owned nested instance. The scene is published only after successful offscreen rendering; failed import/composition cannot publish input for an omitted surface. The current failure policy needs per-client/fallback qualification before production. It is not a guarantee that every driver failure can be recovered.

`surface_under` and click-to-focus use the published scene. A queued layout action does not immediately move the pick target ahead of the composed pixels. Publishing marks pointer focus for refresh; unchanged surface/origin does not emit a synthetic motion. The published scene represents the compositor-owned virtual output, not the host's last physical scanout or a presentation feedback timestamp. Frame callbacks are sent to owners with captured surface bounds intersecting their clip.

## Bounds and limits

The builder admits at most 16,384 trees/visited surfaces, 128 levels per captured tree and 65,536 copied input-region rectangles across the scene. It checks depth/visit count before descending and stops traversal on budget failure. The separate geometry snapshot caps window inputs at 1,024 and checks finite coordinates/endpoints. A budget failure currently terminates this owned nested instance with diagnostics. These checks bound this builder, not every upstream protocol/commit traversal or allocation; they are not hostile-client fairness/DoS qualification.

The current nested output uses scale 1 and no interactive zoom. Fractional scale, transformed native outputs, per-output scene ownership, animation/transaction snapshots, input constraints and presentation-timestamp coupling remain later work. All arithmetic and clipping assumptions must be revisited with their coordinate transforms rather than extending the scale 1 result by assertion.

## Development readback

`--screenshot NEW.ppm` retains the existing first-mapped composition export. `--screenshot-after-ms N` defers arming that export with one timer, allowing a bounded real fixture to change layout before readback. It requires `--screenshot`; N is 0..86,400,000 ms. The timer is registered during startup, before backend initialization, and requests one redraw when it fires. The export occurs on a subsequent eligible composition; this is not an exact physical-time screenshot API. Files must not already exist.

This option is an explicitly requested compositor-owned debugging readback. It is not a capture protocol, portal permission, lock/privacy boundary or application screenshot service. Screenshot bytes and fixture timestamps must be inspected to establish that the intended test state was reached before capture.

## Verification boundaries

The pure geometry crate tests overlapping/hidden/offscreen windows, clipping, order, integer endpoints and invalid coordinates. The normal nested smoke still checks original pixels/orientation and lifecycle. The container fixture checks actual oversized-buffer clipping and adds stationary subsurface input coverage. The [popup lifecycle fixture](popup-lifecycle-fixture.md) separately checks real parent unmap, invalid background grab rejection and parent-role retirement.

These checks do not close complete buffer-consumer, GPU, input, transaction, popup-chain, resource or native phase gates. In particular authorized grabs, parent/child destruction races, import-failure injection and native scale/transform cases remain required.
