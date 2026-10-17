# Implementation contracts for the first Rust milestones

These are required design contracts and conceptual interfaces. Some core/config/layout/IPC APIs now exist, but the bootstrap has not integrated them; consult [development docs](development/README.md) and the [current audit](24-implementation-readiness-audit.md) for executable behavior. Use the [architecture](02-architecture.md), [ordering rules](14-state-machines.md), and [resource policy](17-resource-security.md) as constraints when selecting actual types against a pinned Smithay revision.

## Identity and state boundaries

Use distinct newtypes, not interchangeable integers, for `WindowId`, `WorkspaceId`, `OutputId`, `SurfaceCommitId`, `ConfigGeneration`, `LayoutEpoch`, `PermissionEpoch`, and `CaptureSessionId`. A generational ID contains slot plus generation and is invalid after slot reuse. Persistent workspace labels and EDID hints are not authority-bearing runtime IDs.

Conceptual pseudocode:

```rust,ignore
struct FrameIdentity {
    output: OutputId,
    output_epoch: OutputEpoch,
    scene_revision: SceneRevision,
}

struct CommitReadiness {
    commit: SurfaceCommitId,
    layout: LayoutReadiness,
    acquire: AcquireReadiness,
}

struct CaptureAuthorization {
    principal: PrincipalId,
    source: CaptureSourceId,
    permission_epoch: PermissionEpoch,
    expires: PermissionLifetime,
}
```

The actual buffer consumer guard must be backend-specific and cannot simply derive `Copy`. Its destruction must not imply GPU completion unless the guard has retained ownership until completion. Encode renderer-thread/device affinity using the actual library types and documented unsafe boundaries; do not make arbitrary texture handles `Send` to satisfy a generic queue.

A command returns accepted/rejected with typed reason and resulting state sequence. Acceptance means validated intent was queued/applied, not that hardware presentation already completed. Methods that require completed device state return an operation ID and later completion/failure event. The CLI must distinguish these states.

## Startup and readiness sequence

1. Parse CLI paths/backend selection and determine full-session versus nested mode. Do not import any activation environment yet.
2. Establish bounded diagnostics, monotonic clock, runtime directory ownership/permissions, unique instance identity, and socket creation strategy. Reject unsafe runtime directory ownership or stale socket collisions explicitly.
3. Start config helper with safe built-in defaults already available. Bad config cannot remove the recovery terminal/reload path. Helper must receive the exec-only fd/environment/limit allowlist from the [review resolutions](21-review-resolutions.md).
4. Create Wayland server/protocol state and core model, then selected backend/session/device resources. Register globals using the declared backend capabilities and privilege filters.
5. Probe output state, initialize renderer resources, and establish valid initial output or documented outputless policy. If any startup step fails, unwind only owned resources and return session control.
6. Publish compositor readiness only when ordinary clients can connect and make protocol progress. Full native session readiness additionally requires its declared display/input prerequisites. A socket existing is not sufficient proof.
7. Full-session launcher exports actual display/instance environment and starts essential companions with explicit readiness. Start optional companions independently; publish degraded state when required integration is missing.
8. On first usable frame, provide accessible startup feedback when configured. Record effective capabilities and config generation. Never report a production-ready desktop solely because the main loop entered dispatch.

For a display-manager session, define whether the launcher or compositor is the supervised session leader and what exit codes mean. A nested process must not start system-wide or per-user host graphical-session targets. Tests use unique display/IPC sockets and cleanup guards.

## IPC v1 wire contract

Proposed frame layout: exactly four big-endian bytes containing payload length, followed by that many UTF-8 JSON bytes. Maximum public payload is 1,048,576 bytes; the prefix is additional. Reject zero-length frames and unsupported versions. Count partial-read buffer memory against per-connection admission limits and apply a bounded read timeout policy. The config helper transport is separate and can have a different documented ceiling.

Example request and success response:

```json
{"version":1,"id":"req-17","method":"windows.list","params":{}}
```

```json
{"version":1,"id":"req-17","state_sequence":"42","result":{"windows":[]}}
```

Example error:

```json
{"version":1,"id":"req-18","state_sequence":"42","error":{"code":"permission_denied","message":"Action is unavailable while the session is locked."}}
```

