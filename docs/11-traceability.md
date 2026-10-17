# Requirements and completion evidence

This table audits the **documentation deliverable** requested for this repository. Implementation phases remain planned; their future acceptance evidence is specified in the linked documents. The request is to research and plan building FluxWM, not to claim that this documentation constitutes a working compositor.

| User requirement | Design evidence in this folder | Implementation phases | Future proof required |
| --- | --- | --- | --- |
| Own FluxWM compositor | [Product](00-product.md), [architecture](02-architecture.md) | 1–10 | FluxWM executable, ownership/policy code, release journey |
| Rust implementation | [Crate boundaries and dependency policy](02-architecture.md) | 1–3 | Pinned Cargo workspace builds and runs |
| Wayland foundation | [Protocol matrix](07-protocols.md), [research](01-research.md) | 2–3, 7 | Native clients, lifecycle/conformance and hardware tests |
| Scrolling tiling like niri | [Strip algorithms](03-layout.md) | 4–5 | Stable widths, camera reveal, columns/stacks/tabs navigation |
| Infinite canvas and scrolling | [Two-axis canvas and coordinates](03-layout.md) | 4, 8 | Independent x/y placement/pan, extreme coordinates, recovery |
| Animations inspired by Hyprland | [Motion engine](05-animation-input.md) | 5, 9 | Spring/Bézier, continuous retarget, mixed refresh, reduced motion |
| Lua configuration | [Schema/reload/security](06-lua.md), [example](examples/config.lua) | 6, 8 | Example parses, atomic reload, limits and rollback tests |
| GPU support | [Rendering and synchronization](04-rendering.md) | 3, 9 | GLES/DRM, DMA-BUF, explicit sync, hardware qualification |
| Optimization | [Budgets and workloads](09-performance-testing.md) | 5, 9–10 | Raw percentile/idle/memory results on recorded hardware |
| Complete window manager experience | [Feature scope](00-product.md), [desktop integration](08-desktop-security.md) | 4, 7–10 | Apps, shell, X11, IME, sharing, lock, suspend, accessibility |
| Detailed web research and references | [Source register and caveats](01-research.md) | 1 and every upgrade | Exact dependency/source inventory refreshed during implementation |
| Documentation folder | [Index](README.md) and linked subsystem specifications | Documentation delivered now | Files present and internal links resolve |
| Exactly ten implementation phases | [Roadmap](10-roadmap.md), [phase work packages](phases/01-foundation.md) | Exactly 1–10 | Each phase has tasks, artifacts, verification, gate |
| Extreme detail for implementation | Architecture, algorithms, lifetime rules, protocol matrix, Lua contract, risks, tests, phase tasks | All | Review evidence at phase exits; add API detail against pinned code |

## Documentation audit method

Inspect file contents, not merely filenames. Confirm each phase includes dependencies, tasks, required artifacts, verification, and a gate. Resolve all relative Markdown file links. Check the Lua example's basic syntax when an interpreter is available; distinguish the helper schema tests now available from unqualified compositor reload integration. Check that source-backed upstream facts link to relevant primary documentation and that targets are not presented as benchmark results.

## Implementation evidence ledger template

During implementation, keep one record per requirement: requirement ID, feature/build revision, phase, exact environment, scenario/test name, command, observed result, raw artifact location, limitations, reviewer/date. This document currently supplies the mapping and proof obligations, not fabricated implementation results. A checkbox or green unit test cannot alone prove full desktop readiness.

## Deeper production-plan traceability

| Additional requirement | Contract | Evidence status |
| --- | --- | --- |
| Immutable upstream evidence | [Manifest](evidence/upstream-sources.json), [findings](18-source-findings.md) | Retrieved source bytes/hashes; bootstrap compiles, complete phase-1 qualification remains open |
| Hard config execution boundary | [Resources/helper](17-resource-security.md), [Lua probe](evidence/lua54-probe.txt) | Standalone Lua behavior observed; helper/component evidence is recorded in the current audit |
| No layout timeout bypasses acquire | [State machines](14-state-machines.md) | Required design; GPU runtime tests unrun |
| Revocable privileged capabilities | [Trust policy](17-resource-security.md) | Required design; bound-object tests unrun |
| Complete integration contracts | [Desktop contracts](16-integration-contracts.md) | Source-informed plan; portal/X11/assistive spikes open |
| Explicit failure/edge oracles | [112 cases](15-edge-cases.md) | Specified only, machine-readable statuses preserved |
| Incident and release readiness | [Operations](19-release-operations.md) | Planned; no release artifact/runtime qualification |
| Independent Claude review | [Review records](reviews/README.md) | See actual review output and disposition; never a production certification |
