# Research, sources, and implementation reading guide

Accessed 2026-10-17. Links to `main`, `master`, and `latest` move. Phase 1 must record exact commits, crate versions, compiler version, and protocol XML hashes. The source facts below are deliberately separated from FluxWM design recommendations elsewhere.

## Verified findings

- **Wayland:** the compositor is the display server; clients exchange buffers and input through protocols. This makes output lifecycle and protocol state part of FluxWM, beyond tiling policy. [Official architecture](https://wayland.freedesktop.org/architecture.html), [protocol reference](https://wayland.freedesktop.org/docs/html/index.html).
- **niri:** its documented workflow uses columns on a horizontal strip, independent monitor layouts, and vertically arranged workspaces. It is a reference for scrolling behavior, not proof of arbitrary 2D canvas support. [Project README](https://github.com/niri-wm/niri).
- **Hyprland:** animation configuration exposes named curves, including Bézier and spring parameters. FluxWM should study the motion concepts rather than copy its evolving configuration syntax. [Official animation guide](https://wiki.hypr.land/configuring/core/animations/).
- **Smithay:** supplies Rust compositor building blocks and sample compositors; it does not provide FluxWM's window policy. [Project](https://github.com/Smithay/smithay), [getting started](https://github.com/Smithay/smithay/blob/master/GETTING_STARTED.md).
- **Renderer caveat:** the inspected released renderer documentation identifies GLES and includes pixman/multi-GPU modules. A backend Vulkan allocator is not evidence of a ready Vulkan scene renderer. Choose GLES initially. [Renderer API](https://docs.rs/smithay/latest/smithay/backend/renderer/index.html), [backend API](https://docs.rs/smithay/latest/smithay/backend/).
- **DRM:** Smithay provides device/surface abstractions and a DRM compositor for plane composition. FluxWM still chooses compatible rendering/output configurations. [Smithay DRM](https://smithay.github.io/smithay/smithay/backend/drm/index.html), [kernel KMS reference](https://docs.kernel.org/gpu/drm-kms.html).
- **Lua:** `mlua` supports embedded Lua and serialization integration; its Lua object provides memory limits and instruction hooks with backend-specific restrictions. These are useful controls, not an OS security boundary. [Crate](https://docs.rs/mlua/latest/mlua/), [Lua API](https://docs.rs/mlua/latest/mlua/struct.Lua.html).
- **Desktop sharing:** xdg-desktop-portal has frontend and backend interfaces. A compositor needs a compatible backend and capture integration; merely implementing screencopy is insufficient for browser portal support. [Portal documentation](https://flatpak.github.io/xdg-desktop-portal/docs/), [backend interfaces](https://flatpak.github.io/xdg-desktop-portal/docs/impl-dbus-interfaces.html).
- **X11:** xwayland-satellite is a separate Xwayland integration project. Evaluate its compositor requirements and lifecycle before selecting it. [Upstream](https://github.com/Supreeeme/xwayland-satellite).

## Primary source register

| ID | Source | Use during implementation |
| --- | --- | --- |
| S01 | [Wayland protocol specification](https://wayland.freedesktop.org/docs/html/index.html) | Surface commit, buffers, seats, data devices, object lifetimes |
| S02 | [wayland-rs](https://github.com/Smithay/wayland-rs) | Rust client/server dispatch and generated bindings |
| S03 | [Smithay getting started](https://github.com/Smithay/smithay/blob/master/GETTING_STARTED.md) | Understand library approach and learning path |
| S04 | [Smallvil](https://github.com/Smithay/smithay/tree/master/smallvil) | Minimal compositor source reading |
| S05 | [Anvil README](https://github.com/Smithay/smithay/blob/master/anvil/README.md) | Nested and DRM backend examples, system dependencies |
| S06 | [Smithay renderer](https://docs.rs/smithay/latest/smithay/backend/renderer/index.html) | Render elements, damage, imports, synchronization |
| S07 | [Smithay DRM](https://smithay.github.io/smithay/smithay/backend/drm/index.html) | Output state, plane composition |
| S08 | [niri](https://github.com/niri-wm/niri) | Study layout and compositor boundaries; obey license for any reuse |
| S09 | [niri design principles](https://github.com/niri-wm/niri/wiki/Development%3A-Design-Principles/bff576e2dc7f024f19414bc05e45aebdc6059d16) | Historical design discussion; verify against current source |
| S10 | [Hyprland animations](https://wiki.hypr.land/configuring/core/animations/) | Motion terminology and user configuration concepts |
| S11 | [mlua crate](https://docs.rs/mlua/latest/mlua/) | Embedded Lua features and Rust conversion |
| S12 | [mlua Lua object](https://docs.rs/mlua/latest/mlua/struct.Lua.html) | Limits, hook behavior, VM lifecycle |
| S13 | [Kernel KMS](https://docs.kernel.org/gpu/drm-kms.html) | Atomic modesetting, planes, formats, modifiers |
| S14 | [libinput documentation](https://wayland.freedesktop.org/libinput/doc/latest/) | Device events, gestures, configuration |
| S15 | [Calloop](https://github.com/Smithay/calloop) | Event-loop integration |
| S16 | [Portal documentation](https://flatpak.github.io/xdg-desktop-portal/docs/) | Portal selection, configuration, integration |
| S17 | [Portal backend interfaces](https://flatpak.github.io/xdg-desktop-portal/docs/impl-dbus-interfaces.html) | ScreenCast/Screenshot backend obligations |
| S18 | [PipeWire](https://pipewire.pages.freedesktop.org/pipewire/) | Capture stream buffer negotiation and delivery |
| S19 | [xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite) | External X11 bridge feasibility |
| S20 | [WLCS](https://github.com/canonical/wlcs) | Conformance infrastructure and integration effort |
| S21 | [xdg-shell XML mirror](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/stable/xdg-shell/xdg-shell.xml) | Normative configure, popup, mapping semantics |
| S22 | [session-lock XML mirror](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/ext-session-lock/ext-session-lock-v1.xml) | Secure session coverage and lock lifecycle |
| S23 | [DRM syncobj XML mirror](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/linux-drm-syncobj/linux-drm-syncobj-v1.xml) | Acquire/release timeline semantics |
| S24 | [DMA-BUF XML mirror](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/stable/linux-dmabuf/linux-dmabuf-v1.xml) | Imports, formats, feedback |

The freedesktop GitLab staging directory returned an access-denied challenge during research. Protocol source links above use a source-code mirror of upstream XML; verify each selected file against the upstream repository or installed distribution package in phase 1. The Wayland Explorer pages were consulted for navigation but are not the normative source for this plan.

## Reading sequence and research questions

1. Read S01/S02 and trace a client from registry binding through first buffer commit to destruction. Write object lifetime diagrams.
2. Read S03–S07 and run a pinned Smallvil/Anvil outside FluxWM. Trace input dispatch, render element generation, damage, and page-flip completion.
3. Read S08/S09 for policy ideas. Reimplement independently unless an explicit license decision permits source reuse. Determine which behavior belongs to layout versus protocol glue.
4. Read S10 and derive a small analytic spring/Bézier engine with deterministic clocks. Do not tie animation lifetime to frame count.
5. Read S11/S12 and prototype restricted Lua evaluation in a supervised helper process. Verify runaway coroutine, allocation failure, and reload rollback.
6. Read S13/S23/S24 before advertising GPU extensions. Trace ownership across import, rendering, scanout, release, and device removal.
7. Read S16–S19 and produce working integration spikes for lock, capture, and X11 before committing to their full architecture.

## What research does not establish

No source establishes FluxWM's speed, latency, NVIDIA support, accessibility completeness, battery life, or reliability. Those require implementation and recorded measurements. No FluxWM build dependency combination is pinned or qualified yet. The deeper audit adds immutable research snapshots in the [source manifest](evidence/upstream-sources.json), which are not a selected build dependency set. The inspected renderer page displayed Smithay 0.7.0; that is an observation, not a claim that it is the newest version or the correct version to select. No estimates below are upstream delivery commitments.

## Deeper audit and evidence

See [immutable source findings](18-source-findings.md) for commit-specific API observations, [local preflight](evidence/local-preflight.json) for development prerequisites actually checked, and [production blockers](13-production-audit.md) for unresolved implementation decisions. The Lua 5.4 error-catchability reproducer changed the config design from thread-only execution to a supervised helper process. No runtime/compositor qualification follows from source inspection.

Protocol mirror provenance was independently questioned and then compared with installed distribution XML: see [per-file hashes](evidence/protocol-provenance.json). Twelve match; DMA-BUF contains an explicitly recorded difference. Mirror byte hashes alone never authenticate freedesktop origin. B01 must resolve selected source provenance before dependency qualification.
