# Real popup lifecycle and invalid-grab fixture

`fluxwm-fixture popup-lifecycle` exercises configured SHM popups through the actual Wayland connection. It requires dismissal of a non-grabbing popup when its root null-unmaps, rejection of a background popup's grab with serial zero without keyboard focus theft, and dismissal when the parent toplevel role is destroyed. It injects no pointer, key, or touch input.

## Run and bounds

```sh
cargo build -p fluxwm -p fluxwm-fixture
target/debug/fluxwm --backend nested --socket fluxpopup-UNIQUE \
  --exit-after-ms 6000 --stats-file /tmp/NEW-popup-stats.json \
  -- target/debug/fluxwm-fixture popup-lifecycle --timeout-ms 4500
```

Use a unique socket and nonexistent stats path. Run a new isolated nested instance without other clients or user interaction. The compositor supplies the actual nested `WAYLAND_DISPLAY` and runtime `FLUXWM_SOCKET`. The fixture does not infer or contact the host Wayland server, install input devices, fabricate an input action, or modify the parent environment.

The fixture has one monotonic deadline (default five seconds; accepted range 1–5,000 milliseconds). The first root-null-unmap dismissal additionally has a 400 millisecond local deadline after a sync proves the unmap was processed. Missing events, protocol errors, counter mismatches and unmet prerequisites fail rather than skip. The compositor process requires its own external timer/supervisor bound; the example gives the fixture time to finish before shutdown.

Resource growth is bounded by the hard-coded sequence: two root toplevels, three popup roles, at most two 64×48 popup SHM buffers, and the two existing 320×240 root buffers. The invalid-grab popup never attaches a buffer. Event evidence uses the existing 1,024-entry limit, and diagnostics use the existing nonblocking/deadline-controlled helper with a 64-KiB response ceiling. The test does not generate an arbitrary popup depth or flood unbounded client objects.

## Protocol sequence and assertions

1. Bind compositor, SHM, XDG shell, seat and keyboard. Create a root toplevel, acknowledge its configure, attach the original SHM pattern, and wait for a frame callback.
2. Create a **non-grabbing** child popup with a valid positioner: size 64×48, anchor rectangle `(20,20,1,1)`, bottom-right anchor and gravity. Wait for popup/surface configuration, acknowledge the surface configure, attach its own SHM buffer and request a frame callback. The recorded configure resolves to `(21,21,64,48)` in the observed run.
3. Attach a null buffer to the root and commit. A subsequent display sync proves the unmap request was processed. Require `xdg_popup.popup_done` within 400 milliseconds, without physical input or an explicit popup grab. Then destroy the dismissed popup in popup-role → XDG surface → wl_surface order.
4. Remap the root through a new bufferless commit and fresh configure. Map a second root and wait until a real keyboard-enter event identifies it as foreground focus owner.
5. Create a popup on the first, now-background root and send `grab(bound_seat, 0)` **before mapping**. Require `popup_done`. Use a sync barrier to process the response stream, then assert keyboard focus remains on the second root and no keyboard-enter event targeted this popup. The dismissed invalid popup is destroyed without attaching a buffer. Serial zero is deliberately invalid; it is not presented as a real user authorization.
6. Create, configure and buffer-map another non-grabbing popup on the foreground root, again requiring callback progress. Destroy the parent `xdg_toplevel` role while retaining its underlying XDG/wl_surface objects. Require popup dismissal, destroy the child popup, then finish parent-surface destruction. This ordering exercises role removal without prematurely destroying the parent surface objects referenced by the child.
7. Explicitly destroy the remaining root, cross a sync barrier, and require runtime root counters `maps=3`, `unmaps=1`, `destroyed=2`. Root remapping accounts for the third map; explicit role destruction is counted as destruction rather than an additional null-buffer unmap.

Popup state is tracked independently from root state: separate XDG-surface userdata, configure/geometry counts, SHM storage, frame callbacks, `popup_done` counts and destruction flags. Keyboard-enter evidence records only root/popup identity; it does not log key presses or keymap contents. The fixture output retains event ordering and its current stage on failure so a missing initial configure/frame cannot masquerade as a dismissal failure.

