# Desktop and UI integration contracts

Design requirements below must be qualified against chosen versions. Upstream facts are in [source findings](18-source-findings.md); where no working spike exists, the decision remains [blocked for implementation claims](13-production-audit.md).

## Capture route selection

The observed xdg-desktop-portal-wlr source includes both ext image-copy and legacy wlr capture dispatch and contains toplevel handling. This source observation does not prove an installed release supports FluxWM window capture. Phase 7 must pin and build a candidate, inspect its required globals, and demonstrate output **and individual-window** sharing in a real browser.

Candidate route A reuses a compatible portal backend and implements its exact capture/toplevel requirements. Candidate route B provides a FluxWM-specific backend with a stable private source-selection API. Select on actual functionality, permission model, maintenance cost, and chooser accessibility. Do not choose only by crate availability or compositor brand. Both routes need file-picker and other desktop portal delegation, not just ScreenCast.

| Contract | Required behavior |
| --- | --- |
| Request lifecycle | Track D-Bus request/session owner; cancel on owner loss; avoid replying twice |
| Source identity | Use immutable window/output generation within a session, not mutable title, app ID, or list index |
| Permission | Bind grant to authenticated portal request and exact source/scope; deny tagged sandbox raw clients; trusted unsandboxed host tools follow model A |
| Restore | Validate grant lifetime, source existence, and revocation before reuse; do not treat saved layout names as authority |
| Constraint negotiation | Re-advertise dimensions/formats when source changes; reject outdated consumer buffers coherently |
| PipeWire | Negotiate buffers/metadata and honor backpressure without waiting on compositor thread |
| Cursor | Match hidden/embedded/metadata mode; avoid capture of password-adjacent or stale cursor textures while locked |
| Window geometry | Define whether decorations/popups are included; source remains window-local despite animated canvas movement |
| Hidden/minimized source | Keep the same authorized window-local capture source updating; active capture prevents suspended hint; no output substitution |
| Privacy rule | Evaluate before capture and effect sampling; account for translucent/blurred surfaces and thumbnails |
| Failure | Stop/fail source on lost authorization or irrecoverable device state; retain resource leases until reads finish |

A private window under a transparent/blurred public window can leak through composited effects. Output capture needs a redacted scene whose effect dependencies exclude protected content, or a conservative opaque replacement of affected regions. A window capture of a public window that samples desktop blur must similarly use safe inputs. A simple post-composition black rectangle may be insufficient. Provide exact screenshots for these cases and validate scanout fallback too.

Capture policy applies separately to output streams, window streams, screenshot previews, overview thumbnails exported externally, DMA-BUF export, and debug dump APIs. Local-only overview may show private windows by product policy; its buffers cannot then be reused as authorized external thumbnails. Distinguish internal visibility from export authorization.

## X11 handoff

The pinned satellite README declares host xdg-shell and viewporter requirements, optional integrations, and HiDPI limitations. Treat that as a starting contract. Build the selected version and record Xwayland version, renderer path, protocol inventory, and exact readiness mechanism before launching clients.

On-demand integration must reserve sockets/display identity atomically, supervise the helper, and publish a usable DISPLAY only after readiness. Avoid global hard-coded display numbers and orphaned X sockets. A crashed satellite/Xwayland server ordinarily loses its live X connections; helper restart is service recovery, not transparent app restoration. Explain this in the user support guide.

Test transient dialogs, override-redirect menus/tooltips, fullscreen games, Wine/Java, mouse confinement, selection ownership, INCR-style large transfers where the chosen bridge handles them, multiple X11 windows from one process, and app title/ID matching. Missing app metadata cannot become an authorization bypass. In mixed DPI, validate actual text/input scaling on each output instead of claiming native fractional-scale parity.

## Input method contract

Select exact text-input and input-method protocol versions as a compatible pair. Apply model-A trusted-host classification to input-method binding, with protocol singleton/seat rules; while locked, route any accepted trusted-host IME/virtual-keyboard input only into the current lock tree. No unspecified per-program approval is claimed. Track activation, focus, serial/commit sequencing, surrounding-text updates, content purpose/hints, preedit/commit/delete operations, and popup placement through animated transforms. Password contexts must not leak surrounding text into logs or ordinary observers.

Candidates display in output logical space derived from the current surface-local cursor rectangle. They must follow surface movement and scale changes without moving the underlying application caret. On focus switch/death, deactivate old context and clear helper state according to protocol. Include different language engines and GTK/Qt/Electron behavior in qualification. Compose/dead-key handling must avoid double composition between toolkit and compositor paths.

## Clipboard and drag-and-drop contract

Data transfer uses nonblocking pipes/fds with cancellation, deadline policy, size accounting, and receiver backpressure. The compositor routes offers; it does not need to load all clipboard bytes into memory. A clipboard manager can persist data only under an explicit policy and must not capture lock/password content unintentionally. Core selection, primary selection, and privileged data-control are distinct capabilities.

Drag state records initiating seat/client/serial, source MIME types/actions, icon, target, accepted action, and completion. A canvas pan or workspace change during drag must transform picking correctly and preserve drag ownership; source or destination destruction cancels cleanly. Never infer an authorized drag from an arbitrary IPC request or replayed serial.

## Accessibility and built-in UI

