# Stationary pointer feedback regression fixture

`fluxwm-fixture pointer-feedback` is a bounded real Wayland client that reproduces repeated stationary `wl_pointer.motion` events after a client commit. This checks the independent-review finding that generic scene/redraw reevaluation called `pointer.motion` even when neither the target surface nor its origin changed. A client that commits in response can amplify that into a request/event feedback loop.

The pinned Smithay revision `023b78b1c5a05ca512767cacc07efca55fae8be4`, `src/input/pointer/mod.rs:800–832`, provides direct source evidence: `PointerInternal::motion` calls the focused target's `motion` method when the focus remains the same. It does not suppress identical coordinates. The compositor therefore needs to distinguish a real motion or changed scene target/origin from a generic redraw request. Correct suppression must retain enter/leave when the target changes, and local-coordinate motion when scene movement changes the target origin under a stationary physical pointer.

## Run and expected behavior

```sh
cargo build -p fluxwm -p fluxwm-fixture
target/debug/fluxwm --backend nested --socket fluxpointer-UNIQUE \
  --exit-after-ms 1500 --stats-file /tmp/NEW-pointer-stats.json \
  -- target/debug/fluxwm-fixture pointer-feedback --timeout-ms 4500
```

Use a new socket and stats path each time. Run without interacting with the host pointer while this nested window is present. The compositor supplies `WAYLAND_DISPLAY` and `FLUXWM_SOCKET`; the fixture does not inject physical input, change a host pointer position, or connect to an inferred parent display. Its own absolute timeout defaults to five seconds and accepts 1–5,000 milliseconds. The compositor's exit timer is a separate bound; increase it if the environment cannot initialize the nested output within the example's 1.5 seconds.

The fixture binds `wl_seat`/`wl_pointer` before mapping its first 320×240 SHM toplevel at the layout origin. It waits for a real pointer enter and frame callback, observes a 200 millisecond settling interval, then requires the recorded pointer-local coordinate to be exactly `(0,0)` on that primary surface. A missing enter or a different initial position fails the prerequisites; it does not become a passing stationary test.

After settling it captures runtime diagnostics and pointer counters, enables commit-on-motion, and sends exactly one seed damage/commit. For the next 400 milliseconds, each motion on the primary surface generates another one-pixel damage/commit, up to 32 feedback commits. This cap bounds amplification against an unfixed compositor; exhausting the cap cannot become success. The fixture compares identical-coordinate motions separately from genuine coordinate changes, enters and leaves.

Success requires at most one additional stationary motion after the seed, with no changed-coordinate motion and no enter/leave change during the observation. Real movement or focus change marks the run contaminated and fails it. The one-event allowance tolerates one relevant initial refresh without accepting a sustained cycle. The result records the actual count rather than merely reporting whether a timer elapsed.

The mode emits one JSON result containing prerequisite coordinates, counters before/after, event evidence, feedback commits, observed duration, redraw/composition deltas and a typed-in-text failure explanation. It binds no extra input devices and sends no keys/buttons. It reads only bounded diagnostics through the existing 64-KiB/deadline-controlled IPC helper. The retained event array has the existing 1,024-entry ceiling; this case normally records only setup/configure/enter/frame events, while motion counts remain numeric counters.

Redraw and composition deltas are diagnostic evidence, not a promise that each spurious pointer event causes a GPU composition. In the recorded failure, many extra pointer events caused extra Wayland requests while the renderer avoided additional scene composition. This is still an observable protocol/input feedback defect.

## Actual before/after evidence

On 2026-10-17 the same fixture executable ran before and after the root compositor correction. The tested fixture SHA-256 was unchanged:

`217a86f9d873472bacf41f744b85839312d248b514e52409c0b23f230c332399`

| Observation | Before correction | After correction |
| --- | --- | --- |
| Root binary SHA-256 | `562d4a444ebcf684f86fa3659d912e36cc1c5189ef739b8d576e456363caf115` | `7aaf606110519231069b8883e2a40fe0f271864c03e3f68fd908cf77a3134b01` |
| Initial primary pointer coordinate | `(0,0)` | `(0,0)` |
| Settling / observation intervals | 200 / 400 ms | 200 / 400 ms |
| Baseline stationary motions | 0 | 0 |
| Additional stationary motions | **33** | **0** |
| Changed-coordinate motions during observation | 0 | 0 |
| Generated feedback commits | 32, fixture cap reached | 0 |
| Redraw delta | 2 | 1 |
| Composition delta | 0 | 0 |
| Wayland request counter before → after | 24 → 90 | 24 → 26 |
| Fixture result | Failed: stationary feedback exceeds one event | Passed |
| Compositor exit | 1, supervised fixture failure propagated | 0 |

The failing fixture completed its observation in 621 milliseconds; the passing fixture completed in 620 milliseconds. Both included a real pointer-enter event on the primary surface and callback progress. The extra 64 requests before correction correspond to the bounded 32 damage/commit pairs; the passing case generated only the seed pair.

Raw local evidence is retained in these directories, each with command/binary metadata, fixture stdout, compositor stderr, readiness and final stats:

- Before: `/home/ahmad/Documents/Projects/fluxwm-layout/target/pointer-feedback/20975a9327ce4b09ac47c7127807cee7/`
- After: `/home/ahmad/Documents/Projects/fluxwm-layout/target/pointer-feedback/ef4bc75db6e549509173fb6a0c0e0749/`

An earlier development run failed because the fixture's `poll_fd` treated a rounded-down millisecond poll timeout as an expired absolute deadline. That run is retained separately at `target/pointer-feedback/bcd74ed0954147c39ee87d69a7f3e701/`; it is not compositor regression evidence. The helper now rechecks its absolute monotonic deadline after a zero-return poll. The same corrected fixture produced both results in the table.

These are observations of the specified in-progress debug binaries, not qualification of a committed release. The result proves the stationary same-target case on this controlled nested instance. It does not qualify actual physical pointer routing, animated or scaled input transforms, pointer constraints, popup/drag/lock ownership, target-origin changes, touch/tablet behavior, native hardware, or host presentation timing. Those require distinct regression cases. A final integrated revision should rerun this mode instead of relabeling these historical binary results.

## Local checks

`cargo test -p fluxwm-fixture` passes six tests, including a fixed-duration idle observation against a silent socketpair that must finish normally after its interval rather than fail early. Existing deadline, diagnostic response bound and SHM pattern tests remain passing. `cargo clippy -p fluxwm-fixture --all-targets -- -D warnings` passes. The original `lifecycle` and `slow-reader` modes remain available; this mode binds the pointer only when explicitly selected.
