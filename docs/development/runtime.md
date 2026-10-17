# Nested runtime milestone

This is an early implementation of the ten-phase plan, not a qualified desktop or a native session. The executable now connects live Wayland role ownership to a checked generational arena, a strip/canvas workspace model, rendering and IPC. Lua parsing remains in a separate helper; compositor config reload, animations, full window states and desktop integrations are not implemented.

## Build and run

```sh
cargo build --workspace --locked
./target/debug/fluxwm --backend nested --exit-after-ms 30000 -- ./target/debug/fluxwm-fixture lifecycle
```

Run from an existing Wayland or X11 graphical session with a real same-UID private `XDG_RUNTIME_DIR` (0700). The host backend is selected by winit; this does not create a DRM/seat session. `--backend` currently accepts only `nested`; no argument also chooses nested. `--help` describes actual options. No client is launched implicitly.

For a longer interactive nested session:

```sh
./target/debug/fluxwm --backend nested --ready-file /new/path/ready.json -- foot
```

Output files must not exist. Readiness JSON is published atomically without replacing another path. Socket and lock paths must be absent, including dangling symlinks; stale endpoints need explicit owner investigation/removal. The upstream Wayland listener still lacks replacement-inode cleanup protection, so same-user path replacement races remain unqualified. `--ready-file` publishes after a dispatch iteration and reports display/socket, PID and backend. `--stats-file` writes counters on clean shutdown. `--screenshot NEW.ppm` exports one compositor-owned first-mapped composition through a worker; it is a development readback feature, not a capture protocol, privacy boundary or permission grant. The nested target is capped at 8192 per dimension and 16,777,216 total pixels. Driver allocation errors fail the owned nested instance with diagnostics.

Children receive this instance's `WAYLAND_DISPLAY` and `FLUXWM_SOCKET`; inherited `WAYLAND_SOCKET` and host `DISPLAY` are removed because the nested server does not implement an X11 route. The parent/service environment is not changed. Explicit child argv is passed without shell splitting. Direct supervised children are killed/reaped on teardown; descendant groups/session autostart ownership remain future work. SIGINT/SIGTERM/SIGCHLD are handled through calloop signal readiness installed before backend/launch threads; these three masks are unblocked before child exec. SIGCHLD only wakes owned-child reaping; no global `waitpid(-1)` reaper is used. Runtime Spawn actions enqueue work in the [bounded launch worker](processes.md); admission success does not mean exec or application readiness. One worker and queues of eight are bounded further by 64 total pending/live worker applications, plus at most one explicit startup command. Spawn replies return a decimal-string `operation_id`; diagnostics retain the latest 64 exec outcomes (PID or bounded error) with that ID. Missing executables are logged and recorded. This history can evict old records, has no subscriptions, and records exec-start results rather than complete process exit/readiness history. Final worker join and child cleanup occur outside event dispatch; kernel-blocked exec/kill/wait have no universal deadline.

## Development controls

These are temporary built-in nested controls, not configured Lua bindings. The host compositor can intercept its own shortcuts first.

| Shortcut | Behavior |
| --- | --- |
| Alt+arrows or h/j/k/l | Focus and minimally reveal a tile/group |
| Alt+Shift+arrows or H/J/K/L | Move/reorder the selected tile/group |
| Alt+c / Alt+s | Canvas / strip mode |
| Alt+plus / Alt+minus | Resize the selected group width |
| Alt+Return | Spawn `foot` with explicit child environment |
| Alt+q | Request graceful close of focused native toplevel |
| Alt+Shift+q | Quit this nested compositor |

Consumed presses retain physical key ownership through release. Runtime keymap reconfiguration, native-device removal, host-focus-loss held-key recovery, shortcuts inhibition and physical keyboard qualification remain open. A client refusing close remains live. Clicking blank workspace keeps keyboard selection. Scene changes refresh a stationary pointer before dispatch completion, axes refresh when needed, and buttons refresh their target after focus/relayout. These source fixes still need a live pointer/scroll/grab regression matrix. Client-initiated interactive moves/resizes are currently ignored for tiled windows; floating mode is unavailable. Existing legacy floating grab code is not qualification of resize transaction correlation.

