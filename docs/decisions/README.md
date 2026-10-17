# Architecture decisions and risk register

All decisions are proposed until implementation validates them. Changes should record trigger, alternatives, evidence, consequences, and affected phase gates.

## ADR-001: Rust and Smithay

Use Rust for compositor policy and integration, with Smithay for protocol/backend building blocks. Alternatives are direct wayland-rs plus hand-written backends, wlroots FFI, or a fork of an existing compositor. Smithay reduces backend plumbing while keeping policy in FluxWM. Costs include dependency evolution, system-library FFI, and the need to implement desktop policy explicitly. Pin a revision and review upgrades separately. Source reuse requires a deliberate project license decision and preserved notices; an upstream README license label is not a full dependency audit.

## ADR-002: GLES first

Use GLES as the initial production GPU renderer, because the inspected Smithay renderer API documents that path. Vulkan may be evaluated after SDR/GLES succeeds; allocator support alone is insufficient. A Vulkan decision requires a demonstrable import/sync/output path, effect parity, and performance evidence justifying maintenance cost.

## ADR-003: Separate layout and presentation

A pure layout model owns target geometry; scene transforms own animation and hit testing. This enables randomized tests and continuous camera motion without spamming clients with intermediate resize configures. The cost is explicit reconciliation between committed buffers, target geometry, and presentation state.

## ADR-004: Strip plus full canvas

Default to predictable column scrolling and vertical workspaces. Deliver independent two-axis canvas mode in phase 8. A strip alone does not meet the full canvas interpretation; arbitrary freeform canvas alone would lose the requested scrolling tiling behavior. Preserve mode-specific state and unique membership.

## ADR-005: Lua evaluated in a supervised helper process

Compile a returned Lua table into immutable Rust configuration. Avoid runtime callbacks in version 1. A parent-enforced helper-process deadline contains runaway evaluation; instruction/allocation limits and restricted bindings are defense in depth. A Lua 5.4 probe showed hook errors can be caught, so thread-only timeout is insufficient. This is configuration isolation, not a claim of complete hostile-code sandboxing.

## ADR-006: Single authoritative event loop

Keep mutable compositor/protocol state on one thread initially. Workers return owned results. This simplifies lifetime and ordering rules. Consider a render thread only after profiling and a documented synchronization/resource ownership design; threads by themselves are not optimization.

## ADR-007: Integrate desktop services

Use external services for panel/launcher/notifications/authentication and a tested portal/capture stack. FluxWM owns protocol correctness and session integration. Choose X11/capture companions using phase 7 spikes rather than assume niri's selected integrations apply unchanged.

## Open decisions with deadlines

| Question | Deadline | Evidence needed |
| --- | --- | --- |
| Exact Smithay/toolchain/protocol revisions | Phase 1 | Working nested build, API compatibility, required fixes |
| Project license and source reuse | Phase 1 | Dependency/source inventory and chosen redistribution policy |
| Seat implementation and distribution support | Phase 3 | Unprivileged VT, suspend/reactivation tests |
| Xwayland external vs integrated | Phase 7 | Clipboard, dialogs, fullscreen, crash recovery matrix |
| Portal backend reuse vs FluxWM backend | Phase 7 | Browser ScreenCast and Screenshot proof with consent |
| Accessibility protocol/service route | Phase 8 | Tested screen-reader focus/navigation and permission model |
| Explicit sync advertisement coverage | Phase 9 | End-to-end driver and buffer lifetime tests |
| HDR, Vulkan, DRM lease scope | Phase 10 | Capability spike and maintenance/compatibility decision |

## Risks

| Risk | Effect | Mitigation / detection |
| --- | --- | --- |
| Configure/commit race | Incorrect geometry or stalled windows | Transaction epochs, slow-client tests |
| Early buffer release | Corruption or driver errors | Consumer lifetime accounting, sync stress |
| Precision loss on canvas | Jumping or unreachable windows | Anchor coordinates, rebasing, large-position tests |
| Full redraw during scrolling | Latency/battery regression | Damage visualization, visible-count benchmarks |
| Blur invalidation mistakes | Trails or excessive GPU cost | Background dependency tests and bounded passes |
| Lock helper failure | Exposure of private content | Compositor safe cover, failure injection |
| Capture bypasses policy | Sensitive pixels exposed | Shared capture authorization path and lock revocation tests |
| Lua runaway/native block | Frozen desktop | Supervised exec helper, parent deadline, restricted functions, budgets, rollback |
| Driver differences | Vendor-specific failures | Recorded hardware matrix, capability negotiation |
| Dependency drift | Build or protocol breakage | Exact pins and dedicated upgrade validation |
| Desktop scope growth | Delayed usable session | Explicit companion contracts and gated optional work |
| Accessibility assumed from toolkits | Unusable navigation | Actual assistive stack integration demonstration |

Review the register at every phase exit. A documented risk does not satisfy its required mitigation or acceptance gate.

## Deeper-audit authority

The [production blocker register](../13-production-audit.md) refines the older decision deadlines above with closure evidence. The source manifest is an immutable research snapshot, not a validated dependency selection. Numeric scheduling/resource defaults remain proposals until measured. No deadline permits omitting a foundational security or buffer-lifetime contract until a late phase.

ADR-008 selects compatible trusted-host/restricted-sandbox model A consistently across Wayland, IPC and config. ADR-009 retains capture streams with opaque black during lock and separates real terminal revocation. ADR-010 withdraws unsupported effort forecasts. Detailed contracts and staged blocker ownership are in [review resolutions](../21-review-resolutions.md) and [production audit](../13-production-audit.md).
