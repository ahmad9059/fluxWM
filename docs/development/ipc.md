# IPC implementation and integration

`flux-ipc` supplies Linux IPC v1 framing, request/response DTOs, a nonblocking local server and a deadline-bounded CLI client. `fluxctl` is its command-line frontend. This implements part of the [implementation contracts](../20-implementation-contracts.md) and [resource security](../17-resource-security.md); it does not close phase 6 or qualify a production desktop. The [initial production audit](ipc-production-audit.md) and its original transcripts describe the earlier revision and remain historical evidence.

## Event-loop integration

```rust,ignore
let mut ipc = flux_ipc::Server::bind(runtime_dir, instance_name)?;
// Export the exact socket_path() to owned session children as FLUXWM_SOCKET.
// At fd readiness, an armed IPC deadline, or buffered-work continuation:
ipc.set_state_sequence(state.sequence());
ipc.poll(std::time::Instant::now(), |request| {
    // Snapshot queries, validate state, or enqueue actions without blocking.
    // Return the EXACT resulting core state_sequence, including any mutation.
    dispatch(request, state)
});
let readiness = ipc.readiness(std::time::Instant::now());
// Reconcile readiness.interests with event-loop registrations.
// Schedule a continuation when readiness.needs_dispatch is true.
// Arm readiness.next_deadline even when no fd is ready.
```

`readiness()` is calloop-neutral. Each `IoInterest` contains an `Endpoint`, borrowed raw fd and readable/writable flags. `Endpoint::Listener` identifies the listener; `Endpoint::Client(u64)` is a per-connection token independent of descriptor reuse. The server owns every fd: adapters must not close them. Reconcile after every poll; remove obsolete registrations before registering replacement fds and ignore stale endpoint events. Do not assume an integer fd identifies the same client forever.

`needs_dispatch` covers complete requests already held in userspace and IO work stopped by a budget. Arrange another bounded dispatch without waiting for a fresh fd edge. A continuously blocked writer has a writable interest, rather than an immediate continuation loop. `next_deadline` is the earliest absolute monotonic deadline for incoming partial frames, pending response, idle connection or accept retry; register or update a timer and clear obsolete timers. Initial readiness includes the listener, so the compositor can sleep without periodic IPC polling while idle. The adapter remains responsible for scheduling IPC fairly with input, Wayland, GPU, lock and device events.

`poll` accepts at most 32 peers and invokes at most 16 handlers per connection per call. Read/write byte budgets are 64 KiB each per connection per call; each read/flush loop also has a syscall-attempt bound. Interrupted syscalls cannot create an unbounded inner retry loop. Acceptance errors and connection-capacity pressure disable acceptance for 100 ms and expose a retry deadline. `stats()` exposes bounded counters for acceptance, capacity/credential rejection, accept errors, disconnects, protocol errors and timeouts; it does not log payloads. The compositor should export those counters in bounded redacted diagnostics and rate-limit human-readable error messages.

Call `set_state_sequence` with the current core sequence at poll entry. Every handler response must contain the exact resulting sequence. The server uses that response to advance its sequence before decoding the next request, including requests on another connection; transport-generated validation errors therefore see mutations earlier in that poll. A regressing handler sequence, wrong ID or wrong version is a handler contract violation and closes the connection. External mutations outside the handler must be reflected at the next poll entry. A restarted instance needs a separately specified instance/epoch policy before subscriptions or persistent clients are supported.

## Optional calloop driver

Enable the `calloop` feature on `flux-ipc` to use `EventLoopDriver<D>` with calloop 0.14.5. The transport's default features remain empty; applications with another event loop can continue using `Server` and `readiness()` directly.