FluxWM's own search/overview/confirmation/screenshot UI requires semantic roles, labels, selected/focused state, navigation relationships, and actionable controls. A textured OpenGL label is not an accessible UI element by itself. Candidate implementation: AccessKit Unix adapter for compositor-owned UI and the compatible keyboard-monitor D-Bus contract needed by the selected assistive stack. This is a proposed route backed by upstream examples, not transferred compatibility.

Define a UI tree with stable node IDs per generational object, a root per active compositor UI context, window/group/workspace labels, search input, results, buttons, and announcements. Do not duplicate application-owned accessibility trees. Actions arrive asynchronously and validate current node ID/permissions before invoking core commands. Track focus transitions in and out of compositor UI and de-duplicate announcements during animation.

Accessibility adapter/D-Bus communication cannot block the event loop. Use bounded worker queues and explicit disconnected state; full queues trigger resync/restart or a clear failure rather than silent permanent loss. Never discard arbitrary tree updates without repairing consistency. Nested mode cannot take the host's session-wide keyboard-monitor name or global assistive authority. Full session startup owns that registration deliberately.

Trusted screen-reader key monitoring/input capture must have a threat model and lock policy. Ordinary app IDs cannot authorize global observation. Companion launcher, chooser, terminal, and locker must themselves be accessible; qualify the entire journey with Orca and keyboard alone. B09 stays open until exact versions and observed behavior are recorded.

## Text rendering and internationalization

Perform the minimal text/font spike by phase 3 (B17), then select/pin and qualify the full compositor UI stack by phase 5; do not hand-place code points. Required behaviors: UTF-8 validation, grapheme-aware truncation/editing, bidi paragraph direction, font fallback, combining marks, emoji sequences, scale-correct rasterization, and glyph-cache budgets. Candidate libraries must be evaluated for Rust compatibility and render integration; no library is selected by this document.

Keep localized UI strings separate from protocol/debug identifiers. Honor locale for presentation while preserving stable action names and IPC keys. Treat window titles as untrusted text: bound byte/codepoint length, escape terminal controls in logs, and distinguish display direction from identity. Search normalization/casefolding must be documented and preserve original labels. Dynamic text updates cannot churn unlimited glyph atlases.

## Fullscreen, floating, and layering

Specify scene precedence: background/bottom shell, workspace tiled/floating content, top shell, compositor interaction UI, overlay shell according to policy, then secure lock coverage/lock surfaces. Fullscreen may suppress top panels/floating content according to the product rule, but ordinary overlay clients cannot outrank secure lock. Tooltip/popups inherit role and parent placement rather than becoming unconstrained desktop overlays.

Modal dialog behavior follows transient relationships when available; do not freeze unrelated windows solely because a client set a title or app hint. Floating geometry is stored relative to a usable output region and restored/clamped after output changes. Focus search must expose hidden tabs and offscreen groups without violating popup/lock grabs. Maximize and fullscreen are separate client states; the proposed usable-area/physical-output and restore behavior is specified in the [implementation contracts](20-implementation-contracts.md) and requires client tests.

## Session services

The [systemd desktop integration guidance](https://systemd.io/DESKTOP_ENVIRONMENTS/) documents shared user-session environment constraints and graphical session targets. FluxWM's supported systemd mode should own a session target, essential services in the session slice, appropriate app launch scopes/services, and explicit termination ordering. This guidance is work in progress; final units require clean-session tests.

Nested mode uses private sockets/children and never imports display variables into the host user-service environment. A non-systemd mode needs documented D-Bus/seat/autostart equivalents or an explicit unsupported designation. Do not terminate unrelated user applications/services during shutdown. XDG autostart and Lua autostart must have one ownership policy to prevent duplicate starts, with optional companion failures clearly distinguishable from missing required session components.

## Privacy graph and capture workload (B20)

Privacy is a compositor-owned class on stable surface/window groups. Provide an explicit user action to mark private independently of app_id. App metadata rules are convenience classification, not authenticated security identity: re-evaluate on every relevant change before capture admission, merge capture rules deny-first within each evaluation and keep deny sticky until trusted user clearance, and conservatively redact not-yet-classified content. A source chooser cannot silently clear a private mark.

Propagate private state to subsurfaces, related popups/transients, text-input-associated IME/OSK surfaces, retained snapshots, thumbnails and derived compositor UI. Notification content has no universally trustworthy parent association: exclude notification overlays from output capture by default unless a tested helper labels safe content explicitly. Cursor images, magnification and overview exports derive from the capture-safe graph; do not reuse the literal on-screen framebuffer when it contains protected pixels.

Effects in the export graph sample export-safe inputs only. Individual-window capture uses a solid neutral background for transparency/blur and never samples the desktop. Closing snapshots and caches retain privacy labels and generation invalidation. Add W11: active output and window sharing, blur enabled, private app plus IME/notification/overview/magnifier changes, then lock/unlock. Measure the additional redacted render/copy CPU/GPU time, memory and frame deadlines; the original no-capture budgets do not cover a second composition for free.

B20: ownership/labels designed phase 3, plain capture graph qualified phase 7, effects/scanout/magnifier requalified phase 9. CAP-04/CAP-11 require pixels AND effect/source graph evidence.
