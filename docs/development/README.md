# Initial implementation and development

The initial workspace uses Rust 1.99/edition 2024 and pins Smithay to `023b78b1c5a05ca512767cacc07efca55fae8be4`. See the root manifests, lockfile, [source notices](../../THIRD_PARTY.md) and [historical initial audit](../24-implementation-readiness-audit.md) and [current production gaps](production-status-2026-10-18.md). The bootstrap is adapted from MIT-licensed Smithay Smallvil; it is not a native-session implementation.

See the [current nested runtime milestone](runtime.md) and [bounded nested smoke harness](nested-smoke.md) and [scene regression harness](scene-smoke.md) for the integrated executable and its exact limits. The tables below retain component boundaries, not a production support matrix.

## Component boundaries

| Component | Current scope | Integration limits |
| --- | --- | --- |
| `flux-core` | Generational IDs, geometry, action vocabulary | Checked owning arena and shared strict actions now drive the nested runtime |
| [flux-layout](layout.md) | Pure strip/canvas workspace model | Connected to the nested scene/lifecycle; multi-output and complete state remain open |
| [flux-config / fluxwm-config](config.md) | Schema, isolated Lua helper and asynchronous coordinator | Nested gap/mode/default-width/binding publication and asynchronous autostart implemented; full device/output transactions remain open |
| [flux-input](input-bindings.md) | Real-keymap binding resolution and immutable Rust dispatch tables | Development/user controls use this path; keymap replacement and full device/input policy remain open |
| [flux-ipc / fluxctl](ipc.md) | Unix transport and command-line client | Nested query/action dispatch implemented; subscriptions unavailable |
| [flux-scene](scene.md) | Immutable checked window geometry and ordered clip/hit candidates | [Runtime scene integration](runtime-scene.md) adds copied Wayland input regions and clipped elements; native/transaction ownership remains open |
| [flux-process](autostart.md) | Bounded launch worker and named/positional autostart ownership | Direct children only; session dependencies/readiness/descendants and universal shutdown bounds remain open |
| [flux-native](native-foundation.md) | Device lifecycle model and opt-in Linux session/GPU initialization | Separate probe; main compositor native output/render/input integration and hardware qualification remain open |
| `fluxwm` | Smithay nested compositor bootstrap | Managed map/unmap, gated capabilities, bounded redraw and cleanup; full protocol/native qualification open |

## Reproduce checks

```sh
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python3 tools/check_docs.py
```

These commands require the committed lockfile and cached dependencies or network access. The audit bundle records actual outcomes including failures. Component tests do not automatically run a compositor or exercise physical devices. This machine has Wayland-server 1.26.0, xkbcommon 1.13.2, libudev 262, GBM 26.2.4-arch1.1, EGL 1.5 and GLES 3.2 development metadata. That is a local observation, not a complete package list, clean-install recipe or supported distribution matrix.

Use the [nested runtime instructions](runtime.md) for real commands and supported controls. It has automatic test shutdown, per-child display environment, readiness/stats and socket cleanup. Native backend selection is rejected; this is not a primary-session recommendation.

## Open audit records

- [Layout and lifecycle](layout-production-audit.md).
- [IPC and security](ipc-production-audit.md).
- [Config/helper](config-production-audit.md).

The ten-phase plan still governs qualification. Independent branches implement owned components; merge commits and final lockfile checks establish what actually belongs to the integrated source revision. Preserve a clean evidence snapshot before review, and record any later changes separately.

See [Linux build setup and CI](build.md), [runtime behavior](runtime.md) and [application launch ownership](processes.md) for current implementation boundaries.

[Runtime findings and production gates](runtime-findings.md) distinguish addressed defects, live evidence and source-only fixes that still need closure tests.

The [private-display pointer regression](private-input.md) checks real button/axis routing and stationary grab release through an authenticated owned Xvfb. The [shared-scene evidence bundle](../evidence/nested-milestone/2026-10-17-shared-scene/README.md) retains failure/pass comparisons and exact source/binary identities.

The [IPC shutdown regression](shutdown-smoke.md) verifies the quit dispatch boundary and a fixed response-drain deadline with an unread peer. Full process/session teardown remains separately qualified work.

[XDG shell responsiveness](shell-health.md) adds nonfatal ping/timeout/recovery diagnostics, a bounded shared timer and real serial/timing/disconnect/monitor-cap tests. It does not establish application frame progress or global resource admission.

[Nested host-focus recovery](host-focus.md) verifies cancellation of held keys/buttons, consumed shortcut ownership and one authorized popup grab through real private-display input. Separate short runtime directories keep test socket paths below AF_UNIX limits even when artifact paths are long.

[Bounded toplevel configures](configure-stream.md) limits ordinary unacknowledged traffic, coalesces latest layout intent and verifies ack/commit separation, healthy-sibling progress, remap overlap and pending-role destruction. Full layout transactions and global resource accounting remain open.

[Configuration loader coordination](config.md) now owns one active evaluation, one replaceable pending source and one completion, with explicit last-good commit and cancellation. It was developed on an isolated worktree/branch and integrated with the input compiler; [Runtime Lua publication](runtime-config.md) now uses this coordinator behind a worker and explicitly limits application to gap/mode/bindings.

[Configured width defaults](config-widths.md) extend Lua application to new admissions using fixed/fraction policies and one-based preset references. Desired/effective queries distinguish the requested tag from the normalized policy; existing column policies remain intact.

[Autostart ownership](autostart.md) defines named/positional identity, reload behavior, bounds, diagnostics and direct-child shutdown. [Current production gaps](production-status-2026-10-18.md) records evidence and closure requirements.

- [Native output discovery and candidate routing](native-outputs.md): bounded topology, exact format candidates, resource matching and unimplemented KMS/presentation requirements.

- [Native RGB target allocation and atomic test](native-target-test.md): manual opt-in, fixed TEST_ONLY flags, owned resource retirement and unqualified hardware paths.

- [Native frame transport and ownership](native-frame-transport.md): full-cookie event delivery, bounded owned frame ledger and remaining live submission integration.

- [Native live submission and reverting preview](native-live-submit.md): owned primary-plane commits, copied rollback resources and explicit hardware qualification gaps.

- [Experimental native compositor](native-compositor.md): optional main executable backend, completion-gated scenes, input/output integration and software cursor.

[Minimize/restore](minimize.md) documents retained layout slots, explicit IPC restore and input-grab policy.
