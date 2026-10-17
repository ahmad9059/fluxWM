# Layout, presentation, input, and lifecycle production audit

Audit date: 2026-10-17. Status: open production blockers. This audit distinguishes compiled pure-model evidence, inspected integration source, and work that still needs runtime proof. It does not qualify phases 2–8 as complete.

## Scope and reproducibility

The pure layout snapshot is `feature/layout` commit `0903482` (following `68461e2`). The compositor source was inspected in `/home/ahmad/Documents/Projects/fluxwm`, whose HEAD was `ae3abc3a7e45947205dc4d06858e65356bde5b4b`; compositor files were present as work in progress, so the hashes below identify the inspected content more precisely than HEAD. Later fixes must be re-audited against their final integrated revision.

| Inspected root-worktree source | SHA-256 |
| --- | --- |
| `crates/fluxwm/src/state.rs` | `9f4b42188018935c1173bf44da547ad97e1bc5cdce16f1deeddcc2fd4d8527dc` |
| `crates/fluxwm/src/input.rs` | `1a3714cd8b6627846877ab3affb7e22b3953a3ff4d99a37c56c092059848dd76` |
| `crates/fluxwm/src/winit.rs` | `27c3fa6b3c4489f620bd0da85b3e74766a784d291d57db307de0dacc5613e949` |
| `crates/fluxwm/src/handlers/xdg_shell.rs` | `e60c5242b63545ca807869464bf83244cd6ce419b7cc5ed9bb697d69e8969a8f` |
| `crates/fluxwm/src/grabs/resize_grab.rs` | `f0ccdd77f46577d8e158db1b53b6da425764205ee9cbc97e792b089ce9a68642` |

Pinned upstream evidence: Smithay revision `023b78b1c5a05ca512767cacc07efca55fae8be4`, inspected directly in the Cargo checkout. Source references below to Smithay are to this exact revision, not `latest` documentation. The root manifest's `crates/fluxwm/Cargo.toml:9` pins this source.

Commands actually run for the pure layout snapshot:

```sh
cargo test -p flux-layout
cargo clippy -p flux-layout --all-targets -- -D warnings
git diff --check
```

Result: 11 unit tests passed, including one deterministic 10,000-command mutation trace; clippy passed with warnings denied. This audit did not run compositor protocol fixtures, a native backend, output hotplug, visual image comparison, or hardware latency/power measurements. None of those results can be inferred from these commands.

## Confirmed implementation and its boundary

The layout crate implements generational-key membership uniqueness, focus history, independent fixed/fractional strip widths, stack minimum/maximum allocation, tab visibility, focused internal overflow reveal, reorder/join/resize, camera reveal, two-axis per-window canvas placement, world/view transforms, picking, and atomic internal canvas rebasing. See `crates/flux-layout/src/lib.rs:233` for insertion, `:292` for removal, `:459` for join, `:508` for reorder, `:608` for placement generation, and `:663` for rebasing.

Tests prove these model operations for their enumerated inputs. They do not prove protocol lifecycle, buffer synchronization, output ownership, GPU rendering, actual presentation timing, or screen-reader operation. The random trace has at most 100 windows, one initial seed, and no hotplug/Wayland/animation commands. The seed and command sequence are deterministic; no shrinking property-test framework or broad fuzz corpus exists in this crate.

At the inspected root snapshot, `Fluxwm` owns a Smithay `Space<Window>` but no `Layout`, no generational window arena, and no workspace/output ownership graph (`state.rs:26–43`). New toplevels are mapped at `(0,0)` (`handlers/xdg_shell.rs:35–38`). Thus compiled pure-model behavior and live compositor behavior are separate artifacts at this point. Integration work must not be reported as verified scrolling tiling until real clients pass the protocol and visible-scene tests below.

## Priority findings and acceptance evidence

### LP-01: Advertised XDG capabilities exceed implemented behavior — confirmed high priority

