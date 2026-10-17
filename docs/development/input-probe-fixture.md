# Bounded live pointer event probe

`fluxwm-fixture input-probe` supplies two ordinary Wayland clients surfaces and a live event journal for an external, isolated input regression harness. It does not inject input or decide whether compositor grab/focus delivery is correct. The harness generates controlled events against its private graphical server and checks the recorded client-visible sequence.

## Run contract

```sh
FLUXWM_FIXTURE_EVENT_FILE=/tmp/NEW-input-events.jsonl \
  target/debug/fluxwm --backend nested --socket fluxinput-UNIQUE \
    --exit-after-ms 9000 --stats-file /tmp/NEW-input-stats.json \
    -- target/debug/fluxwm-fixture input-probe --timeout-ms 10000
```

Every output path and socket must be unique. The event-file parent directory must exist. The `FLUXWM_FIXTURE_EVENT_FILE` variable is required explicitly; this mode never chooses a host log path or silently reuses a file. The command prefix changes only that command's environment. The compositor supplies the actual nested display to its startup child.

The probe creates root index 0 titled exactly `input-A` (solid red) and root index 1 titled exactly `input-B` (solid green), both app ID `org.fluxwm.InputProbe`, with 320×240 SHM buffers and minimum size 320×240. It binds `wl_pointer` before mapping. A `ready` record is emitted only after both roots acknowledge configure and receive frame callbacks. External IPC can resolve the root identities by title; indices in this probe are client-local and are not compositor generational IDs.

The external harness requested a fixed **7,000 millisecond observation window after ready** to accommodate software-rendered CI and event barriers. `input-probe` therefore has a mode-specific default/maximum absolute timeout of 10,000 milliseconds; the other live fixture modes retain their existing 5,000 millisecond limit. A timeout too short to contain setup plus the full observation interval fails explicitly. The compositor must have its own bounded supervision/exit timer long enough for the probe to finish. The roots remain mapped throughout observation and are released when the probe finishes and disconnects.

The probe does not commit on pointer motion, create popups, or alter scene layout during observation. Its two fixed SHM buffers total 614,400 bytes. The external harness may change the scene through supported compositor IPC while holding a real input button; pointer records then reveal what the clients actually received.

## JSONL journal

The event file is opened once with `create_new`, no-follow/close-on-exec flags and mode 0600; existing files and symlinks are rejected. Each event is serialized, followed by a newline and flushed before recording continues. A reader must consume complete newline-terminated JSON records and retain any incomplete tail until the next read. Regular file flush provides live visibility; this is not a durability/fsync claim.

The wrapper is:

```json
{"at_ms":123,"event":{"type":"pointer.button","root":0,"button":272,"state":"pressed","serial":18,"time":534}}
```

`at_ms` is monotonic time since this fixture started. Protocol `time` and serial values retain their original unsigned protocol values; they use a separate clock/sequence domain. Root indices are 0 or 1, or null when no root is selected. For leave, the root is resolved from the leaving event's surface; for motion/button/axis/frame it is the current protocol-entered surface.

| `event.type` | Additional fields |
| --- | --- |
| `ready` | `root:null`, `roots:[{"root":0,"title":"input-A"},{"root":1,"title":"input-B"}]`, `observation_ms:7000` |
| `pointer.enter` | `root`, `serial`, surface-local `x`, `y` |
| `pointer.leave` | leaving `root`, `serial` |
| `pointer.motion` | current `root`, surface-local `x`, `y`, protocol `time` |
| `pointer.button` | current `root`, Linux `button` code, `state:"pressed"` or `"released"`, `serial`, `time` |
| `pointer.axis` | current `root`, `axis:"vertical"` or `"horizontal"`, `value`, `time` |
| `pointer.axis_stop` | current `root`, `axis`, `time` |
| `pointer.frame` | current `root` |
| `complete` | `root:null`, `mapped_through_observation:true` |

The stream also retains existing setup records such as `create`, `configure_ack` and `frame_done`, which do not have `event.type`. Filter with an optional field lookup rather than assuming every record is an input event. Unknown protocol button states fail the probe; unknown axis enum values are recorded as `"unknown"` for external diagnosis rather than assigned a valid direction.

The same records are retained in the terminal stdout JSON's `events` array. All other fixture modes still write only their one terminal stdout result and do not open this event file, even if its environment variable is present. The stream is opt-in for this mode only.

## Bounds, completion and interpretation

There are at most 1,024 retained events and one MiB of serialized journal bytes. Exceeding either budget or failing a write/flush fails the fixture. These limits prevent the probe itself from growing indefinitely under a broken compositor or excessive input. A harness should apply its own total wall-clock deadline, child-process cleanup and bounded reads as well.

On normal completion, stdout contains one record with `mode:"input-probe"`, `ok:true`, `observation_ms:7000`, `ready_at_ms`, `elapsed_ms`, `mapped_through_observation:true`, `event_file`, `event_file_bytes`, and the full event array. It means initialization, observation and evidence writing completed. It does **not** mean implicit-grab restoration, button ownership, axis routing, keyboard focus, popup behavior or physical input latency passed. The external harness must assert those outcomes from the event sequence and its own command/input trace.

For an implicit-grab regression, the harness can require a press on root 0, move the scene under the held pointer, require release delivery still to root 0, then require leave/enter restoration and subsequent wheel-axis delivery to the newly eligible root. It must distinguish a missing event from a passing absence using its own bounded barriers. `close-window` is not a suitable scene-change trigger for this probe: the common fixture handler treats unexpected client-close requests as failures; use supported pan/move/focus actions instead.

The intended external input harness uses a private Xvfb/XTest server and child-only display environment; this fixture does not create that server or authorize injecting events into a host session. Input generation and before/after runtime evidence are separate harness responsibilities.

## Local validation

`cargo test -p fluxwm-fixture` passes nine tests. The new journal test reads the live file after each appended record, verifies complete JSONL matches retained events and checks 0600 permissions. Existing parser/deadline, pattern and pixel-checker tests remain passing. `cargo clippy -p fluxwm-fixture --all-targets -- -D warnings` passes.

The executable was supplied to the separate input-harness worker for real before/after checks against the preserved pre-fix runtime and the corrected runtime. This document does not claim those external checks passed until their own command trace and event evidence are recorded. Unit checks cannot substitute for actual event delivery.
