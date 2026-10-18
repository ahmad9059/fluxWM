# Native output discovery and candidate routing

The optional native driver now acquires connector/mode, encoder/CRTC and primary-plane metadata after GPU initialization. It computes independent output routes before publishing GPU readiness. `GpuInfo.output_plan` is a **candidate plan**, not a plane reservation, buffer allocation, KMS test, display configuration or presented frame. Main FluxWM remains nested-only. The manual `--session` probe remains unexecuted here and still requires a separate authorized VT; `--check` continues to perform udev inventory only.

## Acquisition and identity

The DRM adapter uses pinned `drm-rs` connector/encoder/resource queries and Smithay `DrmDevice::planes`. It force-probes connectors only when this driver prepares a GPU at startup, explicit device change or resume. It reads the `non-desktop` property, treating absence as an ordinary desktop connector, and rejects unreadable or incorrectly typed properties rather than silently assuming their value. EDID, serial numbers, mode names and connector names are not retained in the plan.

Encoder CRTC masks are resolved against the **original resource-list ordering**, not numeric CRTC handle order. Primary planes discovered under multiple CRTCs merge into one resource with multiple compatibility edges; their exact format sets must agree. The adapter retains original kernel mode objects privately, indexed by connector and original mode index. A future commit must use that exact mode and current GPU ticket, then revalidate against device changes; rounded resolution/refresh fields cannot reconstruct the original timing.

Acquisition validates bounded counts, nonzero/unique IDs, references, raw/policy mode-index alignment and matching resource-ID sets before and after querying. A query failure returns no inventory. A changed ID set is diagnosed as invalid topology. Matching ID sets do **not** establish an atomic snapshot: connection state, mode lists, properties and compatibility may change without handle changes. KMS test/commit and event-generation validation remain mandatory. These limits bound retained application data; they do not bound allocations or latency inside kernel/library queries.

## Routing and mode policy

The planner builds a unit-capacity residual network from connectors through encoders and CRTCs to primary planes. Every resource can serve at most one independent output. Augmenting paths can reroute an earlier flexible assignment to accommodate a constrained connector. The resulting number of routes is maximal for the supplied compatibility graph; cloning, MST bandwidth/link limits, GPU-wide constraints and actual buffer/KMS feasibility require subsequent backend testing and are not inferred from that graph.

Sorting resource IDs makes results independent of discovery order. Current encoder/CRTC preferences order ties, but this is not a minimum-disruption optimizer and does not promise to preserve all existing assignments. No live output exists in this stage. Publication and rollback of an existing display configuration remain separate work.

Disconnected, unknown and non-desktop connectors receive explicit exclusion reasons. Connected desktop connectors need a valid mode and a complete compatible route. A mode needs nonzero dimensions/clock and ordered sync timings; malformed or zero-refresh modes are excluded. Selection prefers preferred modes, then larger pixel area, higher rational refresh and earlier original index. Refresh is rounded from pixel clock and complete totals, including interlace, doublescan and vscan; integer `vrefresh` labels are not used. Custom modes, unsupported timing flags, mode fallback after failed allocation/test and user output policy remain unimplemented.

A plane is eligible only if its format/modifier set intersects the renderer's render-target set exactly. Import support is not substituted for render support. Modifiers, including implicit/INVALID versus Linear, remain distinct; full-width modifiers serialize as decimal strings. A nonempty intersection is a candidate list only. GBM allocation, framebuffer export, selected-plane claims, atomic testing, successful commit and frame/buffer ownership must still establish feasibility.

## Bounds and verification

Current admission limits are 64 connectors, 128 encoders, 32 CRTCs, 128 primary planes, 256 modes per connector, 256 formats per plane/renderer, and 256 connector properties. Exceeding a limit fails GPU preparation with a discovery/topology stage rather than returning a silently truncated plan. These are implementation limits, not qualified hardware support limits.

```sh
cargo test -p flux-native --locked
cargo test -p flux-native --features system --locked
cargo clippy -p flux-native --features system --all-targets --locked -- -D warnings
```

The device-free planner suite compares all 4,096 two-by-two connector/encoder/CRTC/plane graphs against an independent exhaustive route-combination oracle. It checks cardinality and resource/reference/format invariants, constrained rerouting, topology permutations, shared planes, modifier distinctions, mode ranking/timing, exclusions, invalid identities, capacity and empty/outputless plans. This is exhaustive for those small graphs, not all bounded graphs or actual hardware feasibility.

The acquisition suite drives the production query coordinator through a fixture provider. Each of its eight query positions fails in turn and must abort without partial publication. Fixtures cover shared-plane merging, original raw-mode indices, resource churn versus reordering, initial capacity rejection, conflicting plane formats and mismatched mode records. They do not execute the DRM ioctl adapter or real libseat/GPU lifecycle. Broader Kernel/Driver integration, property type/error behavior on hardware and every physical output transition remain unqualified.

## Next integration requirements

Before enabling a native desktop, extend the owned GPU domain to output-manager/surface/plane claims, GBM buffers, front/pending frames and render completion. Select compatible formats/modifiers through actual allocation and KMS testing. Retain prior working configurations on failed output transactions, supply an acknowledged fifteen-second preview/revert, and make outputless recovery preserve workspaces. Only publish output globals/scene state after the hardware transaction's required success point. Then integrate rendering and input into the main compositor and qualify VT/hotplug/suspend/device loss on physical hardware. The complete [phase-3 gate](../phases/03-gpu-backend.md) remains open.
