# Initial implementation and development

The initial workspace uses Rust 1.99/edition 2024 and pins Smithay to `023b78b1c5a05ca512767cacc07efca55fae8be4`. See the root manifests, lockfile, [source notices](../../THIRD_PARTY.md) and [current production audit](../24-implementation-readiness-audit.md). The bootstrap is adapted from MIT-licensed Smithay Smallvil; it is not a native-session implementation.

## Component boundaries

| Component | Current scope | Integration limits |
| --- | --- | --- |
| `flux-core` | Generational IDs, geometry, action vocabulary | No owning generational arena or compositor command dispatch |
| [flux-layout](layout.md) | Pure strip/canvas workspace model | Not connected to Wayland lifecycle, scene or multi-output state |
| [flux-config / fluxwm-config](config.md) | Schema and isolated Lua helper | No compositor atomic reload state machine or keybinding application |
| [flux-ipc / fluxctl](ipc.md) | Unix transport and command-line client | No compositor server dispatch or subscription implementation |
| `fluxwm` | Smithay nested compositor bootstrap | Unfinished capability, lifecycle, redraw, startup and failure paths |

## Reproduce checks

```sh
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all -- --check
python3 tools/check_docs.py
```

These commands require the committed lockfile and cached dependencies or network access. The audit bundle records actual outcomes including failures. Component tests do not automatically run a compositor or exercise physical devices. This machine has Wayland-server 1.26.0, xkbcommon 1.13.2, libudev 262, GBM 26.2.4-arch1.1, EGL 1.5 and GLES 3.2 development metadata. That is a local observation, not a complete package list, clean-install recipe or supported distribution matrix.

Do not use the bootstrap as the primary session. Build a private nested smoke harness with bounded execution, an explicit client argv, per-child display environment and socket cleanup before documenting a general runtime command. Current source defaults to `weston-terminal`, which was absent in the inspected environment; it lacks a readiness handshake and automatic test shutdown.

## Open audit records

- [Layout and lifecycle](layout-production-audit.md).
- [IPC and security](ipc-production-audit.md).
- [Config/helper](config-production-audit.md).

The ten-phase plan still governs qualification. Independent branches implement owned components; merge commits and final lockfile checks establish what actually belongs to the integrated source revision. Preserve a clean evidence snapshot before review, and record any later changes separately.
