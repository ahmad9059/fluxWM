# Native compositor integration review (frozen snapshot)

**Verdict:** I found no reachable HIGH defects. There is one MEDIUM issue, which sits on the edge of the stated "no recovery" scope. The rest are LOW findings and CI gaps. This is a code-reading review only. Nothing was built, run or tested, and it does not certify the code.

## What held up under inspection
- **Reentrancy:** `Driver::stop` and `Kernel::stop` never call notify. Every Kernel callback releases its `kernel` borrow before calling `notify` (`system.rs:359`, `:394`, `:866-873`, `:1047-1091`). So `Context::fail → driver.stop()` running inside notify, the watchdog or draw can't hit a double borrow. When stop removes the udev, session or DRM source from inside that same source's callback, calloop handles it safely because the dispatcher keeps its own clone.
- **Watchdog:** `Context::draw` only submits when no frame is pending (`native.rs:91`). `draw` arms a timer only on a None→Some cookie change, and it holds only a weak reference. Completion cancels it (`:255`), and shutdown cancels it too. Timers can't accumulate.
- **Content privacy:** `output_content` reads only `visible_scene` and is gated on ready and healthy. `compose` waits on the returned fence before submitting (`target_test.rs:440`). The cursor fence comes later in the same GLES context, so waiting on it covers the scene pass. Hidden and over-budget cursors fall back to the scene fence.
- **Scene timing:** `state.scene` is only replaced on PageFlip. Ignored or stale completions leave it unchanged. Faulted, closed or inactive queues fail the session.
- **Cursor:** `TraversalAction::Break` stops the whole traversal. The checks skip children exactly where Smithay's `render_elements_from_surface_tree` does. Hotspot and position arithmetic happens in f64 with bounds checked before casting.
- **Option checks and drop order:** options are validated inside `options()`, before any seat work. `native_handle` is declared after `state` and `event_loop`, so its `Drop` runs first on early returns. Clearing `state.dirty` after rendering is safe because `RuntimeScene::capture` takes `&Fluxwm` and can't change it.

## Findings

**MEDIUM: a hotplug on any connector of the selected GPU kills the session without restoring the display, and the error message is misleading**
- **Trigger:** a udev `Changed` event for the device. That happens on hot-plug of any connector, and some drivers send one after a modeset or link retrain.
- **Path:** `Kernel::udev` → `retire(id)` with `stopping=false`. The output is invalidated and not restored, and `Output::drop` deliberately leaks its state (`std::mem::forget`). Then `prepare(id)` rebuilds a full GPU (DRM device, GBM, EGL) before notify runs. Notify receives `GpuRemoved` first (`native.rs:302`) and fails with "native GPU removed".
- **Effect:** the `GpuReady` "topology changed" branch (`:305`) can never run. Real hardware may hit this often, and the user is left with an unrestored screen and the wrong cause.
- **Suggestion:** at minimum, report "topology changed" when `GpuRemoved` is immediately followed by `GpuReady`. Also record that topology changes leave the display unrestored by design.

**LOW**
1. **Init failures after the modeset.** Once `begin_output_preview` succeeds, several steps can still fail: `i32::try_from(refresh)`, `set_viewport`, and `make_ping`/`insert_source` (`native.rs:342-392`). Before the `Handle` exists, the bare `Driver` is dropped and attempts a restore, but nobody checks `output_cleanup_failures`. A failed restore during startup goes unreported, and the wl_output global and space mapping are left behind.
2. **Duplicate logging.** On the normal path, `Handle::shutdown` and then `Handle::drop` both run `driver.stop()`, so cleanup failures are logged twice. An in-loop failure gives three stop calls. They are idempotent; only the log is noisy.
3. **Client-driven log spam.** An over-budget cursor surface logs `tracing::warn!` on every frame (`cursor.rs:108`), and pointer motion now triggers a redraw. Any client can flood the log. Rate-limit it or log once per surface.
4. **Hidden reliance on full redraw.** The cursor overlay is only correct because `render_output(..., age 0, ...)` redraws the whole output every frame. If buffer age is ever passed through, the cursor will leave trails. Worth a comment where `age 0` is passed.
5. **Inconsistent clamping.** Absolute motion is clamped to `size − 1.0` but relative motion to `size − 0.001` (`input.rs:203` vs `:243`).
6. **Dead code.** `let _ = cookie;` (`native.rs:161`) checks only that a pending cookie exists, not its value.
7. **Leaked frames on pause.** On pause, `Output::drop`'s forget also leaks the front and pending `PresentedScene` (a `RuntimeScene` plus a `WlSurface` clone). This is intentional and the process is exiting, but nothing documents it at the integration level.

## Scope gaps (consistent with what you stated)
- No VT switching or session recovery. Hot-plug always ends the session (see the MEDIUM finding).
- No touch, tablet or gesture input. No cursor themes; every named cursor draws the built-in arrow. No dmabuf global, so clients are shm-only.
- Output orientation (`Transform::Normal` into a dmabuf FBO) is unverified on hardware.
- wl_output is removed immediately on shutdown (no `disable_global` first). That's acceptable only because the process is exiting.

## CI and test gaps
- **Nothing tests the new control flow.** CI runs `cargo test -p fluxwm --features native`, but that only covers option parsing and `cursor::origin`. Nothing exercises `Context`, notify, draw, the watchdog, or the cursor budget walk. A green run says nothing about `native.rs`.
- **Nested regression is local only.** No CI job runs the native-feature binary with `--backend nested`.
- **Minor:** the native test step doesn't blank `WAYLAND_SOCKET`, unlike `linux.yml:76`. `cargo doc` never builds with `--features native`.
- **Outside the snapshot:** the live worktree has `package.json` and `pnpm-lock.yaml` staged. They don't appear related to this integration; check they're meant to be in this change.
