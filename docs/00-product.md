# Product specification

## Intended experience

FluxWM is a Linux Wayland compositor written primarily in Rust. It owns window management, input routing, compositing, and output presentation. Users arrange applications on an expanding canvas, navigate smoothly with keyboard or gestures, and configure behavior through Lua. The inspiration is niri's scrolling workflow and Hyprland's responsive visual motion; FluxWM uses its own implementation and configuration contract.

The default workspace opens each new tiled window in a column after the active column. Existing columns retain their widths. Multiple windows can share a column vertically or as tabs. The viewport pans to expose focus without rearranging unrelated windows. Each output owns independent workspaces and cameras. Moving the pointer across outputs does not reveal a continuation of another output's strip.

Users can switch a workspace to `canvas` mode: columns become persistent positioned groups on an unbounded two-dimensional plane. They can pan in both axes, zoom in overview, move groups with snapping, and jump between distant applications through search. Switching modes retains a stored layout for each mode and preserves window identities. The full mode is delivered in phase 8, after the default strip is reliable.

## Product invariants

1. A mapped application window belongs to exactly one workspace and one layout container, except transient rendering snapshots with no input authority.
2. Keyboard focus always identifies a live eligible surface or is explicitly empty. Pointer focus follows the geometry currently drawn.
3. Opening a column does not resize unrelated columns. Reflow inside an explicitly edited stack is allowed.
4. Every window is recoverable through focus/search/overview even after monitor removal, rule changes, or extreme panning.
5. Layout, camera, and render coordinates are distinct. Large canvas positions do not overflow client protocol coordinates.
6. No compositor thread waits synchronously for a client redraw, Lua evaluation, portal response, or child process.
7. A lock request becomes acknowledged only when outputs are safely covered according to protocol semantics. A lock client crash leaves the session protected.
8. The compositor advertises only protocol requests it implements at the advertised version.
9. GPU failure degrades explicitly or ends the session cleanly; it cannot silently reuse released client buffers.
10. Reduced motion and keyboard operation cover every built-in navigation flow.

## Feature scope

| Area | Required release behavior | Later expansion |
| --- | --- | --- |
| Layout | Strip, columns/stacks/tabs, floating, fullscreen, 2D canvas, overview, search | Alternative tiling policies |
| Navigation | Directional focus/move, width presets, gestures, camera history, workspace/output transfer | User-defined navigation graph |
| Visuals | Interruptible camera/window/fade animations, borders, shadows, optional blur | Arbitrary third-party shaders |
| Displays | Hotplug, independent refresh, transforms, fractional scale, adaptive sync where supported | Advanced calibration workflows |
| GPU | GLES composition, DMA-BUF import, atomic KMS, direct scanout when eligible, explicit sync on capable stack | Vulkan compositor renderer after separate proof |
| Hardware | Tested Intel/AMD/NVIDIA configurations and hybrid laptops | Universal compatibility is not a promise |
| Apps | Native Wayland, tested Xwayland strategy, popups, drag-and-drop, clipboard, IME | Exotic legacy X extensions |
| Desktop | Panels/wallpaper, launcher integration, notifications, lock/idle, portal screenshot/cast | Full bundled desktop shell |
| Configuration | Lua schema, rules, bindings, outputs, animations, atomic reload, diagnostics | Stable plugin ABI |
| Operations | IPC/CLI, session launcher, logs, safe defaults, packaging, troubleshooting | Live compositor restart retaining clients |
| Accessibility | Reduced motion, keyboard navigation, scalable UI, zoom, screen-reader integration assessment and tested supported flow | Broad assistive feature expansion |
| Color | Correct SDR/sRGB path and documented assumptions | HDR/wide gamut capability gate in phase 10 |

A complete window manager experience includes tested companion services. It does not require rewriting a terminal, notification daemon, authentication agent, file picker, audio service, or network manager. FluxWM must document and validate their integration.

## Definition of implementation completion

All ten phase gates must pass. Demonstrate a normal work session with browser, terminal, editor, X11 application, video call screen sharing, lock/unlock, suspend/resume, monitor docking, fractional scaling, input method, and configuration reload. Publish hardware limitations, reproducible benchmark records, application compatibility results, and the protocol capability inventory. Optional research such as Vulkan/HDR may conclude with a documented unsupported status; the required GLES/SDR experience cannot depend on it.

## Example journey

Launch into a working session, open a terminal and browser without shrinking either, move by keyboard, pan through a long strip, stack an editor beside terminals, float a settings dialog, fullscreen a video, move a workspace onto a projector, remove the projector, locate every window, switch to a 2D canvas and pan vertically, share one window through portal consent, then lock (the retained stream becomes black) and unlock (resume only after grant/source revalidation). Repeat with animations disabled. Each step becomes a phase 10 end-to-end acceptance record.

The supported baseline trusts unsandboxed same-user host programs consistently with local IPC/config control; portal consent does not prevent trusted host capture tools. Tagged sandbox connections are restricted. Fullscreen/client states, capture retention, helper isolation and release qualification use the explicit [review decisions](21-review-resolutions.md), with unresolved implementation proof tracked rather than assumed.
