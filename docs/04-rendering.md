# GPU rendering, displays, and resource lifetime

## Initial renderer decision

Use Smithay's GLES renderer with EGL/GBM and DRM/KMS for native sessions, plus a nested backend for development. Its documented render abstractions include damage, render elements, imports, and synchronization. [Renderer API](https://docs.rs/smithay/latest/smithay/backend/renderer/index.html). The presence of Vulkan infrastructure elsewhere is insufficient to promise a Vulkan compositor. A later Vulkan implementation requires its own allocator/import/sync/effects validation and benchmarks.

## Frame pipeline

1. Receive client commits; validate state and track buffer ownership. Import lazily on the GPU that will consume the buffer.
2. Resolve acquire readiness asynchronously. Do not block the event-loop thread waiting on a fence.
3. Sample scene transforms at the predicted presentation time, collect visible elements, and compute damage including previous animated bounds.
4. Attempt eligible hardware cursor/plane/direct scanout paths, using backend constraints and atomic test commits where applicable.
5. If composition is needed, acquire a render target, render damaged regions and dependent effects, and submit with the correct synchronization.
6. Commit output state. If it fails, retain valid resources, record the failure, and retry a supported composition/state path.
7. On actual presentation or discard, update timing and finish callbacks according to protocol semantics. Release client resources only after all consumers are finished.

The [kernel KMS documentation](https://docs.kernel.org/gpu/drm-kms.html) is authoritative for atomic state and plane concepts; [Smithay DRM](https://smithay.github.io/smithay/smithay/backend/drm/index.html) supplies the integration layer.

## Buffer ownership and synchronization

Distinguish client buffer lifetime, imported texture lifetime, render-target lifetime, and KMS framebuffer lifetime. Maintain references for every output, plane, effect snapshot, and capture consumer. A client buffer reused before its last consumer completes can produce corruption even in memory-safe Rust.

Implement DMA-BUF feedback from actual import/scanout capability intersections, including format modifiers. Validate plane count, strides, offsets, dimensions, and driver import failures through the library and protocol rules. Do not advertise every format the client GPU supports. [DMA-BUF XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/stable/linux-dmabuf/linux-dmabuf-v1.xml).

For explicit sync, honor acquire and release timeline points with per-commit ownership and destruction handling. Advertise support only when the backend and selected device path can satisfy it. Preserve implicit-sync paths for clients not using explicit sync where supported. Missing capability is an explicit status; it cannot be hidden by advertising a nonfunctional global. [Syncobj XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/linux-drm-syncobj/linux-drm-syncobj-v1.xml).

## Damage and culling

Each scene element has stable identity, current bounds, previous bounds, content generation, transform, opacity, clip, and effect extent. Invalidate old and new bounds on movement/removal. Expand damage by shadow radius and blur dependency radius. Fractional transforms conservatively round outwards. Buffer age handling must include previous frame damage or repaint fully when age is invalid.

Occlusion culling uses verified opaque regions and effective opacity. Rounded corners, transforms, subsurfaces, and translucent effects invalidate naive rectangular coverage. A surface damage region is expressed in a defined coordinate space; transform it exactly once. Add diagnostic views for damage, opaque coverage, culling, plane assignment, and repaint causes.

## Effects and quality policy

Implement borders and rounded clipping first, shadows second, blur last. Cache reusable shadow masks and effect intermediates by size/scale/style generation. Use bounded blur passes, downsampled intermediate targets, and clip to visible affected regions. Blur must include background damage underneath a stationary transparent window.

Offer `performance`, `balanced`, and `quality` profiles with documented values. Reduced motion stops movement/fade transitions; a battery profile can separately disable expensive effects. Adaptive quality may adjust optional blur/shadow resolution with hysteresis, but must not reduce text resolution or input fidelity. Custom shaders remain experimental until compile time, resource use, and failure behavior are bounded.

## Direct scanout and hardware planes

Consider direct scanout for a suitable opaque surface covering an output without incompatible transforms/effects/capture. Eligibility depends on format/modifier, size, scale, color path, synchronization, cursor, overlays, and driver constraints. It is opportunistic. Fall back to compositing transparently when eligibility changes. Screen recording must explicitly resolve whether to copy/compose the source instead of bypassing capture. Track scanout success and rejection reasons.

## Multi-GPU

Identify render nodes and KMS devices independently. Prefer a render device minimizing copies for the active output arrangement. Import shared buffers across devices when formats/modifiers permit; otherwise negotiate or stage through a supported copy path with explicit cost reporting. GPU choice for applications remains an application/session concern; FluxWM must accept valid buffers from supported external devices.

Test iGPU internal display plus dGPU external display, external dock removal, application GPU offload, incompatible modifiers, and suspend. Keep per-device resources separate and discard them only after safe retirement. CPU readback fallback is diagnostic/degraded capability, not the normal promise of an optimized GPU desktop.

## Output state

An output records mode, refresh, transform, physical dimensions, logical scale, placement, render device, KMS device, and capabilities. Apply output configurations as transactions. Validate supported modes, try atomic tests, and retain previous working configuration on failure. Provide a timed user confirmation/revert flow for risky display changes in the eventual product.

Use per-output frame clocks, not one timer at the slowest monitor. Track submission deadline, predicted presentation, actual completion, and estimated rendering cost. Variable refresh is enabled only when the connector/driver path supports it; impose a policy for animation minimum cadence and mixed-refresh behavior. Cursor moves should use hardware updates where viable without waking unrelated outputs.

## Color and memory

Version 1 establishes a documented SDR/sRGB path and verifies gradients, alpha blending, screenshots, and display transforms. Linear-light compositing requires correct input/output conversion; do not apply gamma twice. Treat HDR/wide gamut as a gated extension needing protocol support, metadata, transfer functions, tone mapping, 10-bit formats, capture conversion, and mixed SDR/HDR testing.

An RGBA8 3840×2160 target is about 31.6 MiB. Three such targets are about 94.9 MiB before clients/effects. Set per-output transient pools and global effect/thumbnail budgets, expose usage, and evict only safe resources. Avoid full-resolution offscreen targets for every offscreen canvas window.

## Source-verified lifetime refinement

The [inspected Smithay sources](18-source-findings.md) document acquire-blocker integration and Buffer retirement semantics. FluxWM must retain ownership through actual GPU/plane/capture completion. A layout timeout does not authorize reading an unsignaled acquire point. A never-read discarded buffer may release before acquire under the protocol; do not impose an incorrect universal ordering assertion. These paths have separate [GPU acceptance cases](15-edge-cases.md).

Output capture and public-window blur need a privacy-safe scene/effect input graph; a final black rectangle can miss protected pixels sampled outside the rectangle. Capture redaction, scene picking, and presentation snapshots must remain coherent when enabling scanout/plane fast paths. Cross-device fallbacks must prove format and synchronization compatibility; CPU readback is not an assumed universal escape path.

Retain a source `Buffer` ownership guard per submitted read until nonblocking fence/page-flip retirement; the [concrete guard strategy](21-review-resolutions.md) defines blocker decisions and commit visibility limitations. Capture destination writes have their own leases and completion before ready. The privacy graph/W11 workload in [integration contracts](16-integration-contracts.md) accounts for the second export-safe composition instead of assuming a free black-rectangle pass.
