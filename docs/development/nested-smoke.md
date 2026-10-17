# Nested compositor integration smoke harness

`tools/smoke_nested.py` runs the real `fluxwm` executable and original Rust `fluxwm-fixture` client using only Python's standard library. It is a developer qualification tool for the current nested implementation, not native DRM/KMS, real hardware, desktop-completeness, or host-presentation qualification.

## Build and run

The parent graphical session must already be available through `WAYLAND_DISPLAY` or `DISPLAY`, with a valid owned `XDG_RUNTIME_DIR`. The script does not start a graphical server, modify host display/service environment, install packages, compile automatically, or select a different GPU driver.

```sh
cargo build -p fluxwm -p fluxwm-fixture
python3 tools/smoke_nested.py
```

An isolated worktree can test the main worktree's executables explicitly:

```sh
python3 tools/smoke_nested.py \
  --binary /home/ahmad/Documents/Projects/fluxwm/target/debug/fluxwm \
  --fixture /home/ahmad/Documents/Projects/fluxwm/target/debug/fluxwm-fixture
```

Optional arguments: `--artifacts NEW_DIRECTORY` selects a directory that must not already exist; `--timeout SECONDS` bounds the whole scenario loop (default 40, allowed 10–120). Cleanup has an additional two-second forced-reap bound. The compositor fixture itself has a 4.5-second deadline. Lifecycle process time is at most twelve seconds, other scenarios at most eight seconds, additionally capped by the remaining whole-suite budget. A scenario failure does not hide independent later failures.

## Assertions and their limits

The lifecycle case launches the compositor with a unique socket, six-second exit timer, readiness/stats outputs, and a first-mapped-frame screenshot. It spawns `fluxwm-fixture lifecycle --timeout-ms 4500` through the compositor's explicit `-- COMMAND` support, so the compositor supplies the actual nested display to the fixture without changing the host environment.

The fixture creates two toplevels, exercises an unmapped move with an invalid serial, acknowledges initial configures, maps its primary and requests callback progress twice, maps a sibling, null-buffer unmaps the primary, obtains a fresh remap configure, maps again, explicitly destroys the primary, and disconnects with the sibling still mapped. The harness requires one successful lifecycle JSON record, three primary callbacks and one sibling callback at minimum, actual attempted unmapped move, and server counters `maps=3`, `unmaps=1`, `destroyed=2`, `final_windows=0`. A fixture `ok` field alone cannot establish those server cleanup counters.

The P6 validator reads the compositor's own copied GLES framebuffer, validates dimensions/header/exact byte length, and locates the original fixture's four RGB colors independently of the window's screen position. It requires exactly 19,200 pixels per color, a 160×120 rectangular bounding box per quadrant, red/green above blue/yellow, and green/yellow to the right of red/blue. This detects channel swaps, vertical/horizontal flips, missing quadrants, and distorted/occluded pixels. The expected pattern is the fixture's unscaled 320×240 SHM content. Changing fixture geometry, scaling, effects, or screenshot policy requires updating the contract explicitly. Do not relax color/orientation assertions just to accept a renderer failure.

The framebuffer copy is compositor-side evidence. It does not prove the host displayed that frame, GPU fences retired correctly, scanout worked, color management is correct, or a screenshot portal is implemented.

Two separate empty-compositor runs last 600 and 1,600 milliseconds. Both require initial drawing/submission, zero maps and final windows, at most two extra long-run redraws/submissions, and at most six long-run redraws/submissions in total. This rejects a frame-rate busy redraw loop while allowing a small startup/exposure difference. It is a bounded idle smoke comparison, not proof against arbitrarily slow periodic work or a physical power/latency benchmark. Raw counters remain in the evidence for stricter diagnosis.

The signal case waits for a matching `dispatch-ready` record with the child PID and unique socket, verifies the socket exists, sends SIGTERM to that exact subprocess, and requires clean exit and socket/lock removal. Readiness means dispatch readiness; it is not an asserted first host-visible frame. Each successful scenario requires removal of the Wayland socket and `.lock`, and checks the complete harness-process environment remains unchanged. Parent `DISPLAY`, `WAYLAND_DISPLAY`, `WAYLAND_SOCKET`, runtime path, and optional backend selection are included as allowlisted metadata. No complete environment dump is written.

## Evidence and failure handling

Default evidence lives under `target/smoke-nested/<unique-run-id>/` and is retained for passing and failing runs. Each case writes command/environment metadata, stdout/stderr logs, readiness, stats when available, and a result with timing/exit/error/cleanup state. Lifecycle success also writes decoded fixture JSON and screenshot metadata. The suite writes `summary.json` containing binary hashes, every case outcome, idle comparison when both idle cases passed, and the overall result. A nonzero script exit means failure; missing or malformed evidence does not become a skip/pass.

Logs are monitored under a four-MiB per-stream budget; JSON evidence is bounded to one MiB and screenshot reading to 64 MiB with bounded dimensions. Screenshot validation checks the suite deadline while processing pixels. A timed-out/oversized-log child and its supervised descendants are killed only in the new process group created for that scenario, then reaped with a bounded wait. If a failed process leaks runtime files, the harness records the leak as a failure and removes only matching device/inode pairs it observed while its own unique child was live. It never removes arbitrary host sockets or conceals a cleanup failure by counting harness removal as compositor success.

Current test coverage excludes IPC commands until the runtime wire/API is settled, native sessions, hotplug, transformed physical outputs, real input injection, fullscreen/backdrop behavior, capture permissions, animation timing, and accessibility. Those remain separate production gates.

## Recorded first execution: failures preserved

On 2026-10-17 the harness ran against the root worktree's then-current debug binaries (root source still in progress). Evidence: `/home/ahmad/Documents/Projects/fluxwm-layout/target/smoke-nested/b7f9675b7a9f41948bfcad410fa5bcbd/`. Binary hashes and allowlisted environment are in that run's `summary.json` and per-case metadata. These are local, ignored artifacts, not a committed portable qualification bundle.

Observed results:

- Lifecycle: first toplevel mapped, then GLES framebuffer readback failed with a context-lost/EGL allocation error. The compositor exited 1; no successful fixture/pixel assertion is claimed.
- Idle 600 milliseconds: compositor exited 0, raw counters showed two redraws and two submissions, but its socket and lock remained, so the case failed cleanup.
- Idle 1,600 milliseconds: compositor exited 0 with the same two redraws/two submissions, and again leaked socket/lock. This is evidence of bounded idle work during those runs, not a passing overall case.
- SIGTERM: matching readiness was observed, SIGTERM was sent, compositor exited 0 with one redraw/submission, but socket/lock remained. The case failed cleanup.

All four observed parent environments remained unchanged. The harness recorded runtime leaks before its own cleanup. Root implementation fixes and a full rerun are required before changing these results to passing. Independent code review cannot substitute for the failed runtime evidence.
