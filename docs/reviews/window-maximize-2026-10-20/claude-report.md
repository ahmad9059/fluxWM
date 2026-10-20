I did a source-only review: 13 Read/Grep calls on the snapshot, nothing run, nothing written. This is not a certification. I found no HIGH defects. I found two MEDIUM issues that real clients can hit, several useful LOWs, and a few places I couldn't verify.

## MEDIUM

**M1. Double-click-to-maximize on the client's own title bar is declined.**
- `set_window_maximized` (`state.rs:728-732`) refuses whenever `pointer.is_grabbed()`. Smithay installs an implicit click grab while a button is held. `move_request` and `check_grab` (`xdg_shell.rs:104,305`) depend on that grab existing, which corroborates it.
- GTK3 and GTK4 run the double-click title-bar action on the *press* (`n_press == 2`). Qt does the same through its double-click event. So the request arrives while the button is down and is declined. The window gets only the forced reply configure, so the client sees "not maximized".
- The client's own click grab is being treated as an "unrelated grab". Fix: allow a pointer grab that is a click grab owned by the same client (the `same_client_as` check already used in `check_grab`). Only decline non-click grabs or grabs owned by another client.
- `set_window_minimized` has the same pattern.

**M2. Enforcing the exact geometry kills clients that otherwise work fine, and IPC maximize makes this easy to trigger.**
- `commit_window` (`state.rs:491-497`) checks every commit that has a buffer against `last_acked` with an exact `==`. It skips commits with no buffer attached, so a client that acks and then commits only a frame callback is still checked.
- The maximized size ignores the client's own min/max size (`maximized_rect` at `lib.rs:1388`). The only constraint checks are on camera position and coordinate limits.
- So `{maximize, toggle:true}` from IPC on a fixed-size client (min == max) or one with min > viewport sends a configure the client cannot obey. Its next normal commit triggers `invalid_surface_state`, which disconnects it.
- The protocol allows this error, but the claim that it "never kills a healthy client" doesn't hold here.
- Options, pick one:
  - Clamp the maximized configure size to the client's constraints, or decline maximize for constraint-bound windows.
  - At minimum, decline compositor-initiated maximize when min > viewport or max < viewport.
  - Consider only logging, not killing, when the commit doesn't attach a new buffer.

## LOW

- **L1. Pre-map maximize size disagrees with the mapped size under canvas zoom.** The pre-map path (`state.rs:739-744`) sends the raw `viewport()`, while after mapping the size is `viewport / zoom` (`lib.rs:1389`). With zoom ≠ 1 this costs one extra resize configure on first map. It is correct, just wasteful.
- **L2. A window maximized before mapping can get stuck unmapped.** The pre-map path doesn't check `maximized_rect`. If the camera is near `MAX_COORD`, the map-time `set_maximized` fails (`state.rs:518-526`). `relayout` then retries every time with `initial_maximized` still true, so the window stays unmapped until the camera moves or the client unmaximizes. This is hard to reach.
- **L3. Startup maximize is dropped during unrelated grabs.** A client's own pre-map maximize is declined while a popup keyboard grab from another client is active (`state.rs:728`). The client then maps unmaximized with no retry.
- **L4. The error goes to an arbitrary `wm_base`.** `reject_invalid_maximized_size` posts it on the first `wm_base` found for the client, not necessarily the one that owns the surface. The effect is the same, since the whole client is disconnected. The lock handling is correct: the closure only collects the id, and `post_error` runs after it returns (`state.rs:687-703`).
- **L5. A maximized window disappears when its slot is hidden.** `placements()` overrides only `rect`/`clip` (`lib.rs:1003-1007`). `visible` still comes from the underlying slot's minimized/tab state. A maximized window in an inactive tab or minimized slot is therefore not shown. This fits the "overlay of the slot" model, but it should be stated explicitly.

## Confirmed against source
- **configure.rs reply flag.** It is set before relayout (`xdg_shell.rs:252`), consumed only when the configure slot is available (`configure.rs:41-46`), and cleared on initial send (`xdg_shell.rs:338`) and on unmap (`state.rs:542`).
- **Ack flush.** The ack flushes even when the window is unmapped, and the `initial_configure_sent` check stops a stale ack after unmap from starting a new cycle.
- **Validation target.** It uses the cached `current().last_acked` (the committed configure), not `server_pending`. Up to the size check, this matches what you described.
- **Sequence.** It uses `max(current, next_sequence)` (`actions.rs:114`).
- **IPC toggle.** `toggle:true` toggles and `false` unsets (`actions.rs:57`).
- **Capabilities.** Exactly Maximize + Minimize are advertised (`state.rs:175`).
- **Layout guards.** `set_camera`, `set_viewport` and `rebase` all check the maximized rect, and reveal clamps to `MAX_COORD - viewport/zoom`.

## Gaps I couldn't check
- **Commit call order.** No caller of `commit_window` or `handle_commit` shows up in the snapshot, so I couldn't confirm that `on_commit` runs before validation, or how `just_unmapped` is computed.
- **Smithay unmap reset.** I can't tell from here whether the pinned Smithay clears the sent Maximized state and `last_acked` on a null-buffer unmap. If it doesn't, the next cycle's first configure would still carry the old Maximized state and size.
- **Smithay click grab.** M1 assumes the pinned Smithay installs that implicit click grab. Checking `PointerHandle::button` in the pinned version would confirm it.
- **Not inspected.** I didn't read the fixture scenarios, the IPC query field wiring (`ipc.rs:89`), or the 10k randomized test body. I didn't check whether `set_viewport` failing after an output resize while something is maximized leaves the viewport stale. I didn't look at the native backend, as agreed.

**Verdict:** The configure coalescing and committed-state validation look sound. Fix M1 and M2 before relying on maximize with real GTK/Qt clients or using IPC maximize on fixed-size windows.
