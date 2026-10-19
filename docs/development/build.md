# Build, checks and nested development

FluxWM currently targets Linux development with Rust **1.99.0**, edition 2024. The
committed rust-toolchain.toml requests rustfmt/clippy; Cargo.lock fixes dependency
resolution, including Smithay revision `023b78b1c5a05ca512767cacc07efca55fae8be4` and
mlua revision `a186d461c7772aed4cb0a9fa1d1899121c6450e7`. Default builds implement the nested backend. Feature `native` adds an
[experimental single-output native frontend](native-compositor.md); no production
session package or physical-GPU qualification is claimed.

## Host dependencies from selected source

The default compositor selects Smithay `backend_winit`, `wayland_frontend` and `desktop`
with default features disabled. The selected feature expansion includes EGL/GLES
rendering and both winit host backends. It does not select Smithay backend_drm,
backend_gbm, backend_libinput, backend_udev or backend_session_libseat.

| Requirement | Evidence in selected dependencies | Development consequence |
| --- | --- | --- |
| C compiler/linker and ordinary libc development environment | Vendored Lua build through pinned mlua uses C compilation; Rust binaries link native code | Install a C toolchain; Ubuntu build-essential is the proposed package set |
| xkbcommon development link library | xkbcommon 0.9.0 `src/xkb/ffi.rs` explicitly links xkbcommon; Smithay depends on that crate | Ubuntu libxkbcommon-dev is required for the native link, even when no display is opened |
| pkg-config tool | Transitive build scripts include probes, including x11-dl's build logic; wayland-sys conditionally probes native libraries | Install pkg-config; inspect actual feature changes before adding packages |
| Wayland shared libraries for a Wayland host | wayland-sys 0.31.11 build.rs returns before probes when dlopen is selected; selected feature tree enables dlopen | Current generic compilation does not need libwayland-dev headers/link metadata; nested Wayland runtime still needs the host libraries |
| EGL implementation for rendering | Smithay EGL ffi loads libEGL.so.1 through libloading; GLES symbols are obtained at runtime | Runtime EGL/driver support must be installed and tested separately from successful compilation |
| X11 host libraries and server | winit's selected X11 dependency path uses x11-dl/xkbcommon-dl | An X11 nested run requires those runtime libraries and an available server; generic tests do not open one |
| Python 3 and explicit Lua 5.4 executables | Helper isolation tests use /usr/bin/python3; tools/check_docs.py invokes lua5.4 and luac5.4 | Install Python 3 and Lua 5.4, even though the actual config helper embeds vendored Lua |

