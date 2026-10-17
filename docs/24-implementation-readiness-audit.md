# Production readiness audit of the initial implementation

Historical initial-workspace audit of snapshot `c2abdb5`, with the recorded review disposition appended. Later implementation has changed the code: see the [nested runtime milestone](development/runtime.md) for current behavior and qualification limits. Findings here remain evidence about their named snapshots, not an assertion that every original defect is still present.

Date: 2026-10-17. **Production release is blocked.** This audit covers the current source and executable checks, in addition to the earlier design-only audit. A compiling compositor bootstrap, tested pure layout model, config helper, and IPC transport are useful foundations. They do not demonstrate a working integrated desktop. The evidence bundle in [production-audit-2026-10-17](evidence/production-audit-2026-10-17/) records actual commands and source hashes. Historical review reports are preserved unchanged.

## What evidence establishes

| Evidence | What it establishes | What it does not establish |
| --- | --- | --- |
| `cargo check -p fluxwm` against the pinned Smithay revision succeeded | Selected bootstrap Rust source type-checks with the resolved dependency graph on this machine | Linking, rendering a client, clean installation, native DRM, input correctness or a passed phase |
| Core/layout/IPC/config tests, where explicitly recorded in the command bundle | The named deterministic unit or Unix-process scenarios ran on the recorded source | Integration into the compositor, full protocol conformance, hardware or hostile-client qualification |
| Code inspection of `crates/fluxwm` | Identifies concrete incomplete paths and hazardous assumptions | Does not prove that every `unwrap` is externally reachable or that an exploit exists |
| `tools/check_docs.py` | Local document links, case/schedule agreement, historical source/probe record consistency and explicit Lua fixture checks | Runtime global/version inventory, XML authentication, all anchors, all prose or production completeness |
| Independent Claude report | The scope and findings the reviewer actually inspected | A certification, independent execution of author tests, or coverage of uninspected code |

The [protocol inventory](evidence/protocol-inventory.json) is a historical proposed inventory. Its `not-implemented` and null advertised-version fields must not be read as a live probe of the bootstrap. Derive a separate runtime inventory by connecting a synthetic registry client, recording actual globals and bound versions, then exercising each advertised request. Merely declaring a Smithay handler trait is insufficient.

## Immediate blockers with source evidence

| ID / priority | Observation | Required closure |
| --- | --- | --- |
| P01 / high | `crates/fluxwm/Cargo.toml` depends on core and Smithay; `main.rs` initializes neither layout, config nor IPC | Wire one authoritative model into lifecycle, scene, commands and snapshots. Exercise a real nested client through map, focus, configure, close, null-buffer unmap and remap; prove query state agrees with the scene |
| P02 / high | `handlers/xdg_shell.rs::new_toplevel` inserts the toplevel into `Space` at role creation, and `XdgShellHandler::grab` remains a TODO | Keep role ownership separate from mapped-buffer membership; implement nested popup grabs and destruction. Pass SURF-02/03/10/11 with actual protocol traces |
| P03 / high | `state.rs::Fluxwm::new` uses `XdgShellState::new`; bootstrap overrides no fullscreen/maximize/minimize/window-menu handlers | Capability-gate to actual behavior; protocol version and optional event support are separate facts. Capture capability events and every advertised request; see layout audit for pinned upstream constructor evidence |
| P04 / high | `winit.rs` unconditionally requests another redraw after each frame, assumes a nominal 60 Hz, and unwraps bind/render/submit results | Add damage/animation/frame-demand scheduling, backend-error policy, zero-size handling and measured host timing limits. Qualify RUN-02/03; sending a frame callback does not prove physical presentation |
| P05 / high | `input.rs` drops relative pointer motion, assumes an output exists for absolute motion, and forwards every keyboard event | Add device/state-specific handling and compositor actions with key press/release ownership. Qualify outputless state, grabs, popup routing and held-key transitions before a native input claim |
| P06 / high | `main.rs` mutates process `WAYLAND_DISPLAY` using unsafe `set_var`, silently ignores child-spawn failure and defaults to an absent-on-this-machine `weston-terminal` | Pass display only to explicitly launched children; provide explicit backend/argv/readiness/exit policy and actionable errors. This process-local mutation is not evidence that host activation environment changed |
| P07 / high | `state.rs` unwraps listener/client insertion/dispatch; no per-client admission policy appears in bootstrap | Separate startup failure from client-local failure, bound service work and object budgets, and test fairness/resource pressure. A library-level protocol check does not establish application-level quotas |
| P08 / high | No native DRM/seat path, lock/capture/portal/X11/IME/accessible shell or release packaging is present in the bootstrap | Keep B03–B21 open. Demonstrate each route on the declared support matrix; never advertise a global solely because the dependency supports it |
| P09 / medium | Existing README, phase-1 introduction and interface preamble still described an empty implementation | Correct current entry points and label historical records. Keep design contracts separate from executable API docs; link exact crate documentation |
| P10 / high | Source inspection and IPC reproductions identify transport defects; pure layout and config have unresolved integration contracts | Track the subsystem audits below. Passing component tests do not close findings outside their covered scenarios |