```rust,ignore
// Store the driver in compositor state as Option<EventLoopDriver<Fluxwm>>.
fn wake_ipc(state: &mut Fluxwm) { state.ipc_pending = true; }
let driver = flux_ipc::EventLoopDriver::bind(
    &event_loop.handle(), runtime_dir, instance_name, wake_ipc,
)?;
// After event_loop.dispatch(), outside all source callbacks:
if std::mem::take(&mut state.ipc_pending) {
    let mut driver = state.ipc.take().expect("live IPC driver");
    let outcome = driver.poll(Instant::now(), state.sequence(), |request| {
        state.dispatch_ipc(request) // exact resulting sequence in every response
    });
    // On adapter registration failure, drop the driver and follow the declared
    // IPC recovery/session policy. Preserve the error in bounded diagnostics.
    outcome?;
    state.ipc = Some(driver);
}
```

The driver owns `Server` and exposes `socket_path()`, `stats()` and `connection_count()`. Its source callbacks only call the supplied `wake` function, which must mark pending work rather than invoke `poll` reentrantly. Service that flag after dispatch. A coalesced ping schedules complete buffered requests even if the client sends no additional bytes. One timer tracks the exact earliest server deadline, including partial frames and accept backoff; obsolete timers and socket registrations are removed. With no clients or buffered work there is no IPC timer or periodic polling.

Socket registrations use owned `F_DUPFD_CLOEXEC` duplicates, keyed by `Endpoint`; originals can close during `Server::poll` without invalidating a registered borrowed descriptor. At most 33 additional socket descriptors exist (listener plus 32 clients), plus the calloop ping descriptor and calloop's own backend descriptors. Changed interests and obsolete endpoints are removed before replacements are installed. Dropping the driver unregisters its socket, ping and timer sources before dropping `Server`, so duplicate descriptors do not retain a disconnected client/socket.

Only a weak loop handle is retained, including when a driver lives inside loop-owned state or callbacks. calloop 0.14.5 does not re-export the returned weak-handle type; the adapter stores an upgrade closure that captures that weak handle without capturing a strong handle. Loop destruction therefore cannot create a source/handle reference cycle. A driver that outlives its loop returns `BrokenPipe` from `poll` and still cleans up its owned socket when dropped.

Verify the optional adapter with:

```sh
cargo test -p flux-ipc -p fluxctl --features flux-ipc/calloop
cargo clippy -p flux-ipc -p fluxctl --features flux-ipc/calloop --all-targets -- -D warnings
```

Actual calloop tests cover forty pipelined requests followed by client silence, EOF and large pending-reply flush, read/write-interest transitions, deadline wakeup without periodic polling, acceptance backoff/rearm, idle wake suppression, endpoint replacement, driver/loop destruction in both orders and cleanup when loop-owned callbacks hold a driver. The deadline test moves an existing five-second partial-frame start timestamp so its exact expiry is forty milliseconds away; it exercises the real calloop timer rather than waiting five seconds. This adapter evidence does not qualify compositor-wide scheduling latency or native desktop readiness.

## Socket ownership and authentication

The Linux `SO_PEERCRED` check admits the effective UID only; the client helper also verifies its server has the same UID. Bind requires a real final runtime-directory component owned by that UID with no group/other permissions. Its `fluxwm/` child has mode 0700; sockets have mode 0600. Instance names contain 1..64 ASCII letters, digits, hyphens or underscores. Existing paths are never replaced or unlinked on startup.

After binding, an inode/device cleanup guard covers later setup failures and normal shutdown. Cleanup removes only the recorded socket identity, preserving a replacement path. This is best-effort filesystem cleanup: if identity lookup itself fails or later filesystem access is denied, an operator may need to handle a stale path. Crash termination does not execute destructors. Use a unique instance name and a session-owned cleanup policy; never unlink a possible live instance just from its filename.

These checks implement the documented host-session same-user boundary. They do not defend against malicious unsandboxed same-UID software or an unsafe intermediate runtime-directory path. A sandbox granted this socket retains the corresponding local-control authority. Stronger isolation needs an additional authenticated channel policy.

## Framing, validation and backpressure

