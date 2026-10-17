# Immutable source findings and verification limits

Retrieved 2026-10-17. The [machine-readable manifest](evidence/upstream-sources.json) records exact repository commits, source URLs, byte counts, and SHA-256 hashes. These are **research snapshots**, not selected FluxWM dependencies or proof that the revisions build together. Source contents were inspected from a temporary cache and are not vendored into this repository.

## Observed source facts

### Smithay integration surface

The inspected [Cargo manifest](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/Cargo.toml) specifies edition 2024 and minimum Rust 1.87, with distinct Vulkan backend and GLES renderer feature flags. These values apply to this exact commit only. Released examples/macros may differ from this snapshot; phase 1 must compile the selected revision rather than mix API snippets.

[Syncobj module](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/src/wayland/drm_syncobj/mod.rs) documents an eventfd capability probe and acquire-blocker integration. [Renderer buffer implementation](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/src/backend/renderer/utils/wayland.rs) signals release through final ownership retirement. [Lock handler](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/src/wayland/session_lock/mod.rs) leaves safe output coverage/confirmation to compositor integration. [Security-context implementation](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/src/wayland/security_context/mod.rs) exposes client filtering. [Image-copy capture](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/src/wayland/image_copy_capture/mod.rs) and [capture-source modules](https://github.com/Smithay/smithay/blob/023b78b1c5a05ca512767cacc07efca55fae8be4/src/wayland/image_capture_source/mod.rs) are present. Presence is not completed output/window capture policy.

### Protocol XML

[Syncobj XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/linux-drm-syncobj/linux-drm-syncobj-v1.xml) distinguishes per-commit acquire/release state, invalid timeline point combinations, and release behavior of unused buffers. A release-before-acquire event is not inherently invalid when no reading occurred. [Lock XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/ext-session-lock/ext-session-lock-v1.xml) ties successful lock notification to safely presented output coverage and keeps the session locked after locker death. [Security-context XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/security-context/security-context-v1.xml) warns about nested contexts and defines listener lifetime beyond creator disconnect.

[Capture XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/ext-image-copy-capture/ext-image-copy-capture-v1.xml) defines buffer constraint batches and frame success/failure. [Source XML](https://github.com/wayland-mirror/wayland-protocols/blob/819004adb3ab7e46f3fa3caef05b96e20434b244/staging/ext-image-capture-source/ext-image-capture-source-v1.xml) provides source interfaces. The [interface inventory](evidence/protocol-inventory.json) was generated directly from retrieved XML, including interface maximum versions. All FluxWM advertised versions are null because nothing is implemented.

### Satellite and portal backend

[Satellite README](https://github.com/Supreeeme/xwayland-satellite/blob/97a89cec53eb4f8788386a574450dd94c0538754/README.md) names host xdg-shell/viewporter requirements and discusses readiness integration and HiDPI limitations. [Architecture](https://github.com/Supreeeme/xwayland-satellite/blob/97a89cec53eb4f8788386a574450dd94c0538754/ARCHITECTURE.md) explains its client/server/XWM roles. It does not establish FluxWM interoperability.

[Portal-wlr README](https://github.com/emersion/xdg-desktop-portal-wlr/blob/c0255d7b047b7263629ab5a314661045ff7f65e3/README.md) covers Screenshot/ScreenCast and delegation of other portals. [Capture dispatch source](https://github.com/emersion/xdg-desktop-portal-wlr/blob/c0255d7b047b7263629ab5a314661045ff7f65e3/src/screencast/wlr_screencast.c) chooses ext image-copy when the corresponding source globals are available and otherwise can use wlr screencopy. [Session source](https://github.com/emersion/xdg-desktop-portal-wlr/blob/c0255d7b047b7263629ab5a314661045ff7f65e3/src/screencast/screencast.c) includes window/toplevel selection handling. This invalidates a blanket assumption that every portal-wlr revision is output-only; it still requires a real window-sharing test for the selected release.

### Accessibility

[niri accessibility documentation](https://github.com/niri-wm/niri/blob/45739bc072f9875a674d335ae724f10d3b35c665/docs/wiki/Accessibility.md) describes Orca, session-dependent accessibility registration, and a keyboard-monitor D-Bus interface. [niri accessibility source](https://github.com/niri-wm/niri/blob/45739bc072f9875a674d335ae724f10d3b35c665/src/a11y.rs) uses an AccessKit adapter on a worker with a bounded update channel. This is evidence of a concrete integration pattern and failure handling, not evidence that FluxWM already works with Orca. [AccessKit upstream](https://github.com/AccessKit/accesskit) describes its Unix AT-SPI adapter.

### Lua

[mlua state source](https://github.com/mlua-rs/mlua/blob/a186d461c7772aed4cb0a9fa1d1899121c6450e7/src/state.rs) separates Lua backends and hook/limit availability. The local [Lua 5.4 probe](evidence/lua54-probe.txt) demonstrates catchability of ordinary instruction-hook errors. The standalone probe used Lua 5.4.9. A separate [mlua embedding probe](evidence/probes/mlua-hook/README.md) built the inspected commit with `lua54,vendored` and reproduced five caught hook errors; [execution metadata](evidence/mlua-probe-result.json) records a successful locked/offline rerun. This still does not test the production helper process design. [Lua 5.4 manual](https://www.lua.org/manual/5.4/manual.html) is the language reference.

## External primary documentation inspected

- [Kernel DRM userspace API](https://docs.kernel.org/gpu/drm-uapi.html): device removal/reset and userspace testing context. Use driver-specific qualification rather than assume successful reset recovery.
- [VKMS](https://docs.kernel.org/gpu/vkms.html): useful virtual KMS testing route; it cannot qualify physical scanout, GPU vendors, real input, or monitor behavior.
- [Systemd desktop integration](https://systemd.io/DESKTOP_ENVIRONMENTS/): session targets, shared per-user service environment, and autostart conventions; marked work in progress upstream.
- [Portal ScreenCast frontend](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html): session/restore semantics and versioned stream properties.
- [Portal ScreenCast backend](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.ScreenCast.html): backend contract distinct from application API.

Some direct freedesktop/systemd and libinput page requests returned errors. No claim relies on the failed page loads; alternative primary documentation or immutable source was used where available. Future package selection still needs its own dependency/API checks.

## Local preflight result

[Recorded output](evidence/local-preflight.json) shows Rust/Cargo 1.99.0, present native development metadata, and two render nodes. This does not identify qualified GPUs or prove seat permissions. The default `lua` executable is 5.5.1; the explicit `lua5.4` check used 5.4.9 and successfully parsed/evaluated the example. No native device was opened, no kernel module loaded, and no service environment changed.

## Snapshot index

| Repository | Resolved commit | Retrieved files | Failed files |
| --- | --- | --- | --- |
| Smithay/smithay | `023b78b1c5a05ca512767cacc07efca55fae8be4` | 13 | 0 |
| wayland-mirror/wayland-protocols | `819004adb3ab7e46f3fa3caef05b96e20434b244` | 13 | 0 |
| niri-wm/niri | `45739bc072f9875a674d335ae724f10d3b35c665` | 3 | 0 |
| Supreeeme/xwayland-satellite | `97a89cec53eb4f8788386a574450dd94c0538754` | 3 | 0 |
| mlua-rs/mlua | `a186d461c7772aed4cb0a9fa1d1899121c6450e7` | 2 | 0 |
| emersion/xdg-desktop-portal-wlr | `c0255d7b047b7263629ab5a314661045ff7f65e3` | 4 | 0 |

## Revalidation

Use the manifest URLs to retrieve bytes and compare hashes. Treat a mismatch, inaccessible URL, or changed API as a finding, not a reason to silently replace the snapshot. A later snapshot may be useful but must receive a separate entry and review. The documentation verifier validates manifest structure and optional supplied cache contents; it does not make network access a hidden dependency of local docs checks.

## Provenance correction after independent review

`wayland-mirror` is not assumed to authenticate upstream identity. [Distribution comparison](evidence/protocol-provenance.json) checked all 13 inspected XML files against installed Arch `wayland-protocols 1.49-1`: 12 were byte-identical; DMA-BUF differed. The difference inspected is an added feedback-format-table ordering clarification in the mirror. Do not silently substitute its hash or claim mirror Git ancestry is verified. Phase 1 must select protocol inputs from an authenticated upstream release or trusted distro source and reconcile this difference. The current inventory records **mirror-observed** versions only.

The embedded `lua54` build's `lua-src 551.0.2` source maps Lua54 to `lua-5.4.9`; its header reports release 9. Thus both recorded probes used Lua 5.4.9. The locked/offline execution record is explicitly a **cached rerun after a successful initial build**, not a cold clean-system build. The probe README gives both reproduction modes.
