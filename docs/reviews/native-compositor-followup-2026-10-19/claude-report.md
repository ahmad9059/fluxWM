I found no HIGH or MEDIUM defects in the changed code. The restart barrier does what it claims: stale-batch events are rejected, the drain fails closed, and every startup error path I traced ends in an explicit shutdown or a `Driver` drop. This is from source only, in 12 Read/Grep calls on the snapshot, with nothing run.

## The restart barrier

- **Batch stamps:** `system/events.rs:60` records the generation right before each `read`. Every event decoded from that read keeps that stamp, even if a callback raises the generation partway through the batch. The callback at `system.rs:1083` drops any event whose stamp doesn't match the GPU's current generation.
- **Read errors:** these are filtered by the same check, but they can't arrive with an old stamp. Nothing runs between recording the generation and delivering the error, so the stream can't be silently disabled while still marked healthy.
- **Order of operations:** in `system.rs:524-544`, the generation only moves forward after the blocking initial modeset succeeds and the queue has its first frame. Then the drain runs before `gpu.output` is set, so no new cookie (completion ID) can be issued first.
- **Drain failure:** this covers a closed stream, any errno, the 64-read limit, a missing nonblocking flag, or the generation counter running out. The output is invalidated, transport is marked unhealthy, and the output is kept. `finish_output_preview` (`system.rs:644`) doesn't check transport health, so the rollback can still run.
- **Other screen controllers (CRTCs):** a leftover event for a different CRTC can't match, because `frames.rs:371` checks the CRTC before the cookie. So the barrier only needs to hold on the selected CRTC, which is what the blocking modeset plus drain provide.
- **Uncertain initial modeset:** `scanout.rs:227-231` invalidates the queue and skips the drain. With the queue inactive, `submit_with` refuses anything new (`scanout.rs:307`).

## Startup error cleanup

- **Before the guard exists** (`native.rs:338-378`): an early return drops the `Driver`, whose drop runs `stop()` and then `retire`. No output exists yet, so nothing is left behind.
- **Bootstrap failure** (`native.rs:400-414`) and **ping registration failure** (`native.rs:436`): both call `shutdown(state)` explicitly. Because shutdown runs `model.stop()` first, `retire` sees `stopping = true` and does a real blocking restore of the kept output (`system.rs:771-777`). A failed restore is counted and reported through `state.fail`. The later drop of `Handle` doesn't run cleanup a second time.
- **Probe path** (`probe.rs:225`): returning early with `?` drops the `Driver`, which takes the same `stop` → `retire` → restore route.
- **Topology change with a live output** (`system.rs:915-959`): the output is retired without a restore, the cleanup failure is counted, the GPU is not prepared again, and `GpuChanged` is sent. The frontend then fails, with `native.rs:226` as the trigger.

## Remaining defects (both LOW)

1. **`last_failure` gets cleared on an invalidated queue.** `scanout.rs:263` clears it on any completion other than `Ignored`. On an inactive queue, `frames.rs:404-408` still promotes the pending frame and returns `RetiredProgress`.
   - **Trigger:** the initial modeset ends up uncertain, so there's no drain and no generation bump. A leftover event then arrives on the same CRTC carrying a cookie that matches the pending one. `info()` would then show `last_failure: None` and `pending: None`.
   - **Impact today:** none. Both callers read `info` immediately after `begin_output_preview` returns (`native.rs:401`, `probe.rs:229`), and `native.rs` also requires `queue.active`. But the doc comment at `system.rs:469` tells callers to check `last_failure` later, which this breaks.
   - **Fix:** only clear `last_failure` when the completion is `Presented`.
2. **The "blocking commit = quiescence" proof has a kernel caveat.** In the standard kernel atomic helpers, a blocking commit waits for earlier commits with a 10-second timeout. If that wait times out, the commit goes ahead anyway, so `apply_blocking` returning `Ok` doesn't strictly prove an old flip event was already queued. This only matters if the DRM file descriptor is inherited across a restart; a fresh libseat open never receives old events. It belongs in the docs as a stated assumption, not a code fix.

## Other corrections confirmed

- Cursor warnings are limited to one per 10 seconds (`cursor.rs:113`).
- The relative and absolute pointer clamps (`input.rs:201-208`, `241-248`) are guarded against non-finite values. A zero-size output would make the clamp panic, but `scanout::bytes` rejects width or height 0 before any output is mapped.

## Documented scope gaps (not defects)

- The physical hardware path has never been run. Unit tests and nested Xvfb runs can't qualify it.
- There's no recovery from hotplug, VT switch, or seat pause. Seat pause and topology change keep the forgotten outputs, framebuffers, quota permits and seat lease until the process exits (`scanout.rs:404-412`, `system.rs:810-817`).
- After a drain failure or stream error, transport stays unhealthy for good, so no new preview can start on that GPU.

I didn't write the plan file because you asked for no writes. There's nothing to implement beyond the optional one-line fix in item 1.
