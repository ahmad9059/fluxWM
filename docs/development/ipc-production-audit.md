# IPC, local authority, desktop lifecycle, and release audit

Audit date: 2026-10-17. Audited code revision: `d14957fc63487c139cb194ceb687231e2cf83a98` (`feature/ipc`). This is an independent pass over the initial transport implementation, including defects in code written by this reviewer. It is not an independent Claude review and does not qualify a production compositor. Findings below remain open until fixes and their new-build evidence exist.

The baseline transport has useful controls, but passing its twelve tests does not establish frame scheduling fairness, complete IPC semantics, secure locking, desktop integration, or release readiness. Five probe groups exposed request loss, regressing response sequences, a request/response depth mismatch, an underspecified path boundary, and accepted unknown fields on unit actions. Do not close phase 6 or production blockers from this audit.

## Evidence and scope

- [Input hashes](../evidence/ipc-audit-input-sha256.txt) identify the exact audited source and security/desktop/release documents.
- [Machine-readable manifest](../evidence/ipc-audit-manifest.json) records revision, toolchain, commands, observations, and artifact hashes.
- [Probe source](../evidence/ipc-audit-probe/src/main.rs), [manifest](../evidence/ipc-audit-probe/Cargo.toml), and [lockfile](../evidence/ipc-audit-probe/Cargo.lock) preserve reproduction.
- [Probe transcript](../evidence/ipc-audit-probe-output.txt) contains the observed outputs.
- [Existing test transcript](../evidence/ipc-audit-tests.txt) records nine transport tests plus three CLI integration tests passing.

Reproduce from the repository root:

```sh
cargo test -p flux-ipc -p fluxctl
cargo run --locked --manifest-path docs/evidence/ipc-audit-probe/Cargo.toml --target-dir /tmp/flux-ipc-audit-target
```

The probe creates its own private temporary runtime directory and Unix socket. It never touches an existing desktop, seat, GPU, or session service. Its assertions intentionally describe the defective audited behavior; change them to assert the desired invariant when correcting the implementation. A successful run currently reproduces defects rather than certifying safety. If dependency resolution or source code changes, record the new revision and retain the old artifacts.

Classification: **observed** means executed in the preserved probe; **source-derived** means a control-flow or schema fact in the hashed inputs; **unrun qualification** means a required scenario without runtime evidence. P1 blocks the relevant stable interface or security/reliability claim; P2 blocks complete feature/operational qualification; P3 is documentation or usability work. No finding claims an exploitable cross-user privilege escalation without evidence.

## Prioritized findings

### IPC-A01 — P1: EOF discards complete requests before dispatch

**Observed.** [Transport source](../../crates/flux-ipc/src/lib.rs):547–567 reads until `WouldBlock`, and line 549 returns on `Ok(0)` before decoding buffered complete frames. The probe writes one complete `version` request, calls `shutdown(Write)`, then polls the server. Result:

```text
half_close: dispatched=0 remaining_connections=0
```