The pinned `wayland-protocols` 0.32.13 stable `xdg-shell.xml` says a denied popup grab is immediately dismissed; `xdg_popup.popup_done` indicates compositor dismissal and the client should then destroy the popup. It also defines `xdg_toplevel.destroy` as role destruction and unmapping. This fixture checks FluxWM's selected root-lifetime dismissal policy through actual client events; a source-only manager change is insufficient evidence.

## Actual pre-fix failure and post-fix pass

On 2026-10-17 the **same fixture executable** ran against the root binary before and after the popup-tree dismissal correction:

| Provenance | SHA-256 |
| --- | --- |
| Fixture used unchanged in both runs | `6128fbb5a0c4722f5da39c5856abb1beb9490bcb67a19f80455b136681294708` |
| Root before correction | `09c5ec4b5b1de9fd7a426bd9d95a953a6d48a5b0b0041a35640d3b14913b3e5c` |
| Root after correction | `e7441c78ef284ff7100d72b3b03fb6ed469f178804ed6f12ee459d594a5e3e6a` |

Before correction, the trace showed a mapped root, configured popup geometry `(21,21,64,48)`, popup configure acknowledgement, popup buffer commit and popup frame callback. The root's null-unmap crossed its sync barrier, but popup zero received **no `popup_done`** within 400 milliseconds. The fixture failed at stage `root-null-unmap-popup-done` after 453 milliseconds; the compositor propagated its supervised-command failure and exited 1. Later cases were not reached and are not claimed by that run.

After correction, the complete mode passed in 85 milliseconds and the compositor exited 0. Observed results:

| Popup | Case | Configure / geometry events | Frame callbacks | `popup_done` | Explicitly destroyed |
| --- | --- | --- | --- | --- | --- |
| 0 | Non-grabbing root null-unmap | 1 / 1 | 1 | 1 | yes |
| 1 | Background root, invalid grab serial zero | 1 / 1 | 0 | 1 | yes |
| 2 | Non-grabbing parent-role destruction | 1 / 1 | 1 | 1 | yes |

Keyboard-enter events targeted only roots. The invalid background grab left keyboard focus on the foreground root, and the complete run recorded **zero popup keyboard enters**. The root remap received a fresh configure. Final runtime counters were maps 3, unmaps 1, destroyed 2, Wayland requests 122, redraws 6, compositions 6, and submissions 4.

The invalid-grab trace records `popup_done` before its subsequent initial popup configure/ack. This ordering is retained as observed evidence; the fixture destroys the dismissed popup after processing its event/sync barrier. Immediate client destruction in the event handler and all possible in-flight configure/destruction races remain separate tests.

Raw local evidence, including exact commands, binary hashes, fixture stdout, compositor stderr, readiness and final stats:

- Before: `/home/ahmad/Documents/Projects/fluxwm-layout/target/popup-lifecycle/9447774c9ebc49e68cae4cb3ce0f9091/`
- After: `/home/ahmad/Documents/Projects/fluxwm-layout/target/popup-lifecycle/d783969b4801429381bb79f74f8078b3/`

These identify historical debug binaries in a development workflow, not a production or committed-release qualification. Final integrated builds must rerun the mode and retain their own provenance.

## Coverage limits and checks

This case verifies client-visible popup configuration, SHM commit/callback progress, dismissal, keyboard focus identity and root cleanup in the nested backend. Frame callbacks do not establish host presentation or visual correctness. It does not prove pointer focus preservation, successful grabs authorized by real input, nested popup chains, lock/overview cancellation, workspace hiding, scaled/rotated positioning, output removal, capture/privacy handling, or delayed client destruction/remap resurrection. The normal case destroys a dismissed popup before remapping its root, so it cannot substitute for a delayed-destruction regression.

`cargo test -p fluxwm-fixture` passes the existing six deadline/parser/SHM checks; `cargo clippy -p fluxwm-fixture --all-targets -- -D warnings` passes. Those checks support fixture infrastructure. The before/after real connection runs above provide the popup-specific evidence. Existing lifecycle, slow-reader and pointer-feedback modes remain available and do not bind the new keyboard observer unless popup mode is selected.
