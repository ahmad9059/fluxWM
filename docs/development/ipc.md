# IPC implementation and integration

The `flux-ipc` crate supplies IPC v1 transport and DTOs. `fluxctl` supplies a local client. This implements the framing and trust baseline in [implementation contracts](../20-implementation-contracts.md) and [resource security](../17-resource-security.md). It does not prove production readiness or hardware behavior.

## Server integration

```rust,ignore
let mut ipc = flux_ipc::Server::bind(runtime_dir, instance_name)?;
// Export this exact path to children/session activation as FLUXWM_SOCKET.
let path = ipc.socket_path();
// Every event-loop iteration; never call blocking operations inside this closure.
ipc.set_state_sequence(state.sequence());
ipc.poll(std::time::Instant::now(), |request| {
    // Snapshot query results atomically, or validate/enqueue Action intent.
    dispatch(request, state)
});
```

`poll` performs nonblocking IO and invokes at most 16 requests per connection per call. There are at most 32 connections, 32 accepts per call, 64 KiB reads and 64 KiB writes per connection per call. The event loop must call it regularly; a listener socket alone is not readiness evidence. Native-session readiness and actions remain the compositor's responsibility. Call `set_state_sequence` before polling so transport validation errors report the current snapshot sequence.

The authenticated principal is the effective Unix UID using Linux `SO_PEERCRED`. Bind requires a real runtime directory owned by this UID with no group/other permissions. Its `fluxwm/` child has mode 0700; sockets have mode 0600. Instance names contain 1..64 ASCII letters, digits, hyphens or underscores. Existing paths are never unlinked on startup. Drop removes a socket only when its recorded device/inode still match. This does not defend against malicious unsandboxed same-UID code; it implements the documented host-session trust model.

The transport accepts JSON payloads from 1 through 1,048,576 bytes after a four-byte big-endian length. It rejects oversize lengths before allocating the body, duplicate keys recursively, trailing JSON, invalid UTF-8, nesting deeper than 32, unknown request fields, unsupported versions and invalid IDs/params. Request IDs contain 1..128 UTF-8 bytes and remain outstanding until their response flushes. Reusing an outstanding ID closes the connection. Malformed framing or JSON closes the connection without a best-effort reply. Well-formed unsupported versions, methods, and parameters receive typed errors. Partial messages and stalled outbound queues expire after five seconds, independent of trickle progress. Idle clients expire after 60 seconds.

Per-connection receive staging is bounded to one maximum payload plus a four-byte prefix and 64 KiB read budget; outgoing bytes are capped at one maximum payload plus prefix, and 256 messages. Queue overflow disconnects. These admission limits do not describe total RSS: JSON tree expansion, temporary encoded responses and kernel buffers consume additional memory. The handler must bound snapshot construction before returning, and must avoid filesystem work, process waits, Lua evaluation and blocking device operations. A full-state result larger than the transport maximum is a handler error; pagination is future work.

## Messages and results

Methods are `version`, `capabilities`, `windows.list`, `workspaces.list`, `outputs.list`, `config.effective`, `diagnostics`, `config.check`, `config.reload`, and `action`. Queries accept empty object parameters. `config.check` accepts an optional `path` string of at most 4096 bytes. `action` uses the existing `flux_core::Action` tagged enum, including explicit `argv` spawn; this transport never invokes a shell. The compositor must additionally validate state, lock restrictions, numerical policy, stale references and command admission.

A response contains version, matching ID, decimal-string `state_sequence`, and exactly one of `result` or `error`. `Sequence` accepts canonical decimal strings from `0` through `18446744073709551615`; JSON numbers and leading zeros are rejected. Core window/workspace/output IDs preserve `index:generation` strings. The transport provides typed `Window`, `Workspace`, `Output`, `Event` DTOs; compositor snapshots own final query content. An accepted command is not evidence that hardware presentation completed.

There is currently no subscription method or event broadcaster. `Event` defines serialization only. Capabilities must report subscriptions as unavailable until bounded replay/resync is implemented. There is no native rendering, portal, capture or session readiness claim in this crate.

## Client

```sh
fluxctl --socket "$FLUXWM_SOCKET" query windows.list
fluxctl --socket "$FLUXWM_SOCKET" action '{"name":"focus","direction":"right"}'
fluxctl --socket "$FLUXWM_SOCKET" config-check /absolute/path/config.lua
fluxctl --socket "$FLUXWM_SOCKET" reload
```

Use `--socket` or `FLUXWM_SOCKET`; ambiguous socket discovery is intentionally absent. Exit codes: 0 success; 2 invalid arguments; 3 connection/readiness failure; 4 server rejection; 5 malformed protocol. Full JSON responses are printed to stdout; diagnostics go to stderr. The client helper is blocking and belongs only in CLI/worker contexts, with five-second socket read/write timeouts and bounded reply length. It does not impose a total transaction wall deadline against a continuously trickling server.

## Evidence

Run `cargo test -p flux-ipc -p fluxctl` and `cargo clippy -p flux-ipc -p fluxctl --all-targets -- -D warnings`. Tests exercise bytewise fragmentation, coalescing, duplicate JSON keys, depth/UTF-8/trailing input, oversize prefixes, stale versions, string counters, restrictive permissions, startup collisions, socket cleanup, partial-frame deadlines, duplicate outstanding IDs, queue overflow, connection admission, CLI success, readiness errors and rejection exit codes. They do not qualify hostile same-user directory races, other operating systems, or compositor runtime integration.

## Review-added event-loop and backpressure blockers

The current server has no public listener/connection readiness handles, pending-user-space-work indicator or next-deadline API. Calling `poll` only on unrelated compositor activity can strand decoded frames and deadlines; continuously polling conflicts with idle efficiency. Before compositor integration, provide a calloop event source or explicit readiness/deadline contract, schedule another bounded turn when decoded work remains, and back off accepts on EMFILE. Test 17 complete requests in one write followed by silence, timeout expiry with no further I/O and isolated descriptor exhaustion. See N3 in the [Claude disposition](../reviews/implementation-disposition.md).

Receive staging currently keeps reading before decoding at most 16 requests. Bound outstanding requests explicitly and apply read backpressure when complete buffered frames can be serviced. Test valid pipelines within the documented admission window, then exceed that window to verify deliberate overflow behavior and mutation/reply ambiguity. There is no promise that arbitrary 10,000-request pipelines fit every configured resource limit. N4 remains open; the current staging cap alone is not a complete admission/backpressure contract.
