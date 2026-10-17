# Nested compositor protocol fixture

`fluxwm-fixture` is an original Rust developer client for the nested FluxWM backend.
It is not a desktop client, a WLCS replacement or a production qualification suite.
It uses the stable xdg-shell client bindings and a nonblocking Wayland connection.
No upstream example source was copied into this fixture.

## Build and run

```sh
cargo build -p fluxwm -p fluxwm-fixture
cargo test -p fluxwm-fixture
cargo clippy -p fluxwm-fixture --all-targets -- -D warnings

# Output paths must be new. The compositor overrides WAYLAND_DISPLAY for its child.
fluxwm --backend nested --socket fluxwm-fixture-test \
  --exit-after-ms 6000 \
  --ready-file /tmp/fluxwm-test-ready.json \
  --stats-file /tmp/fluxwm-test-stats.json \
  --screenshot /tmp/fluxwm-test-frame.ppm \
  -- fluxwm-fixture lifecycle
```

Use absolute executable paths when the binaries are not installed on PATH. The parent
needs access to the host graphical backend; run in a controlled visible nested window
or an explicitly configured virtual X server. A host that suppresses/redraw-throttles
an occluded nested window may affect frame progress, so record the host backend and
visibility alongside the run. A successful client lifecycle cannot prove host physical
presentation timing.

For a compositor already running:

```sh
WAYLAND_DISPLAY=fluxwm-fixture-test fluxwm-fixture lifecycle --timeout-ms 3000
```

The mode accepts a total event/connect deadline of 1–5,000 ms, default 5,000 ms. It reads
the explicitly supplied WAYLAND_DISPLAY; a relative name resolves under XDG_RUNTIME_DIR.
It deliberately ignores inherited WAYLAND_SOCKET so it cannot accidentally use an
inherited host connection descriptor. It does not mutate the process environment.
The parent should set only the intended child socket override and retain the runtime
directory needed to resolve that name.

## Protocol sequence and evidence

1. Connect through a nonblocking Unix socket, discover registry globals and complete
   explicit display-sync barriers. Require wl_compositor, wl_shm, xdg_wm_base and the
   mandatory XRGB8888 SHM format. Bind wm_base to min(advertised version, 7).
2. Construct two xdg_toplevel roles. If a seat exists, request move on the not-yet-mapped
   sibling with serial zero; this should be safely ignored. This is an optional
   regression for server-side serial/object checks, not a valid interactive grab.
3. Commit both initial surfaces without buffers. Acknowledge each configure. For
   toplevel version 5 or later, require an initial wm_capabilities event and require its
   array to be empty for FluxWM's current unsupported-optional-state baseline.
4. Map a 320×240 native-endian XRGB8888 SHM primary with red/green/blue/yellow quadrants.
   Require its first frame callback. Request another callback and commit again;
   require progress beyond the first frame. Only then map a solid-magenta sibling
   and require its frame callback. Primary-first mapping lets the compositor capture
   a deterministic first mapped framebuffer before sibling placement changes it.
5. Attach a null buffer to primary and commit, then sync to establish server request
   processing. Restore role metadata, perform a fresh bufferless initial commit,
   require and acknowledge a new configure, reattach primary's buffer and require
   another frame callback. The unmap/remap handshake follows stable xdg-shell XML's
   xdg_surface mapping description and xdg_toplevel unmapping description.
6. Destroy primary in role→xdg_surface→wl_surface order, destroy its buffer resource
   and wait for a sync barrier. Disconnect while the second window remains mapped.
   Server-side stats must independently prove cleanup after that disconnect.

The client emits one JSON result on stdout and exits nonzero for any failed assertion,
protocol/connection error, missing event or deadline. Evidence includes advertised
registry versions, bound wm_base version, configure acknowledgements and serials,
frame callbacks, empty-capability events, lifecycle milestones, elapsed time and
expected pixel colors. Events and registry evidence have explicit entry limits.
A wl_surface frame callback proves compositor callback progress, not a display
presentation timestamp; server submissions, screenshot content and cleanup counters
are separate required observations.

There is no blocking_dispatch or roundtrip call. The event pump dispatches queued
messages, flushes nonblockingly, prepares a read guard, polls for readable/writable
state with the remaining deadline and reads only on readiness. Deadline checks also
run between dispatch batches. Nonblocking connection setup and silent-server tests
prevent an unavailable or unresponsive server from indefinitely holding the fixture.
The five-second bound is a requested monotonic deadline; arbitrary OS scheduler stalls
are not a hard-real-time wall-clock guarantee.

The fixture does not resize its SHM buffers to compositor-suggested dimensions. It
uses valid client-chosen normal toplevel sizes and tests basic lifecycle only. It does
not request fullscreen/maximize/minimize, validate seat event delivery, test popups,
exercise DMA-BUF/explicit sync or prove configure serial misuse rejection. Occluded
surfaces can legitimately be callback-throttled; qualify this lifecycle under the
specified FluxWM nested layout/visibility setup.

## Observed development result

Initial local probe on 2026-10-17 against the then-built nested compositor failed at
`primary initial frame callback` after a 2,500 ms deadline. It successfully discovered
wm_base version 7, received empty capability arrays, acknowledged initial/configure
updates and committed primary. The optional unmapped-sibling move did not crash the
server. Server stats reported redraws=2, submissions=2, maps=1, frames_sent=0 and
final_windows=0; no screenshot was created. This is a real failed progress/render
observation, not a successful lifecycle claim. Raw temporary evidence was retained
at `/tmp/fluxwm-fixture-run-0o9a0mij` on the development host; those temporary files
are not portable repository artifacts. Re-run against the corrected compositor and
preserve reviewed evidence before claiming the full sequence passes.

Local fixture checks passed: three unit tests (pixel quadrants/native format,
missing socket and silent-server bounded event pump) and clippy with warnings denied.
These unit checks do not substitute for a successful compositor integration run.
