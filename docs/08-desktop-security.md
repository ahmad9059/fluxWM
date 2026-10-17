# Desktop integration, security, and operations

## Session composition

FluxWM supplies compositor, layout, navigation UI, IPC, output management, and capture endpoints. A supported session bundle integrates a panel, launcher, notification daemon, lock client, idle manager, authentication agent, portal backend, and PipeWire. Exact companion choices are phase 7 deliverables after compatibility checks. Users can replace them through configuration.

A session launcher establishes the user session environment, starts FluxWM, waits for its display socket readiness, then exports the actual display environment to relevant user services. Avoid hard-coded `wayland-0`, global environment edits, or leaking nested-session variables into the host. Update D-Bus/systemd user activation environments using the selected distribution's supported route. Configure `XDG_CURRENT_DESKTOP` and portal preference files deliberately and document the installed names.

Install a Wayland session desktop entry, executable launcher, config example, manuals, schemas/reference, shell completions, and portal configuration where needed. Keep logs in the journal/stderr by default and state under XDG state directories. A minimal session should still launch a terminal when optional shell services fail.

## Seat and suspend lifecycle

Use a supported seat/session mechanism through the selected backend rather than running the compositor as root. Acquire/release device access according to session activity. On VT switch, stop accessing inactive input/DRM devices and resume only after reactivation. Input device reattachment reconstructs settings without stale handles.

Before suspend, coordinate with the session's idle/lock service and inhibitor mechanism. The lock must be safely established before display/session transition where policy requires it. On resume rebuild output state, invalidate stale timing, repaint safely covered outputs, then expose content only after legitimate unlock. Test lid closure with external monitors, all outputs powered off, and repeated suspend cycles.

## Locking threat model

The protocol source [session-lock XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/ext-session-lock/ext-session-lock-v1.xml) defines the lock lifecycle. FluxWM policy must additionally reject ordinary shortcuts/application input and new capture grants while locked; retain existing captures as black under the selected policy.

Use conceptual states `Unlocked`, `LockPending`, `Locked`, and `Unlocking`. On lock pending, immediately block sensitive input paths and establish compositor-owned safe coverage. Only acknowledge according to protocol requirements once outputs are protected. A new hotplugged output starts covered. On lock client death retain safe coverage and locked state; permit a documented recovery lock-client route that cannot unlock by itself. Never return to the desktop merely because a helper exited.

A lock surface is not a normal always-on-top panel. Secure locking must account for scanout, output creation, capture streams, pending frames, input grabs, and cursor content. Authentication belongs to the lock application's supported mechanism; FluxWM should not invent a password database. Privileged local/root attackers and compromised kernel/GPU drivers are outside this display isolation model; same-user helpers with broad session privileges need explicit treatment.

## Capture and portal integration

