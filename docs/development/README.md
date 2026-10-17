# Initial implementation and development

The initial workspace uses Rust 1.99/edition 2024 and pins Smithay to `023b78b1c5a05ca512767cacc07efca55fae8be4`. See the root manifests, lockfile, [source notices](../../THIRD_PARTY.md) and [current production audit](../24-implementation-readiness-audit.md). The bootstrap is adapted from MIT-licensed Smithay Smallvil; it is not a native-session implementation.

See the [current nested runtime milestone](runtime.md) and [bounded smoke harness](nested-smoke.md) for the integrated executable and its exact limits. The tables below retain component boundaries, not a production support matrix.

## Component boundaries

| Component | Current scope | Integration limits |
| --- | --- | --- |
| `flux-core` | Generational IDs, geometry, action vocabulary | Checked owning arena and shared strict actions now drive the nested runtime |
| [flux-layout](layout.md) | Pure strip/canvas workspace model | Connected to the nested scene/lifecycle; multi-output and complete state remain open |
| [flux-config / fluxwm-config](config.md) | Schema and isolated Lua helper | No compositor atomic reload state machine or keybinding application |
| [flux-ipc / fluxctl](ipc.md) | Unix transport and command-line client | Nested query/action dispatch implemented; subscriptions unavailable |
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
