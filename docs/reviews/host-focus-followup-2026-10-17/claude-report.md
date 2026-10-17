I've done a read-only follow-up review of the host-focus popup work at the frozen revision `d91db8e`. I found three new medium-severity gaps in what the test proves and no high-severity issues. I didn't run any tests or hashes, and this is not a certification. I went over the suggested 8–12 calls (14 reads/greps) and stopped there.

**Scope read:** `tools/smoke_focus.py` lines 19–278; `smoke_input.py` `short_runtime`/`close_runtime` (lines 220–229); the fixture's popup probe code (`main.rs` 725–957, plus `create_popup`/`destroy_popup`); `xdg_shell.rs` `grab`/`popup_destroyed` (172–247); `state.rs` `cleanup_popup_grab` (602–681) and `request_redraw`; `input.rs` `host_focus_changed` (19–79); `scene.rs` `surface_counts`/`capture`; the `ipc.rs` diagnostics (104–117); `winit.rs` 118–140.

## Findings

**M1 – The redraw check can't tell whether the new `request_redraw` fix does anything.**
- **Trigger:** `winit.rs:129` sends the host focus event to `host_focus_changed`. Then `winit.rs:138` calls `state.request_redraw()` unconditionally after the event dispatch. That happens whether or not `state.rs:678` exists.
- **Why it matters:** The check at `smoke_focus.py:161-167` needs two things: the composition count goes up and the popup count goes to 0. The popup was already dismissed when focus was lost, and the unconditional redraw then produces a frame showing that. So the test would most likely still pass if `state.rs:678` were deleted.
- **Not why:** The "no new Wayland requests" guard is fine. It only rules out the client causing the redraw, not the host event path.
- **Caveat:** I didn't read the caller of `winit.rs:138`. The argument only needs line 138 to run after the focus event within the same dispatch, which is how the code reads.
- **Closure test:** End a popup grab without any winit event and assert the popup is recomposed away. Or count redraw requests coming from `cleanup_popup_grab` in the stats and assert that count goes up. Or do a one-off check that removing line 678 makes the case fail.

**M2 – The popup case's full-journal check only counts one thing.**
- **Trigger:** `smoke_focus.py:216-217` counts only key 30 pressed/released events on popup 0.
- **Missed regressions:**
  - The late physical A release (`:172`) reaching root 0 as an orphan release, or reaching popup 1.
  - The late physical button-1 release (`:173`) being forwarded. Pointer buttons aren't counted at all in this case, unlike `:212-214` in the pointer case.
  - S (key 31) being duplicated or reaching the wrong surface.
  - The focus-loss button release going to the wrong surface.
- **Closure test:** Add per-owner counts for key 30, key 31 and button 272 across root 0/1 and popup 0/1, mirroring the pointer case:
  - Exactly one A press/release on popup 0, and no key 30 events anywhere else.
  - Exactly one S press/release on root 0.
  - Button 272: one press on root 0, one cancel release, and nothing after the late physical release.

**M3 – The denial check accepts a denial for any reason.**
- **Trigger:** `smoke_focus.py:170` passes on any popup-done event for popup 1. But `xdg_shell.rs:172-205` has six separate refusal branches: missing seat, serial not in active presses or failed grab check, no root, unknown window, root not visible, and the popup manager refusing.
- **Missed regression:** If grabs were refused for every request after focus loss and return (for example a stale popup-manager or visibility state left over from the retained ended popup 0), the test would read that as a correct stale-serial denial.
- **Closure test:** In the same post-return state, do a fresh physical click on root 0 and require the new popup to get the grab and keyboard focus (a positive control). Or add a stats counter that tracks the reason for each denial and assert the "unauthorized serial" count went up by exactly 1.

## Checked, no new issue found
- **Stale-request path:** the request uses the real recorded press serial. The fixture destroys popup 0 and gets popup index 1 because destroyed popups stay in its list, then calls grab and commits. Focus loss clears the active-press list at `input.rs:68`, so the request is refused at `xdg_shell.rs:187`.
- **Focus-loss order:** keys are released first (delivered to the popup through its keyboard grab), then buttons, then the grab is ended and cleaned up, then any leftover pointer grab is cleared. The test's checks that keys/buttons are empty and both grab flags are false (`:197`), plus the empty-keys root re-entry check (`:158`), are tied to this real state.
- **Diagnostics source:** the popup count comes from the published scene snapshot built at composition time (`scene.rs:138`, stored at `winit.rs:270`), not recomputed when diagnostics is called. `authorized_buttons` reports the size of the active-press list.
- **Fixture branches:** popups are only created on a root-0 button-272 press while no popups exist. The stale request fires only on an S press when exactly one popup exists and it has been dismissed. The fixture sends no requests when a popup is dismissed. I didn't verify that nothing else it does on re-entry sends a request.
- **Runtime cleanup:** the runtime directory is short, under `/tmp/fluxrt-*`. The endpoint check runs after the children are terminated and before the directory is deleted. Any leftover non-directory entry fails the run (`smoke_focus.py:246`).
  - Low: if `cleanup()` itself raises, the `finally` block stops before writing `result.json`.

## Limits
- I haven't verified Smithay's `PopupManager` behaviour for ended-but-alive popups, i.e. whether they really drop out of the scene tree. I'm relying on that from the test design.
- I didn't check whether `pointer_root()` returns nothing for popup surfaces.
- Not covered and still open: DnD cancel policy (DnD is unsupported here: `dnd_requested` → `source.cancel`), native or multi-device input, the full grab matrix, and pixel-level proof of what's on screen.
- I didn't trust the author's results or hashes.