[Portal frontend/backend interfaces](https://flatpak.github.io/xdg-desktop-portal/docs/impl-dbus-interfaces.html) and [PipeWire](https://pipewire.pages.freedesktop.org/pipewire/) are separate layers. Run a phase 7 feasibility spike: pick a portal backend that can connect to FluxWM's capture route, or scope a FluxWM backend implementing the necessary ScreenCast/Screenshot methods and source chooser. Do not claim that setting an environment variable creates support.

The source chooser identifies output/window/region using stable IDs and confirms consent. Create a PipeWire stream with negotiated pixel format, dimensions, strides/modifiers, memory type, timestamps, and cursor policy. Handle resize, source destruction, GPU import mismatch, and consumer backpressure. Bound capture frame queues; drop obsolete frames rather than stall presentation. While locked, retained authorized capture streams contain opaque black with no cursor/lock UI; actual revocation or source loss terminates them. Remove sensitive surfaces from all export paths, including effect inputs, thumbnails and direct scanout handling.

Remote input is a separate capability from capture. If implemented, require explicit permission, session lifecycle, and lock revocation. Screenshot tools with direct capture-protocol access may bypass portal prompts, so privileged global access policy must be explicit and tested.

## X11 strategy

Evaluate external [xwayland-satellite](https://github.com/Supreeeme/xwayland-satellite) against integrated Xwayland/XWM using the pinned Smithay source. Prefer the option whose required protocols, popups, clipboard, gaming, scaling, and helper lifecycle pass the phase 7 matrix. External integration reduces in-process XWM responsibility but creates its own deployment dependency; integrated support adds event/selection/focus coordination.

The decision must cover on-demand launch, DISPLAY publication after readiness, helper crash recovery, child cleanup, X11 selection bridges, transient windows, override-redirect menus, cursor grabs, fullscreen games, and high-DPI behavior. X11 clients sharing an X server have weaker mutual isolation than native Wayland clients; do not describe the bridge as making legacy apps equivalent to native isolation.

## IPC and CLI design

Propose a Unix socket under `$XDG_RUNTIME_DIR/fluxwm/`, unique per compositor instance, with restrictive permissions and peer-credential validation. Protocol v1 uses length-prefixed JSON with maximum message size, request ID, version, method, and parameters. Responses include result or typed error and current state sequence. Events include sequence and config generation. Slow subscribers get bounded queues and a resync signal or disconnect.

Read methods: version/capabilities, outputs, workspaces, windows, effective config, diagnostics, subscriptions. Mutation methods map to validated core commands. IDs carry generation where needed. Spawn/quit/config changes are privileged same-user control operations, not exposed to arbitrary Wayland clients. Same UID is a practical local-control boundary, not sandbox isolation; evaluate security-context/allowlisting for restricted clients before promising stronger guarantees.

Proposed examples, unavailable until implemented:

```text
fluxwm --backend nested --config ./config.lua
fluxctl windows --json
fluxctl action focus --direction right
fluxctl config check ./config.lua
fluxctl config reload
fluxctl diagnostics export --redact
```

## Privacy and reliability

Do not log clipboard contents, keystrokes, passwords, or raw frame pixels. App titles and paths may be sensitive; redact them from default diagnostics bundles. Make debug capture an explicit local action. Rate-limit repeated malformed-client errors and GPU retries without hiding the first useful failure.

Apply per-client and global resource policies for surfaces, pending imports, callbacks, retained snapshots, and IPC/capture queues. Validate sizes before allocation with checked arithmetic. Fuzz parsers and lifecycle command sequences. Worker failure cannot unwind through the render loop. Shutdown stops accepting commands, revokes capture, terminates supervised children, retires device resources, and releases seat ownership with a bounded deadline.

## Packaging and support

Document dependencies by capability, then provide tested package manifests for chosen initial distributions. Avoid presenting guessed package names as universal install commands. Provide a troubleshooting decision tree: nested launch → GPU/EGL import → seat access → DRM modes → toolkit globals → portal selection → helper logs. Collect driver/kernel/library versions and effective backend state in a redacted report. Offer safe switches to disable blur, direct scanout, explicit sync advertisement, or a specific output only when these switches correspond to implemented and tested fallbacks.

## Production contract refinements

The [integration contracts](16-integration-contracts.md) define actual portal/window-sharing, IME, text UI, accessibility, X11, and service obligations. The [state machines](14-state-machines.md) define lock confirmation, capture revocation, output epochs, and safe rollback. [Release operations](19-release-operations.md) distinguishes helper recovery from compositor crash/session termination.

Never automatically restart an unlocked desktop after a compositor crash and describe that as secure lock recovery. A dead process cannot enforce its old lock state; qualify display-manager/session-manager behavior. Avoid sharing host activation environment in nested mode. Use the selected compatible host-session trust classification and check operations after binding; hiding a global is insufficient revocation.

The [selected model A](21-review-resolutions.md) consistently trusts unsandboxed same-user host Wayland/IPC/config access while restricting tagged sandbox clients. Normal D-Bus/systemd-activated portals and idle-started lockers remain compatible. Capture-on-lock retains sessions with black frames, then resumes only after grant/source revalidation; no terminal protocol session is resurrected. Lock UI is never exported through normal capture.