A client can close its sending half while keeping its receiving half open. Rust documents [directional socket shutdown](https://doc.rust-lang.org/std/os/unix/net/struct.UnixStream.html#method.shutdown), and [Read::read](https://doc.rust-lang.org/std/io/trait.Read.html#tymethod.read) documents `Ok(0)` as end-of-stream for a nonempty buffer. An input EOF is therefore distinct from inability to deliver output.

Required invariant: finish decoding complete buffered requests on input EOF, reject any trailing incomplete frame explicitly, flush accepted responses under a deadline, and then close. A valid request must not disappear solely because no more request bytes will arrive.

Closure cases: complete single frame plus write-half-close; two complete frames plus half-close; complete frame plus truncated trailing frame; EOF with pending output; full close after dispatch. Observe dispatch counts and raw reply bytes. Decide and document whether a truncated trailing frame invalidates only itself or the entire batch; never silently vary based on read chunking.

### IPC-A02 — P1: error state sequence can move backward within one poll

**Observed.** The transport stores sequence in [source](../../crates/flux-ipc/src/lib.rs):459–460, passes it by value at line 493, and generates validation errors using that old value at line 574. The handler can mutate state and return a newer sequence before a later frame in the same poll is rejected. The probe sets sequence 10, has the handler return 11 for a mutation, then sends an unsupported version:

```text
batch_sequence: id=mutation sequence=11
batch_sequence: id=invalid-after-mutation sequence=10
```

This contradicts the current-state response contract in [implementation contracts](../20-implementation-contracts.md):69–71 and [desktop/security](../08-desktop-security.md):41. Clients cannot use the second response as an authoritative current snapshot sequence.

Required design decision: obtain current sequence for each generated response, or route validated-envelope rejection through state-aware dispatch. Do not infer freshness merely by taking the largest previous response unless the API expressly guarantees all mutation pathways use that same sequence source.

Closure cases: mutation then invalid version; mutation then unknown method; one connection mutates and another receives an error in the same poll; rejected mutation leaves sequence unchanged; overflow/restart behavior. Assert exact sequence against core state after every response. `set_state_sequence` once before `poll` does not close this finding.

### IPC-A03 — P1: outgoing validation differs from incoming validation

**Observed plus source-derived.** [Source](../../crates/flux-ipc/src/lib.rs):338–343 rejects incoming nesting deeper than 32, while `encode` at lines 348–356 checks only payload length. A deeply nested `Response::success` is emitted successfully but the crate's own `decode::<Response>` rejects it:

```text
response_depth: encode_ok=true decode_ok=false
```

In addition, serialization allocates the entire payload at line 349 before applying the 1 MiB limit at line 350. This second issue is source-derived; no OOM experiment was run. It matters for large window/config/diagnostic snapshots generated from valid live state, even when peers send small queries.

Required invariant: sender and receiver agree on accepted JSON shape, depth and size; serialization stops when its byte budget is exceeded; snapshot construction also has a bounded policy. Counting the finished encoded message alone cannot bound the preceding `Value` allocation. Select pagination or a typed too-large error for queries that exceed the response contract. Do not silently disconnect a normal client after allocating a huge snapshot.

Closure cases: depth exactly at and above limit including response envelope; payload lengths max−1/max/max+1; huge titles and large window lists; full effective configuration near the separate 4 MiB config-helper ceiling; diagnostic growth during fault storms. Record process RSS and compositor scheduling latency. The helper's 4 MiB result ceiling in [resource policy](../17-resource-security.md):34 is not the public IPC ceiling.

### IPC-A04 — P1: asynchronous operations and recovery from uncertain delivery lack a complete contract

**Source-derived; runtime semantics unrun.** [Implementation contracts](../20-implementation-contracts.md):34 require accepted/rejected intent, operation IDs and later completion events for device-state operations. [Method](../../crates/flux-ipc/src/lib.rs):50–74 has no operation-status query or subscription method; `Event` is serialization only. `Outcome::Success` exposes arbitrary `Value`, without a mandatory distinction between queued and completed.

The connection can fail after a handler has applied an action but before its reply reaches the client: `process` invokes the handler at line 573, then serializes/queues at lines 579–581. A later duplicate ID, oversized result or queue overflow can discard queued replies. IDs are tracked only in that connection until flush, not as persistent deduplication tokens. This is a delivery ambiguity, not proof that the server itself applies an action twice.

Required decisions: document retry behavior per method; prohibit automatic retry of non-idempotent spawn/quit/move/toggle operations after an ambiguous disconnect; distinguish operation identity from transport request ID; define completion observation, bounded retention, cancellation and restart semantics. Add `operations.get` or another authoritative completion route if subscriptions are unavailable. A reconnect with the same request ID must not be presented as exactly-once execution.

Closure cases: disconnect after action application but before reply; malformed frame after a valid spawn in the same packet; queue overflow after accepted mutation; compositor restart during operation; late completion after target destruction; timeout with unknown completion. Assert at-most-once application for the defined scope and explicit unknown outcome outside it.

### IPC-A05 — P1: byte budgets do not prove event-loop fairness

**Source-derived; timing qualification unrun.** [Server::poll](../../crates/flux-ipc/src/lib.rs):467–493 can process 32 connections with 16 synchronous handler calls each: up to 512 handlers per invocation. JSON parsing, tree allocation, snapshot generation and serialization all occur on that caller. `Interrupted` read/write paths at lines 559 and 614 retry without a syscall-attempt budget. No global CPU-time budget, priority reservation, dispatch telemetry or handler containment exists in this crate.

This does not prove a measured input or lock stall. It leaves [fair scheduling](../17-resource-security.md):54–58 and [release flood behavior](../19-release-operations.md):38 unqualified. The peer is trusted same-UID under model A, but ordinary malfunctioning control tools must still not freeze input.

Required qualification: measure worst-case per-poll duration with 32 clients, full-size invalid requests, legitimate heavy queries and concurrent redraw/input/lock/device events. Set approved latency and memory thresholds from measured hardware. Enforce global admission/fairness and cheap cached snapshots or worker jobs where necessary. Code comments saying a handler must be nonblocking do not prevent CPU-heavy work or panics.

Closure cases: 32 active readers plus lock transition; repeated maximum-size JSON parsing; signal interruption storm; hotplug during repeated config/effective queries; handler failure containment. Capture raw event timestamps, poll duration distributions, allocations, fds and queue occupancy, and verify no starvation of resource retirement.

### IPC-A06 — P2: queue timeout is queue-age based, not stalled-progress based

**Source-derived; progress scenario unrun.** [Source](../../crates/flux-ipc/src/lib.rs):589–590 starts `write_since` when a queue first becomes nonempty and clears it only when completely empty at lines 618–619. Successful writes at lines 604–607 do not advance that timestamp. A continuously nonempty queue can expire after five seconds while the consumer makes steady progress. The partial-read timestamp likewise spans buffered traffic until the entire decoder is empty.

[Integration docs](ipc.md):23 describe a stalled outbound queue. That phrase does not match the implemented absolute nonempty-queue age policy.

Select and document either an absolute per-message deadline, an inactivity deadline plus maximum-age cap, or deliberate disconnect of continuously backlogged clients. Track individual oldest-frame enqueue times if that is the chosen contract. Test trickle readers, continuously progressing readers, long fragmented requests, multiple buffered complete requests, exact deadline boundary, and delayed poll scheduling. Use virtual time for determinism plus a real socket run for IO behavior.

### IPC-A07 — P2: admission failures and disconnect causes are invisible

**Source-derived.** [Source](../../crates/flux-ipc/src/lib.rs):471–475 drops excess clients/credential failures; line 489 silently stops acceptance for other errors; line 493 discards every processing error. `poll` returns no diagnostic events or counters. This fails to provide the clear limit diagnostic proposed in [resource policy](../17-resource-security.md):28 and the rate-limited first failure guidance at lines 58 and 64.

Expose bounded reason counters and a rate-limited diagnostic channel for connection limits, malformed framing, deadline, queue overflow, wrong UID, `EMFILE`/`ENFILE`, and unexpected IO errors. Do not log request payloads, titles or configuration source. Reserve a minimal diagnostic path that does not allocate an unbounded string per failure. Test fd pressure in an isolated child process, reconnection storms, and permission-denied paths. Assert the input loop keeps progressing and diagnostics neither disappear entirely nor become a log flood.

### IPC-A08 — P2: runtime socket lifecycle and readiness need failure-injection evidence

**Source-derived plus unrun qualification.** [Bind](../../crates/flux-ipc/src/lib.rs):433–446 checks directories, creates the socket, then adjusts permissions/nonblocking mode and records inode. If a fallible step after bind returns an error before `Server` exists, `Drop` cannot clean up the socket. Ordinary startup collision tests do not exercise these failure paths. Existing paths are deliberately preserved; crash-stale instance names therefore need an operator/session-manager recovery policy.

The final runtime path component is checked with `symlink_metadata`; intermediate path components are not resolved through retained directory descriptors. This is not evidence of a cross-user exploit in a correctly owned 0700 XDG runtime tree. Record the trusted path assumptions and whether private inherited directory handles or `openat`-style resolution are required by the actual launcher. The [XDG Base Directory specification](https://specifications.freedesktop.org/basedir/latest/) requires a user-owned runtime directory with mode 0700 and lifecycle constraints. Linux [unix(7)](https://man7.org/linux/man-pages/man7/unix.7.html) documents pathname-socket permissions and `SO_PEERCRED`.

Closure cases: failure after successful bind; crash without Drop; stale socket and conflicting live compositor; long runtime path exceeding `sockaddr_un`; symlink final component; substituted child directory; readiness probe reaches a live server but compositor output/input prerequisites fail. Never unlink a possible live instance based only on a filename or PID string. Confirm socket cleanup does not remove a newer instance's socket.

### IPC-A09 — P2: subscriptions, snapshots and compatibility are planned, not implemented

**Source-derived.** [Desktop/security](../08-desktop-security.md):43 lists subscriptions and [phase 6](../phases/06-lua-ipc.md):16 requires bounded subscriptions/events. [Integration docs](ipc.md):33 correctly say this is absent. Consequently phase 6 remains incomplete even though basic query/action framing exists.

Before implementation, specify per-instance sequence epoch, restart detection, subscription start handshake, snapshot/event atomicity, gap detection, sequence wrap policy, bounded replay retention, overflow disconnect/resync, ordering across outputs/config generations and privacy on lock. A decimal `u64` alone cannot distinguish a restarted instance that reuses a sequence. Capability reporting must accurately distinguish unsupported, temporarily unavailable and degraded operations.

Also define v1 extension policy. `Request` denies unknown fields, `Response` rejects extra fields, and `ErrorCode` rejects new unknown variants. Adding fields or error variants inside v1 can therefore break existing clients. Freeze a compatibility matrix or explicitly allocate a new version; do not assume additive JSON fields are automatically compatible.

Closure cases: subscribe while mutation occurs; overflow exactly at byte and message limit; reconnect after missing events; server restart with lower counter; stale generational IDs; unknown optional field/error variant according to chosen compatibility policy. Preserve golden wire vectors for each supported version.

### IPC-A10 — P2: the CLI has no absolute transaction deadline

**Source-derived; slow-server wall-time run not performed.** [Client helper](../../crates/flux-ipc/src/lib.rs):627 connects before configuring timeouts. Lines 628–638 apply socket-operation timeouts and then `read_exact`; a peer that supplies a byte before each timeout can prolong the transaction. [Integration docs](ipc.md):44 acknowledge this accurately. There is also no client-side peer UID validation; an explicitly chosen arbitrary socket is trusted as the response source.

Decide a bounded connect plus total transaction deadline, separately from server execution/completion semantics. Document whether the CLI requires the same-user server/runtime-directory checks or permits arbitrary test sockets only through an explicit option. Classify connect/read timeout separately from a well-formed `not_ready` reply; never imply a socket path proves compositor readiness.

Closure cases: hung connect/listener backlog; one reply byte per timeout interval; valid response after deadline; truncated prefix/body; response ID/version mismatch; unknown response fields; fake endpoint of a different UID in an isolated test namespace. Confirm exit codes, elapsed wall time and stdout/stderr separation.

### IPC-A11 — P2: action/path validation remains split and incomplete

**Observed plus source-derived.** `config.check` accepts an empty path (probe result `accepted=true`); [validation](../../crates/flux-ipc/src/lib.rs):127–137 checks type, byte length and keys, but not NUL, absolute/relative semantics or directory-vs-file intent. `action()` at lines 96–98 validates enum shape, not domain ranges or lock permissions. The resource/config contract at [resource policy](../17-resource-security.md):50 requires rejecting NUL in OS argv/path fields.

Empty paths are not inherently a vulnerability; their meaning is unspecified. Define empty/missing path behavior, absolute vs compositor working-directory resolution, UTF-8 policy, symlinks, special files/FIFOs, maximum bytes and config file snapshot ownership. Reject embedded NUL before OS APIs. Give both Lua and IPC the same centralized action validation for finite numeric policy, pan/resize ranges, spawn argv count/byte limits and lock restrictions. A syntactically valid enum is insufficient permission to execute it.

Closure cases: empty/NUL/relative/long/nonexistent/directory/FIFO paths; empty argv, NUL argument and oversized argv; enormous finite pan/resize numbers; lock arriving between action admission and application. Assert bounded file handling, typed errors, preserved previous config, no render-thread file evaluation, and no new process after rejected spawn. No actual FIFO/blocking-file test was run in this audit.

### IPC-A12 — P3: documentation status and wire examples are inconsistent

**Source-derived.** [Implementation contracts](../20-implementation-contracts.md):3 still says the standalone probe is the only Rust code; [production audit](../13-production-audit.md):3 says no compositor implementation exists. These are historical snapshots, not descriptions of every current branch. Preserve their dates but link a build-specific implementation status ledger. Do not turn a new skeleton into a production readiness claim.

The sequence examples at [implementation contracts](../20-implementation-contracts.md):60 and 66 use numeric `42`, whereas `Sequence` requires decimal strings. The older CLI examples at [desktop/security](../08-desktop-security.md):45–54 are explicitly proposed/unavailable and differ from implemented `query`, JSON `action`, `config-check` and `reload`. Mark their historical status or update them to the authoritative implemented help, including which commands still require compositor integration.

Closure: parse every normative wire example through the corresponding versioned decoder; run every documented implemented CLI example against a controlled server; check status statements against commit-specific evidence. A link checker alone does not validate command or wire correctness.

### IPC-A13 — P1: unknown fields are accepted on unit Action variants

**Observed.** [Request::action](../../crates/flux-ipc/src/lib.rs):96–98 relies on `serde_json::from_value::<flux_core::Action>`. Although [Action](../../crates/flux-core/src/lib.rs):119 declares `deny_unknown_fields`, the unit variants at lines 129–133 accept extra object fields under the pinned serde implementation. The preserved probe calls `Request::validate` with `{"name":"quit","unexpected":true}` and equivalent unit actions:

```text
unit_action_unknown_field: name=quit accepted=true
unit_action_unknown_field: name=lock accepted=true
unit_action_unknown_field: name=reload-config accepted=true
unit_action_unknown_field: name=close-window accepted=true
```

These are ordinary same-user control requests; this observation does not establish a privilege bypass. It does establish that unknown action fields are not rejected consistently, contrary to the strict schema intent. A typo or future conditional field on a destructive action may be silently ignored rather than rejected. Do not treat a derive attribute or a test of a struct variant as proof for every enum representation.

Required fix: use a shared strict decoder that validates the exact allowed keys for every action variant before deserializing, or change the wire representation so the decoder enforces this property. Reuse the same validator for Lua config and IPC instead of allowing their schemas to diverge.

Closure cases: inject one unknown field into every Action variant and assert typed `invalid_params` without handler invocation; preserve valid minimal representations; reject wrong/missing/duplicate fields and unknown names; include payloads that look like future options on `quit`/`lock`/`spawn`. Validate through both `Request::validate` and the full socket path, plus the config loader's decoder. Re-run the preserved probe on the integration commit; its current assertions deliberately accept the defect.

## Desktop and release closure matrix

The following obligations already exist broadly in the plan. They are **unrun qualification**, with no closure supplied by the transport tests. Each needs an owned implementation issue and a build-specific evidence record before the corresponding release claim. They extend B03/B06/B07/B08/B11/B12/B15/B16/B21 in [the blocker register](../13-production-audit.md); these audit-local labels do not create or close numbered project cases.

| Qualification area | Existing requirement and missing proof | Concrete closure procedure and oracle |
| --- | --- | --- |
| IPC during lock, pending lock and unlock | [Desktop/security](../08-desktop-security.md):19–23 and [resource policy](../17-resource-security.md):21 require current authorization. Transport knows only UID and request shape. | Publish a per-method/action policy for each lock state. Exercise focus/pan/move/overview/spawn/reload/quit, config diagnostics and any future capture command during each transition. Observe state mutation, visible pixels and input destination. Denial must be typed; permitted session termination must return to the qualified session-manager security state. No test may infer safe lock merely from a boolean. |
| Config helper and IPC worker lifetime | [Resource policy](../17-resource-security.md):44–52 requires hard parent deadlines, fd/env allowlists and reaping. Socket nonblocking mode does not implement a config supervisor. | Hold helper output pipe open, return a partial oversized frame, exit without result, exceed instructions and native execution time, trigger reload storms, disconnect requester during reload, then shut down parent. Assert bounded children/fds, reaped helpers, correct generation, last-good config and continuing lock/input progress. |
| Environment, launcher and nested isolation | [Desktop/security](../08-desktop-security.md):7 and [release operations](../19-release-operations.md):23–25 require actual socket environment and supervised readiness. | On every supported session manager, record environment before nested launch, inside child and after exit. Start two instances, terminate one, verify the other and host activation environment remain correct. Native readiness must include app protocol progress and declared device prerequisites. Exercise optional helper crash without destroying unrelated apps. |
| Portal and raw privileged capture boundary | [Desktop/security](../08-desktop-security.md):27–31 and [resource policy](../17-resource-security.md):7–21 distinguish consent-bearing portal sessions from trusted raw host access. | Test a real browser output and window share, cancellation, revocation, source disappearance, resize, slow PipeWire consumer, lock pending, lock client crash and unlock. Verify exact black/no-cursor/no-lock-UI frames for retained lock streams. Separately test tagged sandbox denial through supported sandbox provenance, including already-bound objects. A same-UID IPC socket is not a sandbox boundary. |
| X11 and desktop companion lifecycle | [Desktop/security](../08-desktop-security.md):35–37 and [release operations](../19-release-operations.md):33 require a selected, qualified bridge. No bridge is qualified by IPC DTOs. | Pin exact bridge release and test DISPLAY publication after handshake, bridge crash/restart, clipboard/DnD, transient and override-redirect windows, grabs, fullscreen and scaling. Observe whether existing X11 app connections are lost; never claim transparent restoration. Record which component owns helper backoff and termination. |
| Shutdown, failure after locking, and fd exhaustion | [Desktop/security](../08-desktop-security.md):60 and [release operations](../19-release-operations.md):35–41 require bounded cleanup and a realistic fatal policy. | In disposable session environments inject IPC fd pressure, compositor SIGTERM, compositor crash while locked, helper crash and startup failure after socket bind. Record return to greeter/VT, seat ownership, app connection loss, child cleanup and stale socket behavior. Do not perform physical-session destructive experiments on an active personal desktop. |
| Diagnostic privacy and supportability | [Desktop/security](../08-desktop-security.md):58 and [release operations](../19-release-operations.md):47–59 require redacted reports and real artifact retention. Current CLI can print arbitrary handler JSON. | Define a typed export schema that separates explicit local inspection from a shareable redacted report. Feed titles/paths containing secrets, terminal escape sequences, newlines and huge strings. Verify exported logs and bundles redact by default, bound data, and do not interpret terminal control sequences in human-readable views. Record optional sensitive-data consent in the product flow. |
| Build, compatibility and ongoing maintenance | [Release operations](../19-release-operations.md):15–19 and 57–59 require reproducibility, source/protocol provenance and named maintenance. A passing local test is insufficient. | Run clean locked builds on the declared distro/architecture matrix; verify ABI dependencies, license notices/SBOM, advisory DB revision and reviewed findings. Preserve IPC golden vectors, migration/deprecation policy, package rollback, vulnerability contact, artifact hashes, reviewer and environment ID. Signoff must bind to the exact released source and invalidate affected results after fixes. |

## Required adversarial test expansion

The current tests establish narrow properties. They do not exercise every boundary claimed by the broad design. Preserve the twelve passing tests, add regression tests for observed findings, and introduce the following external cases with real or synthetic clients. Mark each result `pass`, `fail`, `blocked` or `not_run`; an unrun procedure is not evidence.

| Test family | Required dimensions | Evidence to preserve |
| --- | --- | --- |
| Framing and parser | All prefix split positions, every byte split for small corpus, coalesced frames, zero/max/max+1 lengths, truncated EOF, invalid UTF-8, escaped duplicate keys (`a` and `\u0061`), duplicate keys in nested arrays/maps, max depth with envelope, trailing JSON, number overflow, unknown fields, IDs at 0/1/128/129 UTF-8 bytes | Seed/corpus, raw request/response bytes, exit reason, dispatch count, memory bound and timeout |
| Request admission | Unknown method/version with valid ID, invalid parameter type, concurrent repeated ID before/after flush, 32/33 connections, sustained reconnects, tiny-request floods and fair service across clients | Connection/queue counters, error codes, log rate, per-client progress and input/lock timestamps |
| Slow paths and errors | Partial body never finishes, trickle body, continuously progressing backlog, blocked writes, active queue older than five seconds, EINTR, broken pipe, EMFILE and failure after bind | Bounded elapsed time and syscall attempts, dropped/retried work, cleanup, no compositor termination from broken pipe |
| Mutation delivery | Applied mutation followed by duplicate/malformed/oversized request; connection closes before acknowledgement; rejected command followed by valid query; cross-client sequence ordering | Core state trace, response sequence, operations/status policy, retry result and count of side effects |
| Snapshot/event consistency | Mutation during snapshot, pagination and retained snapshot expiry, config generation transition, multi-output ordering, restart epoch, subscription overflow and resync | Atomic snapshot identity, event range and gap detection, stale-ID rejection; mark subscription cases blocked until feature exists |
| Query data and privacy | Huge title/config/diagnostics; geometry containing invalid/nonfinite values at internal boundaries; private paths and lock-sensitive state; unknown future error codes | Bounded serialized bytes and construction RSS, typed rejection, privacy policy applied, compatible-client behavior |
| Client failure modes | Missing socket, wrong permissions, wrong UID, not-ready response, server closes prefix/body early, wrong response ID/version, invalid result/error combination, stdout broken pipe and no socket environment | CLI exit code, wall time, stdout/stderr, no panic/backtrace for ordinary failure, no implicit retries |
| Faulted session integration | Native startup failure, two nested instances, stale socket after crash, parent termination during active commands, lock-client death, capture revocation, bridge crash and display hotplug | Exact build/environment/companion versions, device and process ownership, security state and user recovery path |

The published documentation requires parser/lifecycle fuzzing and 24-hour churn in [release operations](../19-release-operations.md):49–53, but the transport snapshot has no fuzz harness, continuous protocol conformance corpus, or measured soak evidence. Add a deterministic model for decoder/request lifecycle, preserve minimized failing inputs, and run an isolated process-based socket-fuzz/soak workload. A cargo unit-test pass does not substitute for this evidence.

## Proposed execution order and release boundary

1. Reproduce IPC-A01/A02/A03/A13 against the exact integration revision and add invariant-based regressions. Fix the behavioral defects before claiming IPC v1 stable. The preserved probe must stop asserting the defective behavior after fixes.
2. Freeze action admission, accepted/completed semantics, sequence/restart epochs and ambiguous retry policy. Specify subscriptions and pagination before binding a public stable client API.
3. Add bounded serialization, resource telemetry, global scheduling policy, reason diagnostics and failure cleanup. Measure the limits on supported hardware while input, frame scheduling and lock transitions remain active.
4. Qualify config supervision, session launcher, lock, capture, bridge and diagnostics with the desktop matrix above. These remain separate claims with separate evidence.
5. Complete release/package/support matrices, advisory/license/provenance review, fuzz/soak evidence and reviewed case results. Conduct the requested external Claude review on a frozen revision and preserve its inputs, findings and dispositions. This document does not claim that review occurred.

No requirement is closed merely because a remedy is written here. Every closure record must identify the implementation commit, command/procedure, exact environment, raw observations and artifacts, hash, reviewer, and any support limitation. Re-run affected probes after merging parallel branches; source-level correctness in an isolated worktree cannot prove integration behavior.