Evidence: `state.rs:58` constructs `XdgShellState::new`. Pinned Smithay `src/wayland/shell/xdg/mod.rs:1226–1238` explicitly gives that constructor Fullscreen, Maximize, Minimize, and WindowMenu capabilities. FluxWM's complete `XdgShellHandler` implementation at `handlers/xdg_shell.rs:30–124` does not override their corresponding requests. Pinned trait defaults at Smithay `mod.rs:1130–1146` send a configure for maximize/fullscreen without implementing the layout state and ignore minimize; the window-menu default is also not a working menu. This contradicts `docs/22-window-state-matrix.md:9` and `docs/phases/04-scrolling-layout.md:47`.

Required change: construct with an explicit capability set derived from actual behavior. Set no optional action capabilities until their complete implementation exists. Changing the global defaults does not update existing toplevels: pinned Smithay `mod.rs:1260–1264` requires updating their pending capability state and configuring separately.

Acceptance: a synthetic client bound at versions 1 through the selected maximum records initial event ordering. For version 5+, assert only implemented capabilities appear before initial configure; for earlier versions, assert no capability event. Enable each feature independently and verify existing and newly created windows receive truthful updates. Exercise each advertised request and inspect resulting geometry, state flags, focus, restore behavior, and backdrop.

### LP-02: No integrated window-state machine — confirmed high priority gap

Evidence: pure `Layout` has `mode`, columns, constraints, canvas, focus, and cameras but no minimized/maximized/fullscreen/floating state. The root handler has no corresponding request overrides. `docs/22-window-state-matrix.md:17–26` requires all of these states and explicitly distinguishes hidden tabs, minimized windows, offscreen windows, and overview.

Required design: keep ordinary container membership and saved mode geometry while presentation state changes. A minimized member remains recoverable; fullscreen has an output-sized opaque page/backdrop even for transparent or undersized clients. Maximize uses usable geometry and preserves restore intent. These are distinct from a width fraction or removing a member. Add bounded transition/restore state rather than unbounded request history.

Acceptance: test repeated/nested maximize→fullscreen→minimize→restore→unfullscreen→unmaximize requests, fullscreen while focus moves to another column, transparent fullscreen buffers, late old buffers, close refusal, and restore after output removal. Assert actual XDG flags, geometry, capabilities, search/task discoverability, scene layers, and input eligibility. Link these to SURF-09 and LAY-09; pure `Rect` tests do not close them.

### LP-03: Resize completion is not correlated with a configure response — confirmed code gap

Evidence: resize motion sends configure on each input event (`grabs/resize_grab.rs:130–136`). Button release sets `WaitingForLastCommit` with edges and initial rectangle but no configure serial/epoch (`:165–177`). Any subsequent call to `ResizeSurfaceState::commit()` transitions that state to Idle (`:308–315`); `handle_commit` invokes it on the surface's next commit (`:323–335`). The model therefore has no field with which to distinguish a response to the final configure from another commit.

Required change: preserve the layout/configure epoch and readiness independently from acquire-fence readiness. Track the relevant acknowledged configure and the buffer/content commit that satisfies the transaction. Retain a last-valid fallback without waiting synchronously for a hung client. Bound transaction lifetime and pending work.

Acceptance: reorder ack/commit delivery; send a damage-only or stale-size commit after final resize configure; never ack; ack without a new buffer; send superseded buffers; destroy during resize; wait on an unsignaled acquire fence. Other windows must remain navigable. Inspect transaction IDs and presented geometry rather than declaring the first commit successful.

Additional numerical review: `resize_grab.rs:341–345` adds/subtracts `i32` geometry directly. Extreme valid input and pointer/geometry limits need checked arithmetic tests. No panic or release-wrap reproduction was executed in this audit, so this is an unqualified arithmetic risk, not a demonstrated exploit.

### LP-04: Popup grabs are explicitly absent — confirmed high priority gap

