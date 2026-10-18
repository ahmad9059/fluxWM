I found no HIGH defect in the planner. There is one MEDIUM finding that depends on hardware: the 256-format limit may be too small for real devices, and hitting it now stops the whole GPU from coming up. I'm not certifying anything: I ran nothing, and one key Smithay function isn't in the snapshot, so I couldn't check it.

Scope: I read `output.rs`, `system/discovery.rs`, `system.rs:450-788`, the doc, and the pinned drm-rs (`control/mod.rs`, `property.rs`) and Smithay `device/mod.rs`. Everything else here comes from reading the code.

## Concrete findings

**M1 — The 256 limit counts format+modifier pairs, and going over it makes the whole GPU fail (MEDIUM; depends on hardware, not confirmed).**
- `system.rs:721-735` now requires discovery to succeed before `GpuReady`. Any discovery error goes through `?` and the GPU is never registered.
- Both limits count format+modifier pairs, not format codes:
  - renderer formats: `system.rs:727` (`take(MAX_FORMATS + 1)`), then `discovery.rs:70`;
  - plane formats: `discovery.rs:256`, then `:105`.
- **Trigger:** a driver that offers many modifiers. amdgpu with DCC variants, plus the EGL render set on radeonsi or iris, plausibly gives more than 256 pairs per plane or for the renderer. Smithay probably also adds an implicit (INVALID) entry per format, which raises the count.
- **Result:** a GPU that came up before this stage now fails at `OutputTopology` before any output work exists. The doc (`native-outputs.md:25`) states the limit, but it doesn't say the unit is pairs or that real hardware may hit it.
- **To confirm:** run `drm_info` / `eglinfo` on the target GPUs. Consider raising the limit, counting it per format code, or not failing GPU readiness when discovery fails.

**L1 — Errors from the adapter aren't covered by any test, and all of them fail the GPU.**
- `discovery.rs:177-185` makes one `get_property` call per connector property, up to 256 per connector and 64 connectors.
- A failure on an unrelated property, a `non-desktop` property that isn't the boolean (0,1) range type (drm-rs `control/mod.rs:477`), or any `planes()` error all abort GPU preparation.
- This matches what the doc says the code does. The point is how much breaks: discovery is "optional" in name, but in practice GpuReady can't happen without it.

**L2 — `references()` returns the wrong error kind (`output.rs:153-154`).**
- It calls `unique_ids(ids, known.len())`. If a connector lists any encoder while the encoder set is empty, the result is `Capacity` instead of `InvalidTopology`.
- Discovery maps every error to `invalid()` anyway, so this only matters to callers of the planner itself.

**Observation (not a defect) — force probe can cause the GPU to be rebuilt once.**
- `get_connector(handle, true)` (`discovery.rs:171`) runs on every udev `Changed` event.
- If the forced probe changes a connector's status, the kernel's probe helper can queue another hotplug event. That leads to another `Changed`, and `udev()` (`system.rs:657-662`) tears down and rebuilds the whole GPU again.
- It settles once the status stops changing. drm-rs warns that force probes block and can flicker (`control/mod.rs:180-186`).

## Checked and found consistent

**Flow graph (`output.rs:187-246, 287-353`)**
- Every connector, encoder, CRTC and plane is split into an in-node and an out-node joined by a single edge, so each can carry at most one route.
- Augmenting a path flips the edge flags correctly, including when it cancels earlier flow through a reverse edge.
- Searching for the shortest path each time with integer flow gives a maximum matching for the supplied graph.
- `used_to` only looks at forward edges whose flow is used. The split nodes guarantee each out-node has at most one such edge, so reading routes back gives exactly one route per assigned connector.
- Planes with no common format get no sink edge, so `formats[&plane]` (`:390`) can't panic. Duplicate references are rejected, so there are no parallel edges.

**Refresh calculation (`:78-98`)**
- It uses the same formula as the kernel's `drm_mode_vrefresh`: interlace doubles the numerator; doublescan and vscan > 1 multiply the denominator.
- The sync-ordering checks match `drm_mode_validate_basic`.
- No u64 overflow is possible. Values too large for u32 are rejected. The u32 area can't overflow (65535² < u32::MAX).
- `Reverse(index)` under `max_by_key` picks the earliest index.

**Encoder CRTC masks**
- `filter_crtcs` uses the first `ResourceHandles` (`discovery.rs:238`).
- The drm-rs `1 << n` on a `u32` (`control/mod.rs:1241`) is only safe because `MAX_CRTCS = 32` is checked before any encoder query (`:69`).
- The "after" check compares sets, not order. That's acceptable because the CRTC list is fixed for the device's lifetime.

**`non-desktop`:** pinned drm-rs reports a (0,1) range as `Boolean` (`control/mod.rs:477`), so `as_boolean` is the right accessor.

**Drop guard**
- On an early error, the renderer, EGL and GBM are dropped before the `PauseOnDrop` guard pauses DRM.
- `disarm()` runs in a struct literal that can no longer fail (`system.rs:781`), after `insert_source` has succeeded.
- I saw no new ownership problems, and the ticket-epoch check (`is_ready(ticket)`, `:759`) is unchanged.

**Already documented, not defects:** IDs being the same doesn't make the metadata one consistent snapshot. The kernel can reuse freed IDs, and the doc says so (`:11`). Other documented gaps: mode flags beyond interlace/doublescan aren't filtered, there's no bandwidth or atomic feasibility check, and no KMS test or commit, no buffers or surfaces, and `--session` hasn't been run.

## What I couldn't verify

- **Smithay `planes()`:** the free function lives in `backend/drm/mod.rs`, which isn't in `upstream/smithay` (only `device/{mod,fd,legacy,atomic}.rs` are). So I couldn't check:
  - how it classifies primary planes, or whether it filters by `possible_crtcs`;
  - what it does without the universal-planes capability;
  - whether it reads IN_FORMATS and how it falls back (implicit INVALID / Linear entries);
  - how it handles unknown fourccs.

  `create_surface` re-checks `possible_crtcs` for primary planes (`device/mod.rs:344-347`), which hints that `planes()` might not filter by it. If it doesn't, discovery could add plane-to-CRTC edges that don't exist.
- **The real DRM adapter (`DrmQuery`)** isn't run by any test, as the doc admits: not the property loop, the mask ordering, the format conversion, or the error mapping.

## Test limitations

- **Oracle test (`output.rs:549`):**
  - It only compares the number of routes. Route validity is checked separately by `assert_valid`.
  - It covers only 2 connectors. Paths that reroute through more than one earlier assignment can't occur, and connector exclusions and planes filtered out by format are never mixed into the oracle cases.
- **Tie-breaking test (`:579`):** "order doesn't matter" is guaranteed by sorting in the code itself, so the test mostly repeats the implementation. The current-route tie preference is tested on only one case.
- **Mode ranking:** only "preferred beats non-preferred" is tested. Area against refresh, and equal keys falling back to index, aren't.
- **Discovery fixtures:**
  - Every connector is connected, with identical formats and encoders.
  - `bad_mode_count` (`:453`) can't happen in production, because `DrmQuery` builds both vectors from the same list.
  - Nothing tests the per-CRTC plane limit, the global plane limit, too many renderer formats, or a failed check on the second resource query.
  - The fault injection doesn't cover failures inside the adapter (property queries).
- **`PauseOnDrop` test (`system.rs:1085`):** it's a generic unit test; it doesn't run `create_gpu` through a discovery error.
