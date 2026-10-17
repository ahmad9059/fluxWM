# Nested scene regression smoke

Run the real Wayland fixture against a nested compositor inside an existing graphical session:

```sh
cargo build --locked --bin fluxwm --bin fluxwm-fixture
python3 tools/smoke_scene.py
```

The binaries must include the `popup-lifecycle`, `pointer-feedback`, `container-clip` and `popup-clip` fixture modes and the compositor's delayed screenshot option. Missing or older prerequisites fail the suite. This suite requires an existing `DISPLAY` or `WAYLAND_DISPLAY`, and an absolute, current-user-owned `XDG_RUNTIME_DIR` with mode `0700`. It does not start a host display server. Generic headless CI does not run these checks.

To test an already built pair without rebuilding either executable:

```sh
python3 tools/smoke_scene.py \
  --binary /absolute/path/to/fluxwm \
  --fixture /absolute/path/to/fluxwm-fixture \
  --artifacts /absolute/path/to/new-evidence-directory \
  --timeout 45
```

Do not rebuild or replace these binaries during the run. The summary records their paths and SHA-256 hashes before and after all cases; a hash change fails the run. This verifies executable stability across the run, not correspondence with a particular source commit. Record the source revision separately when publishing results.

Each case launches its own nested compositor and fixture with a random socket name. The harness requires atomically published readiness with the launched PID, selected socket, nested backend, `dispatch-ready` status and expected IPC path. It checks that the Wayland socket, lock and IPC socket disappear after normal exit, and that no managed windows remain after fixture disconnect. Parent environment variables are compared after the run; child-only `RUST_LOG` configuration does not modify the parent environment.

The four cases exercise distinct regressions:

- `popup-lifecycle`: a mapped non-grabbing popup receives exactly one `popup_done` after root null-buffer unmap; an invalid zero grab serial is dismissed without stealing keyboard focus; a mapped popup is dismissed when the parent toplevel role is destroyed. Frame callbacks prove callback progress for the mapped popup cases.
- `pointer-feedback`: a real pointer-focused surface commits damage after each received motion. During the fixture's stationary 400 ms observation, at most one additional stationary motion and feedback commit are allowed. Changed coordinates fail the test. This exposes compositor-generated motion/commit loops without physical input injection.
- `container-clip`: a 1900×240 SHM buffer remains unchanged while its canvas container is resized to width 280. The compositor captures its scene framebuffer after at least 700 ms; the fixture waits 1000 ms before changing the scene for its input check. The screenshot must contain red/blue rectangles of 19,200 pixels each, green/yellow rectangles of 14,400 pixels each, no cyan pixels, and background RGB `(15,18,23)` from x=280 through the right edge in the first 240 rows. The harness independently compares every RGB row, including the clear area below the window, to the controlled scene. It requires dimensions of at least 320×240 and accepts varying host output dimensions. The later input stage requires an actual child enter at `(64,0)`, followed by a leave after moving the container. The stationary pointer remains inside the child surface's own geometry but outside the container; no subsequent child motion is allowed. A fixture `ok` result alone cannot satisfy the pixel assertion.


- `popup-clip`: the root container is moved to x=32 with width 280. An 80×60 non-grabbing magenta popup is configured at parent-relative `(-32,0)`, giving world geometry `(0,0,80,60)`. The fixture requires actual root enter before the move, pointer focus outside the moved root, then popup enter at local `(0,0)` and a popup frame callback. The popup remains mapped and stable for 1000 ms. The delayed screenshot must show all 4800 magenta pixels, including the 32-pixel strip outside the root container, and magenta above the root where the two overlap. The root remains independently clipped at x=312. Exact row equality requires red 16,320, green/yellow 14,400 each, blue 19,200, cyan zero, and background RGB `(15,18,23)` outside these controlled regions. The harness checks actual configure geometry and configure→ack→map event order; frame and enter must follow mapping, with either order allowed between them. It does not infer popup input eligibility from image colors.