Evidence: `handlers/xdg_shell.rs:121–123` contains `TODO popup grabs`. Popup geometry is unconstrained relative to the first output (`:201` in the inspected file; find the `self.space.outputs().next().unwrap()` call), not a verified owning-output policy. The existing input path picks ordinary windows from `Space` on press (`input.rs:68–78`).

Acceptance: nested popup chains, valid and invalid serials, popup dismissal by outside click and Escape, grabbed parent destruction, popup below lock/overview overlays, and popup reposition after camera/scale/output changes. Assert client event ordering, keyboard/pointer ownership, constrained surface-local coordinates, and that unrelated clients do not receive a consumed dismissal click. Validate any upstream helper integration from actual protocol traces.

### LP-05: Outputless input and popup paths dereference a nonexistent output — confirmed code gap

Evidence: absolute pointer handling calls `self.space.outputs().next().unwrap()` (`input.rs:34–37`). Popup unconstraining similarly unwraps the first output. The plan explicitly requires outputless survival (`docs/03-layout.md:55–57`). Current winit source only creates one output and does not exercise zero-output operation; this audit did not reproduce a crash through a live removal path.

Required change: resolve an optional eligible owning output; define outputless event handling and retain valid model state. A seat is not an output. Reject/cancel requests when their context disappears without assuming another live monitor exists.

Acceptance: destroy the only output during pointer motion, popup placement, move, resize, and pending configure. Keep clients/server alive, stop presentation work, then create a new output and recover all members. Explicit reassignment during disconnection must suppress automatic return to the former output.

### LP-06: No visible scene snapshot for coherent picking — confirmed architecture gap

Evidence: `state.rs:150–154` directly picks current mutable `Space`. Move grabs remap `Space` during input motion (`grabs/move_grab.rs:42–46`), whereas drawing occurs later in `winit.rs:61–85`. There is no immutable output-visible snapshot in `Fluxwm` (`state.rs:26–43`). The design requires per-output presented/submitted snapshot semantics (`docs/20-implementation-contracts.md:81–85`).

Required change: separate target layout, pending scene, and visible scene. In nested mode explicitly document whether host presentation completion is observable; a submitted scene cannot silently be called host-presented. Picking must include clips, animation transforms, surface offsets, input regions, popup/grab overrides, and protected layers.

Acceptance: move/animate a window but delay submission; click its old visible position and its future position. Repeat with two outputs presenting at different times, zoom, fractional scale, subsurfaces, empty input regions, and popups outside parent bounds. Log scene identity used for each pick and each output submission/presentation.

### LP-07: Relative input and compositor keybinding handling are absent — confirmed scope gap

Evidence: `input.rs:33` discards `PointerMotion`; keyboard filtering always returns `FilterResult::Forward` (`:24–31`). Gesture/touch/tablet events fall through the wildcard in the main input dispatcher. Move/resize grab implementations forward gesture callbacks but do not implement compositor gesture ownership.

Required change: implement device/backend-specific relative motion, pointer constraints and relative-pointer protocol integration; keybinding press/release ownership; repeat semantics; one-owner gesture cancellation. Nested winit event support and native libinput coverage must be qualified separately. Do not advertise client-facing gesture/constraint support solely because event structs exist.

Acceptance: key press consumed by a binding followed by release after focus/keymap change; modifier released during lock; active gesture cancelled by device/output removal; pointer lock activation/deactivation; simultaneous touch contacts; tablet proximity loss. Assert no orphan release or double delivery to client and compositor. Complete recovery commands without pointer input.

### LP-08: Redraw work never becomes idle and submission failures panic — confirmed code gap

Evidence: `winit.rs:63` constructs full framebuffer damage; `:85` submits that full region; `:100–101` requests another redraw after every redraw. Damage tracking is present, but unconditional full submission and scheduling are still explicit. Bind/render/submit use `unwrap()` (`:66`, `:83`, `:85`). `:87–94` loops over all windows for frame callbacks, independent of the planned hidden/capture obligations.