IDs are bounded strings, maximum 128 UTF-8 bytes, unique among outstanding requests per connection. There is no arbitrary shell-command method; `spawn` accepts explicit argv under local-control policy. Return structured unknown-method, unsupported-version, invalid-params, stale-object, permission-denied, busy, and device-operation-failed errors. JSON duplicate object keys and excessive nesting are rejected rather than accepted with parser-specific precedence.

Queries return an internally consistent snapshot sequence; subscriptions start from a declared sequence or require resync. Events contain event type, sequence, config generation where relevant, and payload. If a client misses events, it refetches a full snapshot; it cannot infer current object validity from a stale index. Numeric IDs/large counters must have a JSON representation that does not lose precision in common clients; use bounded decimal strings for 64-bit identities/counters where necessary and specify this consistently.

## Layout command semantics

Resolve command targets by live IDs plus current permission/state before mutation. `focus direction` changes focus and requests minimal reveal; `pan` changes camera and does not implicitly focus arbitrary content; `move direction` changes membership/order/placement and preserves unique ownership. `send-to-workspace` may preserve source focus history while `follow-to-workspace` explicitly transfers active view. Define these as separate actions instead of relying on accidental side effects.

Maximize is proposed to occupy the current output's **usable** area while retaining restore geometry; fullscreen occupies the physical output logical extent according to protocol and panel policy. Neither is identical to setting a width fraction. For strip mode, store maximize as an explicit state with a position/restore policy, and test focus scrolling away and back. In canvas mode it is an output-relative presentation state with saved world placement, not a destructive rewrite of world coordinates. Final toolkit negotiation semantics must pass xdg-shell tests in phase 4/7.

For serial-triggered move/resize/activation, validate seat/client/context using protocol mechanisms. IPC control follows its own trusted channel policy and cannot fabricate a client serial. Pointer-driven operations retain source surface/output generations and cancel predictably on destruction or lock.

## Rendering and picking snapshots

A scene snapshot owns immutable transformed geometry, clips, surface-tree mapping, and element identity. The renderer creates a pending frame from one snapshot; output presentation advances the visible snapshot when its actual completion semantics justify it. Input picking chooses the coherent output-visible snapshot, then applies popup/grab/lock overrides. At nested-backend timing limitations, document whether submitted or host-presented state is observable and test for a bounded mismatch.

There may be a pending snapshot for each independently scheduled output. Avoid one global latest scene causing clicks to target a future frame on a slow monitor. Retiring a snapshot only frees its CPU geometry immediately; GPU/client resources still follow their own consumer ledgers.

## Phase issue template

Every implementation issue names an artifact, upstream revision/API surface, invariant, adversarial case IDs, expected external behavior, verification environment, and blocker closure. For example, implementing syncobj registration is incomplete without acquire-blocker integration, release-consumer tracking, capability gating, object destruction, and tests on supported devices. Implementing a Lua parser is incomplete without helper containment, raw bounded conversion, stale generation handling, diagnostic output, and bad-config recovery.

Do not replace these external contracts with tests that merely repeat private function return values. Keep a trace/replay seed for lifecycle defects so regressions survive refactoring.

## Close, hung clients, and hidden windows

`close-window` requests graceful protocol closure of the chosen live toplevel (or the selected X11 bridge's equivalent). It is not an OS process kill. A nonresponding application remains navigable and does not stall the compositor. A separate explicit `disconnect-client` recovery action targets a validated live Wayland connection, reports that other windows on that connection can be lost, and is never inferred from app ID/title/PID metadata. Any OS-process termination feature requires its own trustworthy process-ownership and user-intent contract.

Define minimize as an explicit hidden window state with stored workspace/container placement and a recoverable task/search entry. A client minimize request is subject to compositor policy and protocol semantics; it cannot cause irreversible removal from membership. Hidden tiles stop ordinary input eligibility, preserve restoration geometry, and follow a documented frame-callback/capture policy. Restore reveals the saved tile/group and selects focus according to user intent. Test close refusal, multiple windows on one client, hidden fullscreen/floating windows, parent dialogs, and restore after output removal. Include close/restore in all navigation modes and accessible UI actions.

The [XDG state matrix](22-window-state-matrix.md) is authoritative for client flags/capabilities/bounds, backdrop and visibility. [Review decisions](21-review-resolutions.md) select host trust, retained black capture sessions, helper process limits and asynchronous buffer guards. These refine the broad types above; they do not assert that any backend path is implemented.