Both image validators report `pixel_validation: "exact-row-equality"` with `expected_rgb_counts`. These counts describe the exact expected image checked by row equality; they are not separately sampled measurements. Screenshot dimensions come from the bounded PPM header, since an initial output-geometry event can precede a later host resize.

The default whole-suite deadline is 45 seconds (`--timeout` accepts 20–120). Each compositor has a 6000 ms lifetime, each fixture a 4500 ms deadline, and each harness case an 11 second ceiling capped by the remaining suite time. Evidence processing also checks the suite deadline. Terminating and reaping a failed child has a separate bounded two-second allowance; filesystem operations and executable hashing rely on a responsive local filesystem. The harness monitors each stdout/stderr log at 4 MiB, JSON files at 1 MiB and screenshots at 64 MiB while the process runs. Final reads enforce these bounds again and reject symlinks and non-regular evidence files. These are sampled limits, not operating-system write quotas: a child can overshoot between polls before termination.

Failures preserve evidence. The harness kills its private process group and reaps its child; endpoint cleanup removes only inodes observed while that case was running. It does not remove the shared IPC directory or unrelated sockets. The unique artifact directory contains commands, logs, atomic readiness, runtime stats, fixture JSON, per-case results, the container PPM and `summary.json`. Exit status is nonzero if any case, cleanup assertion or provenance check fails. A run with no fixture is never reported as a pass.

These checks cover synthetic SHM scene composition, selected popup lifecycle paths and stationary pointer behavior in the nested backend. The screenshot is a compositor scene framebuffer copy; it does not prove host presentation, physical input, native DRM/KMS behavior, arbitrary subsurface trees or complete popup/input protocol conformance. Popup depth, nested popup ownership, overlap with a higher unrelated owner, and wholly offscreen owners remain outside this suite. ClickGrab release after scene changes and after held pointer motion still needs a test that injects actual backend button events; a Wayland fixture cannot generate `wl_pointer` button events itself.

## Recorded local evidence

On 2026-10-17, all three cases passed against the frozen root binaries from runtime commit `6a5473d` and fixture commit `5aed134`. The retained local report is `target/smoke-scene/3eaeae767da84ff8a837846beb639de3/summary.json` in the scene-smoke worktree. Both executable hashes remained unchanged:

- `fluxwm`: `95bc4fd533c59125a1412b5520302f12f7d686eef5fa4d9a6efb517fd2b91e7d`
- `fluxwm-fixture`: `f454ab3fa019c11fcd0a8ca83c4780b3ccb42b611b9a1fd5bac8652910e57c70`

The 1270×1396 screenshot matched every expected row and exact color count. All three cases reported no leaked Wayland socket, lock or IPC endpoint, and the parent environment remained unchanged. Separate harness checks accepted a synthetic valid PPM and rejected a one-pixel cyan leak, truncated payload, symlink evidence and missing executable prerequisite. These local results do not establish results on other graphics drivers or hosts.

On the same date, the extended four-case suite passed against runtime commit `f637c21` and fixture commit `b0d7bde`. Report: `target/smoke-scene/a64b7359d48a4f0bbda4146c0e882e44/summary.json` in the scene-smoke worktree. All endpoint cleanup and environment checks passed, with unchanged executable hashes:

- `fluxwm`: `ee3e1574d17eb0db9d529228e6f9b897ac61434411d770bb650ce8f3142b6e51`
- `fluxwm-fixture`: `4976917e725122fbb09981389f64133a056afea7dcb86fa1e5086122191db408`

Both 1270×1396 images passed independent exact-row validation. The popup case also supplied real configure/ack/map/frame/enter evidence at the required outside-owner point. Synthetic one-pixel popup clipping, wrong paint order and cyan overflow images were rejected. Five altered popup JSON records with wrong geometry, pointer entry, callback progress, premature dismissal or missing pointer precondition were rejected. This run does not exercise the ClickGrab fix because these scene fixtures inject no backend button events.