Required change: schedule only for pending damage, active motion, or protocol/presentation obligations. Handle renderer/output failure with bounded recovery and clear diagnostics. Define a frame-callback policy that keeps clients progressing while respecting hidden state; do not assume callbacks are synonymous with actual display presentation.

Acceptance: settle 20 windows and measure zero compositor-requested redraws over a fixed idle interval; trigger one damage region; animate then settle; hide a video tab with/without capture; inject bind/render/submit errors. Record redraw/submission/callback counts, GPU work, and recovery. These are required measurements; damage-tracker type presence cannot establish idle efficiency.

### LP-09: Live ownership and remap lifecycle remain unproven — confirmed missing integration evidence

Evidence: new toplevel creation maps directly into `Space` before the initial configure/buffer lifecycle (`handlers/xdg_shell.rs:35–38`, `:150–169`). `ClientState::disconnected` is empty (`state.rs:168`). `Space::refresh` runs during redraw (`winit.rs:96`), but no FluxWM generational identity/ownership reconciliation is visible. Upstream helpers may handle parts of surface lifetime; their presence does not prove FluxWM's future column/canvas/transaction state is cleaned correctly.

Acceptance: create without ever attaching a buffer; initial null-buffer commit; map; null-buffer unmap; remap the same role; destroy popup/parent in either order; disconnect one multi-window client; reuse a window slot with a higher generation. At each stage inspect protocol errors, layout membership, focus, task/search entries, frame callbacks, pending grabs/transactions, and resource-retirement counts. Do not automatically equate unmap with irreversible logical destruction.

## Model/plan divergences that need explicit decisions

| ID | Evidence and current behavior | Required decision or acceptance case |
| --- | --- | --- |
| LP-10 | `docs/03-layout.md:7` defines canvas Groups with local columns/stacking/anchors. `flux-layout:612–620` maps each WindowId directly to one rectangle and marks every canvas member visible. | Implement group identity and ownership, preserve stack/tab semantics inside groups, and prove hidden tabs cannot become coincident directional targets. Test stacked/tabbed strip→canvas→strip round trips. |
| LP-11 | The plan seeds canvas from current strip geometry on first entry (`docs/03-layout.md:49`). The implementation seeds each member at insertion (`flux-layout:275–286`) and `set_mode` only changes mode and reveals focus (`:390–393`). | Define and test first-entry conversion. Reproduce by inserting width 500, changing strip width to 700/reordering, then entering canvas: current saved canvas geometry still reflects insertion time. Decide how a newly inserted tab/stack member seeds a group. |
| LP-12 | The same plan line requires deterministic x/y sorting on first return to strip while also saying subsequent mode changes restore saved strip positions. The implementation always retains strip order. | State explicitly which transition is the first conversion and store an initialization flag. Test first return after canvas rearrangement separately from all later restoration. This plan ambiguity must be resolved before marking conversion complete. |
| LP-13 | Plan focus tie-breaking uses projected edge distance (`docs/03-layout.md:41`); code compares center forward distance (`flux-layout:580–598`). No nearest-center alternative API exists. | Select the documented policy or implement both. Use unequal window sizes where edge and center ordering differ, half-plane boundary ties, overlapping windows, and distant candidates. |
| LP-14 | `visible_placements` builds all placements then filters (`flux-layout:623–635`); prefix positions are recomputed by `strip_placements` (`:773`). | Implement measured cached/binary-search strip visibility. Benchmark fixed visible count at 100/1,000/10,000/million synthetic columns, separating layout mutation from scene queries. Do not claim O(log n + k) today. |
| LP-15 | `MAX_COORD` is one billion (`flux-layout:9`); aggregate strip extent is rejected (`:732–765`). Rebasing shifts internal canvas rectangles/camera only (`:663`). | Describe finite local-coordinate policy, introduce checked accumulated origin, and atomically rebase gesture/animation/index state. Test both signs, repeated rebases, stale external snapshots, and rollback. Infinite user navigation requires this external state; finite local bounds alone do not deliver it. |
| LP-16 | Stack allocator distributes equal shares (`flux-layout:862`), while the plan specifies weights (`docs/03-layout.md:11`). Width presets, outer gaps/decorations, reveal comfort margins, requested oversized alignment policy, and explicit stack overflow scrolling are absent. | Define policy types and tests for weights, capped members, decorations consuming all height, long oversized clients, presets after usable-size changes, and keyboard access to all overflow content. |
| LP-17 | `SizeConstraints.max: Option<Size>` validates both axes positive. XDG allows zero/unconstrained on an individual axis (pinned Smithay `SurfaceCachedState` docs at `mod.rs:1047–1063`). | Provide a tested adapter for `(0, finite)`, `(finite, 0)`, and `(0,0)` maxima, normalize min zero, and respect actual protocol validation for contradictory constraints. Never pass raw zeros to the pure positive-size API. |
| LP-18 | Camera transforms use untyped core `Point`/`Rect`; the renderer currently uses integer Smithay `Space`. The plan distinguishes world/output/surface/buffer/physical coordinates. | Add explicit boundary conversion helpers and shared-edge rounding; test 1.25×, 1.5×, all output transforms, viewporter crops, subsurface offsets, and negative world origins. Ensure no world positions leak into XDG size/configure fields. |
| LP-19 | Pure `insert_with_focus(false)` still focuses the first member of an empty workspace to satisfy its valid-focus invariant. The live input path can have no keyboard focus. | Decide whether model selection and protocol activation are separate concepts. Test background/untrusted first-client activation while launcher/lock UI owns focus; do not activate a client merely because the model needs a selected member. |
| LP-20 | `hit_test` prioritizes focused canvas content, then reverse stable-ID order (`flux-layout:638`); there is no persistent canvas stacking key. | Add explicit stacking history and compare picking/render ordering after focus changes, tab changes, overlay display, and group overlap. Window identity order is deterministic but not a complete user stacking policy. |

