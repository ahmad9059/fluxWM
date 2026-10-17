# Wayland protocol capability plan

This is an implementation target matrix, not a list of protocols already supported. Phase 1 must inspect the selected Smithay revision and XML set, record protocol versions, and distinguish library helpers from completed FluxWM policy. Use [core protocol specification](https://wayland.freedesktop.org/docs/html/index.html), [xdg-shell XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/stable/xdg-shell/xdg-shell.xml), and the protocol source tree for semantics, subject to the [mirror/distro provenance limits](evidence/protocol-provenance.json). Selected production inputs require B01 provenance qualification. Never advertise a high version merely because generated bindings compile.

| Capability / interface family | First phase | FluxWM obligation and acceptance scenario |
| --- | --- | --- |
| Core compositor, surface, region, callback | 2 | Double-buffered commit state, destruction, regions, frame callbacks; malformed client cannot crash server |
| Shared memory and buffers | 2 | Validate pools/buffers and lifetimes; render SHM client and handle disconnect |
| Subcompositor/subsurfaces | 2 | Synchronized/desynchronized trees, ordering, offsets; menus/video subsurfaces redraw correctly |
| `xdg_wm_base`, toplevel, popup | 2 | Configure/ack/map/unmap, constraints, ping/pong, grabs, reposition; GTK/Qt dialogs and nested menus work |
| Seat, keyboard, pointer, touch | 2/4 | Correct serials, focus enter/leave and event grouping; change focus while keys/buttons are held |
| Output description, `xdg-output` | 3 | Correct logical and physical metadata; two transformed/scaled outputs agree with applications |
| `linux-dmabuf` | 3 | Validated import, format/modifier feedback; reject invalid buffers without server failure |
| `linux-drm-syncobj` | 9 | Capability-gated acquire/release; delayed GPU work never yields early reuse |
| Viewporter | 4 | Crop/scale surface content correctly; render toolkit viewport tests |
| Fractional scale | 4 | Preferred scale per surface and output movement; crisp fractional-DPI app rendering |
| Presentation time | 5 | Report actual presentation/discard timing, not submit time; compare to page-flip records |
| Cursor shape | 4 | Respect client cursor intent and compositor overrides; themed drag/resize cursors |
| Pointer gestures | 5/7 | Forward unowned swipe/pinch/hold sequences at implemented versions; cancellation and ownership are coherent |
| Tablet tool/pad (tablet-v2) | 7 | Tool proximity, pressure/axes, pad and cursor lifecycle; remove active device cleanly |
| XDG foreign | 7 | Validate exported/imported transient relationships; cross-app portal dialogs follow intended parent |
| Relative pointer and pointer constraints | 7 | Constrained/locked motion, cleanup; game capture exits safely on focus loss |
| Keyboard shortcut inhibition | 7 | Explicit user escape and active-surface policy; remote desktop receives keys without trapping user |
| Output power management | 7 | Trusted idle/display tools power outputs down/up; wake never presents stale unlocked buffers |
| Virtual keyboard | 7 | Trusted host IME/OSK route, tagged sandbox denial, lock-associated input ownership |
| Ext workspace | 7 | Panel workspace list/actions through selected versions; prove compatibility with chosen Waybar module |
| XDG dialog | 7 | Modal/transient state handling without freezing unrelated apps |
| Gamma control | 9 evaluation | Optional trusted color/night-light route; conflicts and scanout/capture color effects tested |
| Idle inhibit and idle notification | 7 | Visible media inhibition, lock/idle coordination; hidden media policy is documented |
| `wlr-layer-shell` | 7 | Anchors, exclusive zones, keyboard interactivity and layer ordering; panel reserves work area correctly |
| XDG decoration | 7 | Negotiate client/server decoration, draw titlebar where needed; avoid borderless unusable windows |
| Activation | 7 | Validate user-interaction tokens and focus-stealing policy; background app requests attention |
| Core data device | 7 | Clipboard/DnD serials, offers, MIME transfer, selection lifetime; copy and drag between toolkits |
| Primary selection | 7 | Distinct selection policy; middle-click paste in tested apps |
| Data control | 7 | Privileged selection access with explicit policy; clipboard manager can be disabled/revoked |
| Text input and input method | 7 | Coherent protocol pairing, focus, surrounding text privacy; compose CJK text in GTK/Qt |
| Foreign toplevel list/management | 7 | Stable identity, lifecycle, authorized actions; taskbar follows moves/close |
| Session lock | 7 | Cover every output and route input only to lock surfaces; lock client death never reveals desktop |
| Screen/image capture | 7 | Select supported ext/wlr route after capability spike; consent, cursor policy, sensitive-surface exclusion |
| Output configuration protocol | 9 | Validate and test configurations, rollback; invalid mode cannot strand user |
| Color management | 10 evaluation | Advertise only after actual rendering/capture path proves correctness; SDR remains baseline |
| DRM lease | 10 evaluation | Optional VR specialist path; exclude leased outputs from desktop and handle return |
| wp-security-context-v1 | 2 design / 7 implementation | Trusted broker listener lifetime, nested-context denial and per-context filtering; required before B16 sandbox tests |
| Accessibility extensions/bridge | 8 qualification | Chosen AT-SPI/keyboard-monitor route, with phase-5 semantic groundwork |

`wlr-*` and staging protocol families may evolve. Prefer interoperable current protocols where available, but retain compatibility where the tested desktop stack needs it. Select exact capture/toplevel/output-management families in their phase spikes and record the decision; similarly named protocols are not interchangeable.

## Surface lifecycle obligations

A surface receives a role once according to protocol rules. Keep pending and current state separate. Track viewport, scale, transform, input/opaque regions, attachment, damage, and subsurface synchronization. XDG initial mapping requires the protocol's configure/ack sequence; an acknowledged serial does not itself supply a new buffer. Null-buffer unmap, role destruction, and client disconnect have distinct effects. Keep popups attached to parent lifecycle and constrain their placement to a meaningful output region without exposing arbitrary global coordinates.

Each seat action that authorizes a client request validates serial, seat, client, and context. A client cannot use a random or stale serial to start a drag, move another window, or steal focus. Focus transitions update keyboard, pointer, text-input, activation state, and scene decoration coherently.

## Callback and pacing policy

Send frame callbacks based on whether a surface can make useful progress, rather than blindly firing all offscreen surfaces at monitor refresh. Offscreen throttling must avoid deadlocking applications whose animation/state machines need progress. Define a conservative fallback cadence and test browsers/video/toolkits. Presentation feedback must describe the buffer actually presented and discard replaced/unpresented commits appropriately. Multi-output surfaces require a deliberate primary scheduling policy.

## Protocol capability inventory format

Maintain a machine-readable inventory containing protocol source revision, interface, advertised version, required library features, backend restrictions, privileged binding policy, owner phase, conformance/client tests, and known limitations. Export runtime globals through diagnostics. On each dependency update compare the advertised inventory to implemented handlers. New global registration is a reviewed behavior change.

## Compatibility scenarios

Use native GTK and Qt clients, browser accelerated video, Electron applications, a game using relative pointer, multiple popup levels, drag between differently scaled outputs, clipboard transfers larger than a socket buffer, destroyed offers during transfers, IME focus switches, and malformed requests. Some protocols require compositor-specific policy beyond conformance tests. Validate both nested and DRM paths; a nested pass cannot prove KMS, device input, or secure session coverage.

## Capability and permission refinement

The [XML interface inventory](evidence/protocol-inventory.json) records source maximum versions separately from FluxWM advertised versions, which are currently null. It covers inspected XML only and is not the entire production protocol inventory. Add missing protocol families with exact sources as implementation chooses versions. Stable/staging/unstable labels are source namespace facts, not a confidence score for FluxWM code.

Optional compatibility candidates include content-type hints, tearing control, FIFO/commit-timing scheduling, single-pixel buffers, toplevel icons/tags, DRM lease, and color management. Evaluate each against actual application demand, pinned library support, and security/lifetime cost in phases 7–10. A required application flow blocked by a missing candidate creates a concrete compatibility issue; do not advertise placeholder globals to silence toolkit warnings.

Global visibility does not revoke already bound objects. Every sensitive capture/input/output/lock operation checks current model-A host/sandbox classification and operation authorization and source generation; see [trust policy](17-resource-security.md). No valid normal client can self-promote using an app ID, title, sandbox label supplied through an untrusted route, or guessed helper PID.

Complete foreign-toplevel **management actions** separately from read-only list enumeration; the list protocol alone cannot activate/close taskbar windows. Phase 7 owns tablet-v2, xdg-foreign, pointer gestures, virtual keyboard, ext-workspace, output-power and xdg-dialog integration as explicit tasks. Companion examples are candidates until their exact module/protocol versions pass a session test. Window capabilities/bounds/suspended/tiled/constrained flags follow the [XDG matrix](22-window-state-matrix.md).
