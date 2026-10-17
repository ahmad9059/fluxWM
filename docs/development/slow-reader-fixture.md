# Real Wayland slow-reader fixture

`fluxwm-fixture slow-reader` exercises actual server-to-client output backpressure against the nested compositor. It requires the runtime IPC diagnostics counters `wayland_requests`, `flush_cycles`, `flush_would_block`, and `flush_retry_timer_fired`. A client merely sleeping before reading is insufficient evidence of EAGAIN; this mode explicitly observes the server's per-client flush returning WouldBlock before generating its final marker.

## Run

```sh
cargo build -p fluxwm -p fluxwm-fixture
fluxwm --backend nested --socket fluxslow-example \
  --exit-after-ms 6500 --stats-file /tmp/fluxslow-new-stats.json \
  -- target/debug/fluxwm-fixture slow-reader --timeout-ms 5000
```

Use a new socket name and nonexistent stats path on every invocation. The example assumes the current shell resolves the built `fluxwm`; use `target/debug/fluxwm` directly if it is not on PATH. The compositor supplies its child `WAYLAND_DISPLAY` and `FLUXWM_SOCKET`. Run in a new isolated nested instance with no other Wayland clients; global request-counter equality intentionally fails if another client generates requests concurrently. An existing parent graphical display is required for the compositor, while this fixture itself only uses its explicitly supplied nested socket.

When testing independent worktrees, pass the absolute path to this worktree's built fixture after `--` and use the root worktree's already-built compositor. The fixture does not compile, modify, or terminate a parent compositor. Its timeout range is 1–5,000 milliseconds, default 5,000; the external compositor process also needs its own exit timer or supervisor deadline.

## Wire procedure and bounds

1. Connect to Wayland without binding any globals, creating surfaces, or reading events. Connect once to the separate persistent IPC diagnostics socket. Record baseline counters.
2. Queue exactly 32 `wl_display.sync` requests and flush their request bytes without reading Wayland input. Each callback generates a done event and a display delete-id event, totaling 24 reply bytes per callback in the pinned protocol path.
3. Query diagnostics until the request counter includes the batch. Then require a later flush-cycle count before proceeding. This two-stage barrier avoids assuming IPC and Wayland event sources dispatch in a particular order. A single diagnostics response can otherwise observe requests before their flush pass.
4. Repeat only while the server's WouldBlock counter remains at baseline. Stop immediately after the first proven blocked pass. A batch holds at most 768 bytes of new pending replies; do not keep flooding after saturation.
5. Generate one additional final-marker sync, flush it, and observe through IPC that the marker request was processed. This marker is generated after proven kernel backpressure. No further Wayland or IPC requests are generated until after its callback is received.
6. Delay all Wayland reading for 200 milliseconds, then drain callbacks. The normal event reader's empty flush operation does not generate requests; sync callback handling does not acknowledge/send anything. Require all callbacks through the final marker, with the exact expected count.
7. Only after marker receipt, make one final IPC diagnostic query. Require the Wayland request count to remain exactly baseline plus generated requests and observe actual retry-timer execution. This final query cannot cause an already received marker to arrive.

The mode caps workload at 512 batches plus one marker: at most 16,385 callback objects and 196,620 Wayland request bytes. It stores only a fixed set of diagnostic snapshots rather than one record per callback/batch. Each diagnostic response is bounded to 65,536 bytes, all socket reads/writes/connects are nonblocking and deadline-controlled, and the no-read delay is attempted only when at least 300 milliseconds remain. If saturation cannot be demonstrated before the batch/deadline limit, the test fails rather than claiming backpressure coverage.

The pinned `wayland-backend` 0.3.17 Rust backend has a default 4,096-byte output-buffer maximum. Its `rs/socket.rs` write path flushes a full buffer and can return E2BIG if it remains full; the server can disconnect that client. A giant unpaced sync flood would test overflow/disconnection rather than the desired pending-output recovery. The bounded batch/diagnostic barrier keeps unflushed user-space responses below that buffer ceiling in this isolated sync-only case. It does not impose compositor-wide resource quotas, measure exact allocator high-water usage, or establish safety for arbitrary clients/protocol event sizes. Kernel socket capacity is measured indirectly by the observed saturation point, not assumed from socket buffer defaults or client-side SO_RCVBUF.

The final output is one JSON record with `ok`, error/deadline information, baseline/blocked/marker/final counter snapshots, number of batches, marker identity, callback count, delay and drain timing. `ok=true` requires actual server WouldBlock, marker request processing, complete marker delivery, unchanged post-marker Wayland request count, and a fired retry timer. The mode does not render pixels, create window state, qualify native hardware, or prove that every possible protocol output path handles backpressure.

## Recorded live result

On 2026-10-17 this fixture ran against the root worktree's in-progress compositor with the new telemetry/retry path. Local evidence remains at `/home/ahmad/Documents/Projects/fluxwm-layout/target/slow-reader/22a67107e5e746799d478b952f0dc0a6/` (command, fixture stdout, compositor stderr, readiness and final stats). This is a developer observation for that run, not a committed release qualification.

The fixture returned `ok=true`; the compositor exited 0. Server backpressure occurred after 94 batches/3,008 sync requests: blocked snapshot showed 2 WouldBlock returns. The final marker was request/callback 3,009, processed while the counter had increased to 5 WouldBlock returns. After 200 milliseconds without Wayland reading or new requests, all 3,009 callback events arrived; draining took 9 milliseconds. Final diagnostics showed exactly 3,009 Wayland requests, 42 WouldBlock returns, and 13 retry timer firings. The fixture completed in 288 milliseconds, below its five-second deadline. The compositor's final output agreed on the 3,009 request count and 13 timer firings.

These observations establish real kernel-pressure recovery without further requests in the controlled instance. They do not establish a continuous idle timer: idle behavior must still be tested separately after pending output has drained. The existing nested smoke harness provides a separate bounded idle comparison.

## Checks

`cargo test -p fluxwm-fixture` passes five tests, including bounded failure against a silent Wayland peer, bounded failure against a silent diagnostics peer, oversized diagnostic response rejection before payload allocation, missing-socket handling and original SHM quadrant bytes. `cargo clippy -p fluxwm-fixture --all-targets -- -D warnings` passes. Those unit checks support deadline/parser behavior; they do not replace the recorded live WouldBlock/marker test.

The original `lifecycle` mode is retained. Invalid popup serial/background-client coverage is a separate requested scenario and is not claimed by this slow-reader test; the existing invalid unmapped move check is not a substitute for a popup-grab fixture.
