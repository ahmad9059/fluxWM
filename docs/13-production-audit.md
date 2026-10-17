# Production gap audit and blocking decisions

Audit scope: documentation and upstream source inspection, 2026-10-17. **FluxWM is not production-ready.** This original design audit predates the initial Rust workspace; consult the [current implementation audit](24-implementation-readiness-audit.md) for source findings and build evidence. This audit strengthens a build plan; it does not certify future code. Production readiness is a set of demonstrated contracts on a declared support matrix, never a promise to handle every driver or application.

## Evidence classification

- **Observed:** a command, immutable source file, or current repository state was inspected. See [source manifest](evidence/upstream-sources.json), [local preflight](evidence/local-preflight.json), and [evidence notes](18-source-findings.md).
- **Required design:** the behavior FluxWM must implement. Its tests are specified but currently unrun.
- **Unknown:** no direct evidence resolves the question. Unknowns stay open until the indicated spike/gate produces evidence.
- **Out of initial support:** a capability is explicitly excluded rather than accidentally half-advertised. Required strip/canvas/Lua/Wayland/GPU/desktop functionality cannot be moved into this category to simplify delivery.

## Corrections to the first documentation pass

| Finding | Evidence | Correction |
| --- | --- | --- |
| Moving latest/master links are weak implementation references | Prior docs had no immutable source manifest | Exact repository commits, individual file hashes, and source interface inventory now accompany research |
| Instruction hooks do not guarantee termination | [Lua 5.4 probe](evidence/lua54-probe.txt) and [pinned mlua embedding](evidence/mlua-probe-result.json): five hook errors caught by `pcall` | Use a supervised config helper process with a parent-enforced wall deadline; VM limits remain defense in depth |
| Previous Lua syntax check did not identify interpreter version | Local `lua` is 5.5; explicit `lua5.4` is 5.4.9 | Rechecked the example under explicit Lua 5.4 and saved the result |
| Layout deadlines could be confused with acquire readiness | Smithay syncobj integration requires acquire readiness at surface transaction availability | Separate layout readiness from GPU acquire and presentation completion; a timeout cannot authorize reading an unready buffer |
| Capture/privileged globals lacked a binding policy | Inspected security-context source exposes per-client filtering; app IDs are ordinary metadata | Add explicit trust classes, authorization at object use, and revocation requirements |
| Accessibility was left as an abstract assessment | niri source uses AccessKit and a bounded adapter worker; its docs describe a keyboard-monitor D-Bus interface | Specify compositor UI tree, key-monitor ownership, isolation, permission, and test flows |
| Output identity and rollback were underspecified | No hardware probe or configuration implementation exists | Treat identity as ambiguous, define output epochs and safe preview rollback |
| No numerical abuse limits or oracle catalog | Previous tests were prose workstreams | Add bounded resource policy and concrete case IDs with expected results |

## Blocking register

These are **implementation readiness blockers**, not tool failures blocking documentation work. Roles below are future owners, not people already assigned. A source read proves API presence, not that the integration works.

| ID | Unknown or required decision | Owner / deadline | Evidence to close | Work prohibited until closed |
| --- | --- | --- | --- | --- |
| B01 | Buildable dependency combination and Rust toolchain | Platform / phase 1 | Clean nested sample + FluxWM skeleton build, lockfile, selected protocol binding versions, selected XML provenance (authenticated upstream release or trusted distro inputs with explicit reconciliation; signature/ancestry checks where applicable) | Declaring dependencies validated |
| B02 | Project license and copied-source inventory | Maintainer / phase 1 | Selected project license, dependency notices, origin log | Shipping copied upstream source without review |
| B03 | Seat backend and session lifecycle | Platform / phase 3 | Unprivileged VT deactivate/reactivate/suspend tests | Native session support claim |
| B04 | Complete buffer lifetime accounting | Renderer / phase 3, requalify 9 | Trace each consumer through GPU completion and scanout retirement; sync stress | Enabling effects/capture that drop required leases |
| B05 | Supported protocol versions vs selected library revision | Protocol / every phase | Exact handler inventory + synthetic-client test per advertised request | Advertising unimplemented versions |
| B06 | Capture backend/window-sharing route and authorization | Desktop/security / phase 7 | Real browser output AND window sharing, revocation, source death, lock denial | Claiming full portal screen sharing |
| B07 | X11 integration selection | Desktop / phase 7 | Satellite or integrated matrix: clipboard/dialogs/games/scaling/crash | Publishing X11 compatibility |
| B08 | Secure lock confirmation and recovery | Security / phase 7 | Multi-output presentation trace plus physical failure injection | Daily-driver recommendation |
| B09 | Accessible UI and keyboard monitor | Accessibility / phase 8 | Orca flow across app/UI/lock, nested ownership, denial tests | Claiming screen-reader support |
| B10 | Explicit sync/multi-GPU supported device paths | Renderer / phase 9 | Fences, copy/scanout paths, exact driver matrix | Vendor qualification and sync advertisement on unsupported backend |
| B11 | Resource limits preserve valid clients | Runtime / phases 2–9 | Boundary/flood tests with legitimate apps and memory/fd telemetry | Unbounded queues or silent broad client kills |
| B12 | Distribution/session packaging contract | Release / phase 10 | Clean install, launch, crash/logout, upgrade, uninstall on each supported distro | Production release |
| B13 | Outputless and no-GPU policy | Platform / phase 3 | Tested headless state or explicit safe failure; recovery documented | Claiming headless support from a nested pass |
| B14 | Performance targets match actual supported hardware | Performance / phases 3, 9 | Raw baselines and variance; target ratification | Marketing absolute latency/optimization claims |
| B15 | Config helper cannot freeze the compositor or leak processes | Config / phase 6 | Catchable hooks, blocked helper, kill/reap, stale result tests | Relying on in-process thread timeout as a hard limit |