## IPC

Read the exact IPC path from readiness JSON; there is no ambiguous auto-discovery.

```sh
./target/debug/fluxctl --socket /exact/instance.sock query capabilities
./target/debug/fluxctl --socket /exact/instance.sock query windows.list
./target/debug/fluxctl --socket /exact/instance.sock action '{"name":"workspace-mode","mode":"canvas"}'
```

Supported methods: version, capabilities, windows/workspaces/outputs queries, diagnostics and the supported action subset. Configuration check/reload/effective queries return `not_ready`; use `fluxwm-config --check FILE` separately. Secure lock, fullscreen, minimize/maximize, floating and overview are not reported as implemented. Unknown methods or invalid action fields are rejected. Capabilities explicitly report subscriptions, native session, capture and X11 unavailable.

The optional calloop IPC driver registers owned CLOEXEC descriptor duplicates, buffered-work pings and exact timers. It retains weak event-loop ownership; no recurring idle polling is required. The handler snapshots or mutates the authoritative model, then returns its exact sequence. Window IDs remain stable across null-buffer unmap/remap and become stale on role destruction/slot reuse. One workspace and one virtual output are permanent for this instance. Current queries bound metadata strings; pagination and multi-workspace/output ownership are future requirements.

## Rendering and lifecycle evidence boundaries

A role becomes a layout member when a committed root buffer exists; null attachment removes that membership while preserving role/ID. The next mapping cycle receives its initial configure after a subsequent bufferless commit, rather than prematurely during unmap. Dead roles are removed before IPC snapshots. Optional XDG capability lists are empty until actual behavior exists. Popup grabs use the selected Smithay manager, and a new grab requires an active same-client pointer press before changing manager or seat state. Invalid/absent authorization dismisses the request. Keyboard/touch initiated popup grabs are not implemented; nested chains and dismissal/input ordering still require explicit qualification.

The renderer composes to a persistent offscreen GLES texture with tracked damage. A one-shot virtual 60 Hz deadline is armed only for pending work; static scenes do not create a perpetual redraw loop. Host EGL blits/submissions occur on host redraw opportunities, so occlusion-throttled host frames do not force every virtual client frame to swap the host window. Wayland output events flush after dispatch independently of rendering. The pinned backend hides per-client flush errors in its aggregate API, so this runtime collects client IDs outside the backend lock and flushes them individually. A reported `WouldBlock` arms at most one 16 ms retry; retries stop after output drains, with no recurring idle timer. Diagnostics expose request/flush/blocked/retry counters. This is bounded scheduling per retry, not a per-client queued-byte cap or hostile-client resource qualification. Exposed output transform is normal; the framebuffer correction belongs to rendering/readback. Zero-size host events suspend rendering and retain state, but a synthetic resize/minimize/failure matrix is still required.

`redraws` count virtual render dispatches; `compositions` count changed offscreen frames; `submissions` count host EGL submissions. Client frame callbacks follow virtual composition opportunities. They do not prove host/physical presentation. Readback quadrant assertions prove content and orientation in the owned composition, not hardware timing or native output behavior. The renderer still needs buffer-consumer tracing, failure injection and the full B04/B10 qualification.

Run [the nested smoke harness](nested-smoke.md) for bounded lifecycle, pixels, idle counters and shutdown/socket cleanup. Its historical first pass and later integrated results identify their exact binary/source artifacts. Unit tests, this fixture and IPC loop tests cover specific paths; no entire phase or full catalog case is promoted automatically. The original [implementation audit](../24-implementation-readiness-audit.md) and [Claude report](../reviews/claude-implementation.md) refer to earlier source snapshots; they do not independently review these later changes.

Remaining production work includes mapped/visible immutable scene picking, container clipping with popup separation, configure/commit transactions, full activation and window-state policy, multi-output/native lifecycle, Lua atomic application, motion/gestures, protocol conformance, desktop/lock/capture/X11/IME/accessibility, resource/fuzz/soak and clean packaging/support-matrix evidence.

See [runtime findings and remaining production gates](runtime-findings.md) for source-confirmed fixes, exact evidence boundaries and the still-open acknowledged-shutdown contract.