## Missing verification artifacts before a production claim

The following artifacts are required by the existing plan and have no supporting runtime evidence in the audited snapshots: synthetic XDG lifecycle/serial clients; version-bound capability/state traces; presentation/picking identity logs; configure/acquire readiness traces; hidden-client callback fixtures; outputless/hotplug migration traces; screenshot/backdrop assertions; accessibility keyboard and screen-reader session recordings; physical input-to-visible-response measurements; and steady-state idle/GPU benchmarks.

Add a machine-readable map from LP finding ID to implementing revision, exact acceptance command/environment, raw trace artifact, expected result, actual result, and reviewer disposition. Record a failure as a failure. A case that requires hardware unavailable in CI stays open and names the missing qualification environment. Do not replace it with a model-only check.

The existing documentation also needs a factual status correction: `docs/20-implementation-contracts.md:3` says the standalone probe is the only Rust code provided. Rust workspace/core/layout work now exists. Replace that statement with a dated implementation-status pointer while retaining the distinction between proposed interfaces and compiled APIs. The phase files still say planned; that is consistent with the missing gates and should not be changed to complete on the basis of this audit.

## Recommended order of corrective work

1. Restrict advertised capabilities; establish explicit map/unmap/destroy ownership and no-panic outputless paths; preserve protocol progress under rejected requests.
2. Integrate the pure layout through target geometry and generational live identities; implement configure/commit readiness without coupling it to acquire fences.
3. Implement popup/grab cancellation and real keybinding ownership; establish submitted/visible scene picking semantics before animation.
4. Implement full window-state/restore/backdrop behavior and cross-output ownership; qualify native client state rows.
5. Resolve group conversion, stacking, numerical origin, constraints adapters, and query complexity with policy decisions and adversarial replay cases.
6. Add animation/overview/accessibility only while retaining the lifecycle and scene contracts; run independent Claude review against the final integrated diff and evidence, then record unresolved findings rather than treating reviewer approval as runtime proof.