These priorities concern readiness impact, not CVSS scores. The bootstrap is development code; this document does not claim a security exploit or recommend running it as a primary session.

## Subsystem audits

- [Layout, rendering, input and window lifecycle](development/layout-production-audit.md).
- [IPC, desktop security and release boundaries](development/ipc-production-audit.md).
- [Lua schema, helper isolation and reload boundaries](development/config-production-audit.md).

Each audit separates observed checks, source-derived findings and unrun qualification. Defects remain open unless a linked patch and scenario result close them. A test reproducing a defect does not turn its corresponding acceptance case green.

## Missing engineering contracts added by this audit

**Event-loop fairness and time.** Per-connection limits multiply by the maximum connections and decoded-object expansion. Bound a whole turn as well as each peer; avoid an always-readable source starving input, timers or device handling. Use monotonic absolute deadlines for partial messages, helper lifetime, source acquisition and CLI transactions. Define suspend/resume behavior separately from wall-clock changes. Ratify numerical service-latency budgets against measured hardware under B14; an unchosen budget blocks qualification. RUN-01/05/08 cover these boundaries.

**Nested development lifecycle.** Treat host-window zero size, minimization, close, host connection loss and renderer failure as explicit states. Publish readiness after clients can make progress, not after creating the socket. Maintain an instance-specific socket ownership guard and cleanup after partial startup. Handle SIGTERM through an event source and reap only owned children; do not call complex cleanup from a signal handler. Native crash recovery while locked still requires the separate physical/session-manager qualification. RUN-03/04 and OPS-04/09 distinguish these claims.

**Coordinate domains.** Window geometry, surface-local input regions, subsurface offsets, buffer transform/viewport, output logical coordinates, fractional scale and physical pixels require explicit conversions. Negative output origins and odd buffer dimensions expose rounding mistakes that integer-scale tests miss. Tiny/oversized buffers must not relax acquire/read ownership. GPU-10 and INP-09 require images and independent coordinate oracles, beyond pure layout hit testing.

**Protocol and compatibility evidence.** Test generated globals through a registry client at both oldest supported and maximum advertised versions. Maintain request coverage and optional capability/state-event coverage separately. Exercise unsupported requests according to their protocol behavior without claiming the feature works. Real GTK/Qt/Electron and X11 applications supplement synthetic cases; screenshots alone do not establish serial/ack/focus correctness. SURF-10/11/12 cover capability, lifecycle and activation gaps.

**Failure injection.** Device test-only commits and real commits can fail at different points. A tested fallback must preserve last confirmed ownership and clear unsafe planes; retain leases until completion on failed scanout transitions. Test allocator/fd pressure, queue overflow, destination death and shutdown during work in disposable environments. GPU-11 extends existing buffer and output cases rather than replacing them.

**Release evidence freshness.** A release result identifies source, lockfile, enabled features, native dependencies, actual advertised globals, environment, command, expected oracle and accessible raw artifacts. A dependency/feature change invalidates affected qualification even if source line numbers do not move. Keep minimized fuzz seeds and replay commands. OPS-10 requires checking evidence freshness instead of relying on a previous green summary.