Frames contain a four-byte big-endian payload length followed by 1..1,048,576 UTF-8 JSON bytes; the prefix is additional. Incoming framing rejects oversize lengths before body allocation. Strict parsing rejects duplicate keys recursively, trailing JSON, invalid UTF-8, value nesting deeper than 32, invalid envelopes and IDs. Encoding enforces the same byte/depth policy incrementally while serde emits bytes, before unbounded serialization recursion or an oversized encoded buffer; a final bounded strict parse catches inconsistent custom serializers. The server substitutes a small typed `internal_error` if the handler produces an oversized or otherwise invalid reply.

The handler must still bound construction of its `Value` snapshot before calling the encoder. Serializer limits cannot undo memory already allocated by a huge snapshot. Decoding produces a bounded-size JSON tree with memory overhead above raw payload length. Query pagination, cached snapshots and whole-loop CPU/RSS qualification remain required before broad production performance claims.

At most one response is queued per connection. While it is pending, the server suppresses further reads and action dispatch. This reserves capacity for a maximum-size reply before another command can have a side effect. Pipelined incoming frames remain buffered or in kernel socket queues and resume after response flush. Request IDs contain 1..128 UTF-8 bytes and may be reused after the corresponding response has fully flushed; they are not persistent deduplication tokens. This response backpressure is stricter than the originally proposed 256-event subscription ceiling. There are no subscribers yet.

A partial incoming frame has a five-second deadline. Each outgoing response has its own absolute five-second enqueue-to-flush deadline, independent of progress; subsequent responses start new intervals. This bounds individual slow replies without applying a single lifetime deadline to an indefinitely active connection. Idle clients expire after 60 seconds. Per-connection receive staging is bounded by one maximum payload, prefix and read budget; one outgoing frame is bounded by payload plus prefix. These are admission limits, not complete RSS bounds; JSON trees, handler state, temporary buffers and kernel queues add memory.

Input EOF is distinct from a full disconnect. Complete buffered requests are processed and their responses flushed before closure. A truncated trailing frame is discarded and counted as a protocol error after prior complete requests; their acknowledged mutations/responses are preserved. Malformed framing/JSON or an invalid outstanding ID closes after already pending work flushes. Full connection loss or a handler failure can still leave a client uncertain whether an action was applied. There is no exactly-once guarantee across disconnect/restart, and no automatic retry of mutation requests.

## Requests and operation semantics

Methods: `version`, `capabilities`, `windows.list`, `workspaces.list`, `outputs.list`, `config.effective`, `diagnostics`, `config.check`, `config.reload`, `action`. Queries take empty object parameters. `config.check` accepts an optional nonempty NUL-free `path` string of at most 4096 bytes; missing path selects the handler's declared default. Relative paths, allowed roots, symlinks, special files and file snapshots are handler policy and must not cause blocking file operations in `poll`.

`action` uses the shared `flux_core::Action` wire enum and decoder. Core decoding validates exact fields and numerical/argv policy; constructed in-process actions still require core validation before admission. The compositor must additionally enforce lock/permission state, stale references, target availability and command queue limits. Spawn carries explicit argv, and this transport never invokes a shell.

Responses contain version, matching ID, decimal-string `state_sequence`, and exactly one `result` or `error`. `Sequence` accepts canonical strings from `0` through `18446744073709551615`; JSON numbers and leading zeros are rejected. Runtime object IDs retain `index:generation` strings. Typed `Window`, `Workspace`, `Output`, and `Event` DTOs define wire shapes; the compositor owns authoritative query content and capabilities.

An accepted command is not evidence of hardware presentation. Operation IDs, completion queries/events, cancellation and restart recovery still need a complete contract before asynchronous device/config operations are advertised as complete. An `internal_error` after a handler applies a mutation does not prove the mutation was rolled back. On an ambiguous disconnect or error, refetch authoritative state where meaningful; blindly retrying spawn, toggles or relative moves can repeat side effects.

There is no subscription method or broadcaster. `Event` defines serialization only; capabilities must report subscriptions unavailable until bounded replay/resync is implemented. The transport has no native rendering, portal, capture or session readiness authority. Additional response fields/error codes can break strict v1 clients; define compatibility/versioning before freezing the public interface.

