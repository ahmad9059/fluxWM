I found no reachable HIGH or MEDIUM defects in this snapshot. The previous low finding is fixed: an unknown or empty reference set now returns `InvalidTopology`, not `Capacity`. The format cap is still an open, unmeasured risk and has not been resolved. This review was static only (Read/Grep in the snapshot); I ran no tests and nothing on hardware.

## Discovery failure isolation (`system.rs:704-803`, `discovery.rs:272-294`)

- **No failure escapes through `?`.** `discovery::prepare` returns a `PreparedOutputs` value, not a `Result`, and `create_gpu` uses it without `?` (`system.rs:739-751`). Any error from `acquire` or the final `plan` becomes `plan: None`, `failure: Some(..)` and an empty `modes` map (`discovery.rs:282-286`). The renderer, GBM and DRM objects are kept, and `drm.disarm()` hands DRM over to the GPU struct (`system.rs:794-802`).
- **Errors stay visible.** `prepared_events` sends `GpuReady` (plan `None`, failure set) followed by a separate `Failure{device}` (`system.rs:438-447`). `dispatch_events` always lets `Failure` through, even after admission closes. The probe counts failure events, so `initialization_healthy` fails on any of them (`probe.rs:96-113`).
- **Steps after discovery.** If `SourceRegistration` fails after discovery, `?` drops the whole GPU, including its plan and modes. This is correct: no `GpuReady` is sent, `Failure` is sent, and `PauseOnDrop` pauses DRM. If `model.complete` fails, the GPU is retired and only a `Lifecycle` failure is reported. The output failure is lost in that case, but no plan is published. Low.
- **No stale plan published.** On Changed or resume, the old GPU is retired and `GpuRemoved` is sent before a new prepare (`system.rs:581-588`, `672-678`). One narrow exception (Low): if `model.changed` fails for a reason other than the session closing (`UnknownDevice`, or the generation counter running out), the old GPU and its last plan stay registered. Neither looks reachable in practice.
- **`Some(empty)` vs `None`.** A topology with no outputs plans to `Plan::default()` through `finish`, so it is `Some(empty)`, distinct from `None`.

## Planner classification (`output.rs:141-159`)

`references` now calls `unique_ids(.., usize::MAX)`, so the capacity check can never fire there. Any reference outside the known set is `InvalidTopology`, including when the known set is empty. An empty reference list is accepted and simply gives `NoCompatibleRoute`. The `Capacity` error is reserved for the length checks at `output.rs:252-266`. **Previous issue addressed.**

## Primary-source conclusions (`upstream/smithay/src/backend/drm/mod.rs`)

- **Primary classification.** `planes()` lists all plane handles, keeps those whose `possible_crtcs` (filtered against a fresh `resource_handles()`) include the CRTC, and sorts them by the plane's `type` property. The type values are the kernel constants `DRM_PLANE_TYPE_*` (`upstream/drm/src/control/mod.rs:1425-1432`). Primary planes are returned whether or not the universal-planes capability is set; only cursor and overlay lists depend on it. `DrmDevice::new` asks for that capability (`device/mod.rs:194`). If the request failed, an old kernel would return no primary planes, every CRTC would lack a plane, and the result would be a valid plan with every output marked `NoCompatibleRoute`, not an error. Low.
- **Panic paths.** `plane_type` calls `unreachable!()` if a plane has no `type` property (`mod.rs:244`). That depends on the kernel always attaching that property, which I could not check from this snapshot. A panic would unwind through `create_gpu` (`PauseOnDrop` would pause DRM), not become a `Failure` event. The `.unwrap()` at `mod.rs:337` is safe.
- **Shared primary planes.** A primary plane that can serve several CRTCs is returned once per CRTC. `acquire` merges these into one `Plane` with all its CRTCs and requires the formats to match. The flow graph's split nodes then give each plane to at most one route. This matches the planner's assumptions; there is a fixture test for it.
- **Format semantics.**
  - **Plane formats** contain every legacy fourcc paired with the `Invalid` modifier, *plus* every `(code, modifier)` pair from `IN_FORMATS` when AddFB2 modifiers are supported (`mod.rs:288-384`). If the set is empty, `Argb8888/Invalid` is inserted.
  - **Renderer formats** (`egl/display.rs:924-1001`) likewise contain every renderable modifier pair plus an `Invalid` entry per fourcc. That covers all EGL fourccs, YUV included, not only scanout candidates.
  - **Matching.** The planner's exact pair intersection is therefore consistent: `Invalid` matches `Invalid`, so implicit-modifier candidates can be chosen. This is not a contradiction, but `candidate_formats` can include `Invalid` entries, and any future allocation or commit has to treat those as implicit.
- **Cap risk (unchanged, not quantified).** Both sets are inflated by those per-code `Invalid` entries. The renderer set is capped at `MAX_FORMATS` *before* it is intersected with any plane (`discovery.rs:70`, `system.rs:745`). So a renderer set over the cap makes the plan unavailable for the whole device, even if the scanout-relevant intersection is small. I make no claim about real counts. The fix to consider is filtering or intersecting with plane formats before applying the cap, or capping per plane.
- **Lost errno (Low).** An error from Smithay's `planes()` becomes `OutputDiscovery` with no errno (`discovery.rs:249`), because the `DrmError` source is discarded. The failure is still visible, just less specific.

## Test limitations

- Every discovery test uses the `Fixture` query. The real `DrmQuery` code (connector properties, `filter_crtcs`, Smithay `planes()`, `IN_FORMATS` parsing) is never run, and there is no DRM ioctl test adapter.
- The 257-pair test uses 257 copies of *one* format. It only checks the raw length before dedup, which matches the real code path but not real distinct-pair data.
- The event-contract test (`system.rs:1061-1080`) calls `prepared_events` directly, not through `create_gpu`.
- The panic paths, the missing universal-planes case, and modifier-capable versus legacy drivers are not covered.
- There are no surfaces, buffers, KMS commits or hardware runs. Main fluxwm is still nested-only. Nothing here amounts to hardware qualification or certification.

I wrote no plan file and made no changes, since you asked for no writes.