## Scope and support decisions

Required release target: one physical seat, native Wayland and a tested X11 path, atomic-KMS-capable supported hardware, GLES SDR rendering, tested fractional scales, independent outputs, full canvas, Lua, portals, IME, accessibility, and secure locking. Multi-seat, live compositor replacement preserving app connections, arbitrary remote desktop injection, HDR, DRM leasing, and Vulkan composition require separate support decisions. They cannot silently appear in globals merely because bindings exist.

Not yet selected: distribution versions, hardware models, drivers, exact companion releases, refresh/scale maxima, runtime resource limits, and final benchmark thresholds. These are measured support choices with named deadlines, not universal assumptions. Local development headers and DRM nodes are present, but this audit did not open a seat/device or disturb the user's running compositor.

## Plan dependencies that changed

Implement buffer acquisition/release architecture in phase 3 even though cross-vendor explicit-sync qualification finishes in phase 9. Design privileged binding policy in phase 2, implement lock/capture policy by phase 7. Add accessibility data models when creating UI in phase 5, then qualify them in phase 8. A late phase cannot retrofit a foundational lifetime or trust model safely without revisiting earlier gates.

Read the detailed [state machines](14-state-machines.md), [case catalog](15-edge-cases.md), [integration contracts](16-integration-contracts.md), [resource policy](17-resource-security.md), and [release operations](19-release-operations.md) before breaking implementation into issues.

## Review-added blocking decisions and staged closure

| ID | Contract / owner | Design deadline | Qualification evidence |
| --- | --- | --- | --- |
| B16 | Consistent model-A host/sandbox trust / security+desktop | Phase 2 | Phase 7 normal daemon helpers, tagged sandbox denial, config/IPC boundary tests |
| B17 | Text/font stack and fallback / UI | Minimal spike phase 3 | Shaped/scaled UI, fallback/emoji policy and semantic data model by phase 5; AT-SPI/Orca qualification phase 8 |
| B18 | Retained black capture-on-lock and destination leases / capture+renderer | Ownership design phase 3 | Phase 7 lock retention/source/revoke/destination tests; phase 9 fast-path recheck |
| B19 | XDG state/backdrop/version matrix / layout+protocol | Phase 4 | Phase 7 native/X11; phase 8 canvas/overview state traces |
| B20 | Derived privacy graph / security+renderer | Labels/ownership phase 3 | Phase 7 plain capture; phase 9 effects/overview/magnifier W11 pixels and cost |
| B21 | Hardware/test infrastructure capacity / release+performance | Work breakdown phase 1 | Physical device access, VKMS/WLCS ownership, fuzz/soak/latency/accessibility capacity before phase 9 claims |

These design choices are recorded in [review resolutions](21-review-resolutions.md). The design decision can be settled while runtime qualification remains open. B04 closes phase-3 single-GPU implicit ownership separately from phase-9 explicit/multi-GPU ownership. B13 phase 3 requires alive outputless server, retained window membership, no output submits and safe reconnection; phase 8 adds no-display assistive announcements and recovery. B05 is owned by the protocol subsystem at each phase, with final consolidated closure in phase 10. No named engineers are currently assigned; role ownership must become actual assignments before scheduling commitments.