## Client

```sh
fluxctl --socket "$FLUXWM_SOCKET" query windows.list
fluxctl --socket "$FLUXWM_SOCKET" action '{"name":"focus","direction":"right"}'
fluxctl --socket "$FLUXWM_SOCKET" config-check /absolute/path/config.lua
fluxctl --socket "$FLUXWM_SOCKET" reload
```

Use `--socket` or `FLUXWM_SOCKET`; ambiguous socket discovery is absent. Exit codes: 0 success, 2 invalid arguments, 3 connection/readiness failure, 4 server rejection, 5 malformed protocol. Full JSON responses go to stdout; diagnostics go to stderr.

`call()` uses a single five-second monotonic transaction deadline. `call_with_timeout()` permits another bounded duration for workers/tests. Encoding, nonblocking connect (including a full AF_UNIX listen backlog), request writes, fragmented prefix/body reads and final validation all share that deadline. Peer credentials are checked after connection. This helper waits for IO and belongs only in CLI/worker contexts. The deadline limits transport wait; it neither cancels an already accepted server action nor waits for hardware completion.

## Verification and remaining limits

Run `cargo test -p flux-ipc -p fluxctl` and `cargo clippy -p flux-ipc -p fluxctl --all-targets -- -D warnings`. Tests cover bytewise fragmentation/coalescing, duplicate keys, depth/UTF-8/trailing data, oversize prefixes, stale versions, string counters, permissions/collisions, setup-failure cleanup, replacement inode preservation, EOF with complete/truncated frames, large reply flush after half-close, same/cross-client sequence freshness, output backpressure/per-message expiry, buffered-work continuation, accept backoff, bounded recursion/encoding and total client deadlines under trickle replies/full connect backlog. CLI integration covers successful queries and readiness/rejection exit codes.

These tests do not establish whole-compositor calloop registration correctness, latency under floods, measured global memory bounds, subscriber resync, hostile same-UID races, physical lock/capture safety or production session readiness. Retain the audit's unrun qualification cases. The historical defect probe/transcript is preserved under `docs/evidence/ipc-audit-probe`; the `current` example checks the corrected invariants against the integrated core/IPC revision without overwriting those historical observations.

## Quit drain boundary

The nested runtime uses `poll_controlled` and `Dispatch::Drain` to queue the quit response before globally ending further dispatch. Only already-queued replies drain, with an absolute 250 ms deadline; pipeline frames after quit are discarded without execution or a reply. `begin_drain` supports external shutdown initiation and cannot extend an existing deadline. Drain readiness has no listener/read interests. `drain_state` exposes remaining userspace bytes, and `drain_timed_out` counts abandoned pending frames. Kernel acceptance is not peer receipt. Read the [shutdown contract and real regression harness](shutdown-smoke.md) for exact scope and remaining teardown limits.

`windows.list` includes per-binding `responsiveness`; `diagnostics.shell_health` supplies bounded probe states, fixed development timing policy and counters. [Shell-health documentation](shell-health.md) explains unmonitored overflow, sequence advancement and the distinction between ping response and application frame progress.

`diagnostics.input_state` reports virtual key/suppression counts, observed host focus, grab flags and `authorized_buttons` (the application’s authorization ledger). `composed_scene` counts come from the last successful published snapshot. [Host-focus documentation](host-focus.md) distinguishes those observations from hardware state and raw-pixel proof.

The nested `windows.list` rows expose [toplevel configure state](configure-stream.md): actual unacknowledged count, pending intent, and separate last-acked/committed serials and size suggestions. These fields do not prove actual buffer size, acquire readiness or presentation.

`diagnostics.keyboard_bindings` reports the development-table source, entry/inactive counts and `runtime_keymap_changes: false`. These counts do not mean user configuration was applied; config check/reload/effective runtime endpoints remain unavailable.