Source references: [selected Smithay manifest](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/Cargo.toml),
[Smithay build script](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/build.rs),
[wayland-sys build script](https://docs.rs/crate/wayland-sys/0.31.11/source/build.rs),
and [xkbcommon FFI](https://docs.rs/crate/xkbcommon/0.9.0/source/src/xkb/ffi.rs).
The Ubuntu package names are recorded by the official [libxkbcommon-dev](https://packages.ubuntu.com/noble/libxkbcommon-dev)
and [lua5.4](https://packages.ubuntu.com/noble/lua5.4) entries. Source inspection
establishes dependency intent; it does not establish a clean distribution install.

Proposed Ubuntu 24.04 compilation/test recipe, **not yet executed on a clean Ubuntu
runner**:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends -y \
  build-essential ca-certificates git pkg-config libxkbcommon-dev python3 lua5.4
rustup toolchain install 1.99.0 --profile minimal --component rustfmt --component clippy
```

GitHub's hosted runner supplies rustup. On a separate host, install rustup through
its reviewed official distribution process before this command. The official
[Rust 1.99.0 channel manifest](https://static.rust-lang.org/dist/channel-rust-1.99.0.toml)
was fetched successfully during this task and declares release manifest date
2026-10-01; this confirms distribution metadata availability, not that this Ubuntu
recipe has run. A system-packaged Cargo can also work when rustc/cargo are both 1.99;
verify versions instead of assuming it honors rustup's override variables.

On the inspected Arch development host, pkg-config reported Wayland client/server
1.26.0, xkbcommon 1.13.2, EGL 1.5 and GLES 3.2. A built nested executable's ldd output
listed libxkbcommon.so.0, libc and libgcc_s, with dynamic-loader libraries not appearing
as direct dependencies. These local observations do not promise ABI/driver compatibility
on Ubuntu or imply that all locally installed development packages are required.

## Reproducible commands and cache scope

From the repository root:

```sh
rustc --version --verbose
cargo --version
cargo fetch --locked --target x86_64-unknown-linux-gnu
cargo fmt --all -- --check
python3 tools/check_docs.py
cargo check --workspace --all-targets --locked
cargo build --workspace --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps --locked
cargo run --locked -p fluxwm-config -- --check docs/examples/config.lua
```

The tests include protocol/transport/config helpers and a synthetic client's unit
tests. They do not launch a compositor, connect to a live display, open DRM/input
devices or run the full graphical lifecycle fixture. The Python documentation check
also verifies standalone Lua 5.4 fixtures; its reported scope is documentation integrity,
not production readiness. The real fluxwm-config CLI check separately evaluates the
example in the supervised mlua helper and prints normalized JSON without spawning
configured commands.

After fetching the target-relevant dependency set and installing the toolchain and
native packages, use `--locked --offline` on Cargo build/check/test/clippy/doc/run
commands to prohibit registry/Git fetching. For example:

```sh
cargo check --workspace --all-targets --locked --offline
cargo test --workspace --all-targets --locked --offline
cargo run --locked --offline -p fluxwm-config -- --check docs/examples/config.lua
```

Offline mode is not an isolated or vendored build: it uses the local Cargo registry,
Git source, toolchain and native-library caches. It fails if a needed source is absent.
Full `cargo metadata --offline` can request dependencies for other targets: an observed
attempt failed for uncached android-activity 0.6.1 while Linux workspace check succeeded.
Use an appropriate explicit target/filter and fetch scope for your diagnostic, or fetch
all target dependencies beforehand; do not interpret that metadata failure as proof
that the Linux build cannot work offline.

A stale Cargo.lock makes `--locked` fail. Update it deliberately during dependency or
workspace membership changes, review the diff and commit it before expecting CI to
pass. The initial checked-out task baseline had such a mismatch; a locally regenerated
lock permitted verification, and that generated lock is not part of the CI/docs commit.
The integration owner must verify the final committed lock against the final workspace.

A clean incremental-output probe can use a fresh CARGO_TARGET_DIR, while retaining
source/toolchain caches:

```sh
CARGO_TARGET_DIR=/tmp/fluxwm-clean-build cargo build --workspace --locked --offline
```

Use a new empty path for that experiment and record its actual results. This document
does not claim that probe, a clean-container install or bit-for-bit reproducible
artifacts have been completed. Ubuntu runner images, apt packages, linkers, timestamps
and environment are not pinned by Cargo.lock.

## Current developer binaries

| Binary | Concrete purpose | Limits |
| --- | --- | --- |
| fluxwm | Nested Wayland compositor with an explicit child command, bounded runtime and evidence paths | No native-session/DRM backend or default application launch guarantee |
| fluxwm-fixture | Synthetic lifecycle client; JSON evidence and a total timeout at most five seconds | Requires the selected nested display for a full lifecycle; unit tests do not establish that lifecycle |
| fluxwm-config | `--check FILE` evaluates a bounded Lua snapshot in the isolated helper and prints normalized config | Does not apply config, validate hardware or start autostart commands |
| fluxctl | Explicit-socket IPC client with query/action/config-check/reload verbs | CLI availability does not prove compositor IPC dispatch or any method is implemented |

Observed compositor help during this task:

```text
fluxwm --backend nested [--socket NAME] [--exit-after-ms N] [--ready-file PATH] [--stats-file PATH] [--screenshot NEW.ppm] [-- COMMAND ARG...]
Only the nested backend is currently implemented. Output files must not already exist.
```

The development runtime changes are integrated separately from this documentation
branch. Check the actual integrated binary's `--help` before use. The old bootstrap
that defaulted to weston-terminal and mutated the process-wide display environment
must not be treated as the documented runtime contract.

Build all four binaries, then run under a suitable host graphical session:

```sh
cargo build --workspace --locked
./target/debug/fluxwm --backend nested --socket fluxwm-dev-test \
  --exit-after-ms 6000 \
  --ready-file /tmp/fluxwm-dev-ready.json \
  --stats-file /tmp/fluxwm-dev-stats.json \
  --screenshot /tmp/fluxwm-dev-frame.ppm \
  -- ./target/debug/fluxwm-fixture lifecycle
```

Choose fresh output paths and an unused socket name; the ready file is a dispatch
readiness marker, not proof of a mapped client or rendered frame. The compositor sets
the child WAYLAND_DISPLAY override. XDG_RUNTIME_DIR and host-backend availability must
already be valid. A screenshot uses PPM bytes even if a misleading extension is given;
use .ppm as above. Review the fixture JSON, server stats and actual pixel content
separately. See [nested fixture](nested-fixture.md) and recorded smoke evidence before
claiming a lifecycle passes. Keep this developer executable out of the primary session.

For component commands that need no display:

```sh
./target/debug/fluxwm-config --check docs/examples/config.lua
./target/debug/fluxctl --help
./target/debug/fluxctl --socket /explicit/ready/ipc.sock query version
./target/debug/fluxctl --socket /explicit/ready/ipc.sock action '{"name":"focus","direction":"left"}'
```

The last two commands require an actually implemented, ready IPC server at that exact
path. No Wayland-to-IPC socket-name equivalence or automatic discovery is promised.

## CI definition and evidence boundary

[Linux workflow](../../.github/workflows/linux.yml) runs pull requests, pushes and
manual dispatch on ubuntu-24.04, pins Rust 1.99.0, installs the recipe above and runs
the formatting/docs/build/test/clippy/rustdoc/config-CLI checks. Display variables are
empty for the test step; no generic CI step invokes a live compositor or GPU backend.
A graphical/hardware qualification job must be designed separately.

Only read-only repository permission is granted; checkout does not persist credentials.
Actions are pinned to immutable commit IDs verified using upstream primary refs on
2026-10-17:

| Action | Inspected tag | Exact verified object |
| --- | --- | --- |
| actions/checkout | v5.0.0 | `08c6903cd8c0fde910a37f88322edcfb5dd907a8` |
| actions/cache | v5.0.0 | `a7833574556fa59680c1b7cb190c1735db73ebf0` |

Verification command:

```sh
git ls-remote https://github.com/actions/checkout.git 'refs/tags/v5.0.0' 'refs/tags/v5.0.0^{}'
git ls-remote https://github.com/actions/cache.git 'refs/tags/v5.0.0' 'refs/tags/v5.0.0^{}'
```

The action commits are [checkout source](https://github.com/actions/checkout/tree/08c6903cd8c0fde910a37f88322edcfb5dd907a8)
and [cache source](https://github.com/actions/cache/tree/a7833574556fa59680c1b7cb190c1735db73ebf0).
Tags are discovery evidence; the workflow executes the fixed SHA even if a tag later
moves. Cache keys include the lock, manifests and toolchain file; restoring a cache
never bypasses locked dependency/check gates.

No hosted GitHub Actions run or clean Ubuntu job has been observed for this workflow.
Do not label it green until an actual run URL and job results exist. Local checks on
a different distribution are useful evidence with that narrower scope.

Local verification for the CI/docs task used source baseline `375f4ba` plus the new
workflow/docs and a locally regenerated, uncommitted lock. Formatting, Linux offline
all-targets check, workspace build, 72 tests, clippy with warnings denied, supervised
config CLI and the documentation checker passed; tests were also run with DISPLAY,
WAYLAND_DISPLAY and WAYLAND_SOCKET empty. The checker found 59 Markdown documents
and 383 local links at that snapshot. The new strict rustdoc command caught an actual
unresolved intra-doc link from unquoted `argv[0]` in flux-core action.rs. That source
fix is outside the CI/docs commit; rerun the gate after integrating it. These counts
and findings are baseline-specific evidence, not a hosted CI result.

The workflow itself was parsed and linted locally with actionlint 1.7.7 (exit 0):

```sh
go run github.com/rhysd/actionlint/cmd/actionlint@v1.7.7 .github/workflows/linux.yml
```

This optional authoring check requires Go/module network access on first use; Go is
not a workspace build/runtime dependency and is not installed by the CI job.