## Closure order and explicit unknowns

1. Finish phase-1 artifacts: reviewed license/dependency inventory, locked build, CI, contributor instructions, isolated sample/client run and explicit hardware/test capacity. The current bootstrap check partially addresses B01; it does not close it. The copied Smithay notice partially addresses B02; a complete transitive license inventory remains required.
2. Integrate lifecycle, safe scheduling, input and capability gating in phase 2. Add synthetic protocol clients and a nested smoke harness with automatic shutdown and private display/IPC sockets. Preserve traces and pixel oracles. Do not require a person to replace their host compositor for these checks.
3. Establish native session/device/ownership architecture and physical qualification in phase 3. Keep pure layout/config/IPC work independently testable while their integration is staged in phases 4–8.
4. Follow the existing ten phases and B01–B21 qualification requirements. Implementing an early component from a later phase does not complete either phase or waive earlier gates.

Unknowns include supported distributions/hardware/driver ranges, actual host-present timing observability, secure native failure recovery, companion versions, whole-loop service budgets and maintenance ownership. This audit names closure evidence; it does not infer answers from installed headers, available DRM nodes, test count or reviewer confidence.

The case catalog now specifies **112 full cases and 11 narrower variants**. All remain specified, with no full FluxWM runtime qualification pass. No finite audit establishes that nothing else can be missing; new failures must produce minimized regressions and update the contracts and support matrix.

Bootstrap formatting was normalized after the source inspection. The subsystem audit retains the original source hashes and line references; formatting does not close its behavioral findings. The final command bundle and independent-review snapshot identify the formatted source separately.

## Final local verification before independent review

The recorded assembled workspace passes `cargo check`, **43 component/integration tests**, Clippy with warnings denied, formatting and whitespace checks. The documentation checker passes with 112 cases, 11 variants and ten phase files. The separate IPC defect probe also executes successfully **by asserting the defective behavior**: half-close dispatch count zero, sequence 11 followed by 10, encode/decode depth mismatch, empty config path accepted and four unit actions accepting unknown fields. None of these observations is a production acceptance pass. The `fluxwm` binary has zero tests in this suite and was not launched by these checks.

The first-cycle strip/canvas conversion ambiguity (LP-12) is clarified in the layout contract using explicit initialization flags and stable tie-breaking. That is a documentation decision, not an implementation fix. Other subsystem findings remain open unless explicitly identified as fixed in their audit. Initial formatting and temporarily unresolved cross-document links were corrected before the final checks.

## Independent review additions

[Claude's unedited report](reviews/claude-implementation.md) broadly confirms the audit and the 43-test count, with four additional medium findings. The [disposition](reviews/implementation-disposition.md) records author source checks, precise limits and closure tests. These remain open behavioral work:

| Finding | Source-derived gap | Closure requirement |
| --- | --- | --- |
| N1 | Wayland output events flush only during redraw; making rendering idle can strand events | Flush protocol output independently from rendering. Prove keys/configures reach a client with zero redraws and a measured latency budget |
| N2 | Move/resize assume requested target is in Space; current serial check establishes same client, without mapped-target validation | Make seat/target/location lookups fallible and validate live target policy together with map-on-buffer integration. Exercise a click on A followed by move/resize on unmapped sibling B; do not claim a reproduced current crash |
| N3 | IPC has no readiness/deadline integration API and may retain complete requests past a bounded turn | Expose event sources/readiness, buffered-work rescheduling and next deadlines; test 17-frame silent pipeline, idle expiry and EMFILE backoff without a busy loop |
| N4 | IPC reads more data regardless of complete queued input, eventually reaching staging limits | Define an outstanding-request window and use read backpressure. Verify within-window pipelines and explicit overflow semantics; arbitrary pipeline length is not guaranteed |

The reviewer did not execute tests or verify source hashes and did not inspect pinned Smithay source or validate phase prerequisites. The author separately checked the frozen snapshot and its hashes. Review scope and incomplete coverage remain visible; the review is not a production certification.
