# Ten-phase implementation roadmap

This roadmap implements the complete requested FluxWM experience in **exactly ten phases**. Each linked work package provides dependencies, tasks, artifacts, verification, and an exit gate. The documents describe future implementation work; none of these implementation gates have passed yet.

| Phase | Deliverable | Dependencies | Scheduling evidence |
| --- | --- | --- | --- |
| [1. Foundation](phases/01-foundation.md) | Pinned workspace/development and source baseline | None | Not estimated; contract breakdown required |
| [2. Minimal compositor](phases/02-minimal-compositor.md) | Clients, lifecycle and input | 1 | Not estimated; contract breakdown required |
| [3. Native GPU backend](phases/03-gpu-backend.md) | Seat/DRM/GLES, primary plane, font spike | 2 | Not estimated; contract breakdown required |
| [4. Scrolling tiling](phases/04-scrolling-layout.md) | Columns/workspaces/window states/scale | 2; 3 for hardware | Not estimated; contract breakdown required |
| [5. Animation and frame pacing](phases/05-animation.md) | Motion, clocks, shaped text and UI semantics | 3, 4 | Not estimated; contract breakdown required |
| [6. Lua configuration and IPC](phases/06-lua-ipc.md) | Supervised helper, typed config and CLI | 4, 5 | Not estimated; contract breakdown required |
| [7. Desktop compatibility](phases/07-desktop.md) | Shell/security-context/lock/capture/X11/IME | 3–6 | Not estimated; contract breakdown required |
| [8. Full canvas and accessibility](phases/08-canvas.md) | 2D canvas, search/overview, AT-SPI qualification | 4–7 | Not estimated; contract breakdown required |
| [9. Optimization and GPU qualification](phases/09-optimization.md) | Explicit sync, multi-GPU, client scanout, W11 | 3, 5, 7, 8 | Not estimated; contract breakdown required |
| [10. Hardening and release](phases/10-release.md) | Packages/support matrix/runtime evidence | All previous | Not estimated; contract breakdown required |

**No defensible total duration is available.** The earlier 38–76 person-week range is withdrawn following independent review. Phase 1 must estimate per protocol/backend/desktop contract and infrastructure work; re-estimate after phases 3 and 7. Hardware access, driver matrix, VKMS/WLCS/fuzz/soak infrastructure, latency measurement equipment, accessibility testing and ongoing maintenance are separate cost/work items with owner assignments. Do not infer a schedule from documentation length.

## Delivery rules

Each task becomes an issue with owner, dependency IDs, artifact, and acceptance evidence. Gates require reproducible records, not just a demo video. A phase can be explored early, but it cannot be marked complete before its dependencies and own gate pass. Fixes to earlier phases remain part of their invariants.

The end of phase 4 is an internal scrolling-compositor milestone. Phase 7 is an internal desktop-integration milestone, still requiring canvas, optimization, and release hardening. Phase 10 completes the release target only if the [traceability table](11-traceability.md) has evidence for every required capability.

## Cross-cutting work

Security, diagnostics, accessibility, resource lifetimes, and tests start when their first relevant feature is introduced. They are not deferred until phase 10. Keep an architecture decision log and a capabilities inventory updated in every phase. Gate failures create concrete remediation tasks and remain visible.

## First implementation session

Read phase 1, choose the exact Smithay revision and Rust toolchain, run the pinned upstream nested sample, and record the result. Then create only the initial Rust workspace and a pure state-model skeleton. Do not begin with custom shaders, broad plugin APIs, or a Vulkan rewrite before a client can map and a GPU-backed output can present reliably.

## Production audit dependency refinement

All phases retain their numbering. Each work package now includes blocker IDs and adversarial case coverage. Buffer acquisition/release and helper trust architecture start before their later optimization/integration qualification. Config evaluation uses a supervised helper process, not a thread-only timeout. The [blocker register](13-production-audit.md) identifies unresolved decisions and evidence required; source snapshots and prose estimates cannot close them.
