# FluxWM engineering documentation

Research date: **2026-10-17**. This folder specifies the product, architecture, algorithms, integration work, and exactly ten implementation phases. It is the handoff for building FluxWM; it does not claim the compositor is already built.

## Read in this order

| Document | Purpose |
| --- | --- |
| [Product and requirements](00-product.md) | User experience, feature scope, measurable completion criteria |
| [Research and source register](01-research.md) | Verified upstream findings, primary sources, reading order, uncertainty |
| [Rust architecture](02-architecture.md) | Crates, ownership, event loop, state transitions, dependency policy |
| [Infinite canvas and scrolling layout](03-layout.md) | Columns, 2D canvas, coordinates, navigation, transactions, edge cases |
| [GPU rendering and output management](04-rendering.md) | GLES, DRM/KMS, DMA-BUF, synchronization, multi-GPU, effects |
| [Animation and interaction](05-animation-input.md) | Springs, gestures, frame pacing, input, accessibility |
| [Lua configuration contract](06-lua.md) | Proposed schema, validation, reload, limits, bindings, rules |
| [Wayland protocol matrix](07-protocols.md) | Protocol priorities, implementation obligations, acceptance scenarios |
| [Desktop integration and security](08-desktop-security.md) | Locking, portals, X11, session lifecycle, IPC, recovery |
| [Performance and verification](09-performance-testing.md) | Budgets, workloads, test layers, hardware matrix, release gates |
| [Ten-phase roadmap](10-roadmap.md) | Dependency order and links to ten detailed work packages |
| [Decisions and risks](decisions/README.md) | Architecture decisions, unresolved choices, risk mitigation |
| [Example Lua configuration](examples/config.lua) | Complete proposed configuration example |
| [Requirement traceability](11-traceability.md) | Requirement → design → phase → verification |
| [Initial documentation audit](12-documentation-audit.md) | Historical first-pass checks and limits |
| [Production audit and blockers](13-production-audit.md) | Evidence classification, corrections, unresolved implementation gates |
| [State machines](14-state-machines.md) | Surface, buffer, output, lock, capture, and config ordering |
| [112 edge cases](15-edge-cases.md) | Stimuli, expected results, and future verification oracles |
| [Integration contracts](16-integration-contracts.md) | Portal/X11/IME/accessibility/text/session obligations |
| [Resources and privilege](17-resource-security.md) | Numeric proposals, helper containment, trust policy |
| [Immutable source findings](18-source-findings.md) | Source commits/hashes and observed API facts |
| [Release operations](19-release-operations.md) | Support matrix, packaging, incident/recovery, evidence bundle |
| [Implementation contracts](20-implementation-contracts.md) | Typed identities, startup readiness, IPC framing, action and snapshot semantics |
| [Review decisions](21-review-resolutions.md) | Selected trust, capture, helper, ownership and infrastructure contracts |
| [XDG window-state matrix](22-window-state-matrix.md) | Client flags, bounds, visibility, fullscreen backdrop, version gates |
| [Current implementation audit](24-implementation-readiness-audit.md) | Source-backed findings, executable evidence and production blockers |
| [Runnable nested milestone](development/runtime.md) | Actual CLI, controls, IPC, lifecycle and rendering evidence boundaries |
| [Runtime findings and remaining gates](development/runtime-findings.md) | Integrated runtime fixes, actual tests and unqualified production behavior |
| [Development documentation](development/README.md) | Implemented component APIs and remaining integration work |
| [Independent review](reviews/README.md) | Claude review, remediation, and verification limits |

## Interpretation

**Upstream fact** means information supported by a linked source. **Design** means a FluxWM proposal chosen here. **Target** means a goal to measure during implementation, not a benchmark result. **Gate** means evidence required before a phase can be considered complete. No upstream project promises FluxWM compatibility.

Start the deeper production review with [gap audit and blockers](13-production-audit.md), then the [expanded case catalog](15-edge-cases.md) and [independent review](reviews/README.md).

The initial workflow uses an unbounded horizontal strip per workspace, with vertically arranged workspaces. The final design also includes a genuine two-axis canvas mode with independent x/y panning. This preserves the requested infinite canvas instead of silently treating a one-axis strip as the entire feature. Infinity means no fixed user-visible layout boundary; memory, protocol coordinates, window count, and GPU resources remain finite.

Use the source register to refresh moving dependencies before implementation. Pin exact revisions in phase 1. Keep protocol XML versions and advertised versions aligned. Numerical budgets are provisional until the first hardware baseline.

## Verification and independent review

Run `python3 tools/check_docs.py` from the repository root (explicit Lua 5.4 tools required). The [review outcome](reviews/final-summary.md) preserves all three Claude reports and remediation limits. [Final verification](23-final-verification.md) distinguishes actual checks from future compositor qualification.
