I've finished reading; here is the report.

# Review: host-focus cancellation and real-input regression (snapshot 9c4c7331…)

I only read files. I ran no commands, tests or builds and computed no hashes. Nothing here certifies the code for production. Smithay (pinned git rev `023b78b1…`) and winit `0.31.0-beta.3` are not vendored in the snapshot, so anything I say about how they behave internally is inference.

## What I inspected
- `crates/fluxwm/src/input.rs`: all of it, including `host_focus_changed`, the key filter in `process_input_event`, how buttons are admitted, and `refresh_pointer_focus`.
- `crates/fluxwm/src/state.rs:590-726`: `cleanup_popup_grab` and `finish_dispatch`. I only grepped the field declarations.
- `crates/fluxwm/src/handlers/xdg_shell.rs:165-248`: popup `grab`, which checks that the grab's serial came from a tracked press, and `popup_destroyed`.
- `crates/fluxwm/src/winit.rs:95-140`: the Focus hook.
- `crates/fluxwm/src/ipc.rs:105`: only the `input_state` fields, pulled out with a regex. The line is too long to read whole.
- `crates/fluxwm/src/handlers/mod.rs`: grep only, which shows `DataDeviceHandler` and `WaylandDndGrabHandler` are implemented.
- `crates/fluxwm-fixture/src/main.rs:776-839`: the input-probe keyboard journal.
- `tools/smoke_focus.py`: all of it.
- `tools/smoke_input.py:160-254`: the focus action, the `activate` flag, the key whitelist and `Session.test`.
- `docs/development/host-focus.md`: all of it.

## New high/medium findings

### M1 (medium, inference about Smithay): losing host focus during a drag-and-drop performs the drop instead of cancelling it
- **Where:** `input.rs` `host_focus_changed`, lines 52-67. Each tracked button gets `pointer.button(Released)` through whatever grab is active, and the unconditional `unset_grab` comes later (lines 73-77).
- **Trigger:** A client starts `wl_data_device.start_drag`. DnD is reachable because `WaylandDndGrabHandler` is implemented. The pointer moves over a target that accepts the drag, and then the host window loses focus.
- **What goes wrong:** Smithay's DnD grab treats the final button release as a drop. If the target has accepted, it sends `drop` to the target and `dnd_drop_performed` to the source. So something meant as a cancellation ends up committing a data transfer. The later `unset_grab` comes too late to stop it.
- **Same pattern, by policy:** ordinary click grabs also deliver a real-looking release, and toolkits that activate on release will treat it as a completed click. The stated policy covers that case, so I'm not counting it as a finding. Move and resize grabs are excluded from scope.
- **Test that would close it:** The fixture starts a drag from A with a mime offer and enters B, which accepts. Then a host focus loss happens. Assert that B gets `leave` and no `drop`, the source gets `cancelled` and no `dnd_drop_performed`, and `pointer_grabbed` is false afterwards. One possible fix: before releasing buttons, if the active grab is the DnD grab, unset it (which cancels it) first.

### M2 (medium, test oracle): `pressed_buttons` reports the compositor's own bookkeeping, not the seat's real state
- **Where:** `ipc.rs:105` reports `pressed_buttons` as `self.pointer_presses.len()`. `host_focus_changed` clears that map unconditionally at line 68. `smoke_focus.py:156` and `:171` then assert it is 0.
- **Problem:** The check passes no matter what happened. It can't tell whether Smithay's own pointer state still thinks the button is down, for example if a grab swallowed the release.
- **What does cover it:** The client-side release at `smoke_focus.py:137`, the B-enter check at `:147`, and the final counts at `:169` cover the plain click-grab path.
- **Gap:** No check would notice leftover state on any other grab path. `keyboard_grabbed` is reported but never asserted.
- **Test that would close it:** Assert `keyboard_grabbed is False` after the loss. Either expose Smithay's own pressed-button count or add a check that a fresh press on B delivers `pointer.enter` before the button event.

### M3 (medium, test coverage): popup-chain cancellation has no test
- **Where:** `host_focus_changed` lines 69-72 call `ungrab(All)` and then `cleanup_popup_grab`.
- **Problem:** That path, which also restores keyboard focus, runs in neither case even though the docs at line 7 claim it. Code paths I checked that this test would also exercise:
  - Keys are released while the `PopupKeyboardGrab` is still installed. This ordering is correct: the keyboard enter that `restore_keyboard_focus` sends later carries an empty key array.
  - After the loss, an xdg popup grab request is refused because its serial no longer matches a tracked press (`xdg_shell.rs:181`).
- **Test that would close it:** The fixture opens a popup grab from a press on A, while also holding a forwarded key. After the host focus loss, assert `popup_done`, the key release goes to the popup surface, keyboard enter returns to A with no keys held, both grab flags are false, and a later popup request using the old serial gets `popup_done`.

## Lower-severity gaps (not counted as findings)
- **Shortcut key still held when focus returns:** Hold Alt+C, lose focus, regain it while C is still held, then release C. The suppression set is already cleared, so the filter returns `Forward` (`input.rs:136-141`). Whether the client then sees an unpaired release depends on Smithay dropping releases for keys it doesn't consider pressed. The held-A check (`smoke_focus.py:128-130` plus the counts at `:165`) tests that behaviour only for a key that had been forwarded. The C path is untested, because the test releases C while focus is on the root window (`activate=False`).
- **Alt held while focus returns:** After cancellation, the compositor's keyboard state has no Alt. Pressing C then sends a plain `c` to the client, and the real Alt release is dropped. This follows from policy (winit filters synthetic keys), but it's worth documenting as user-visible.
- **Fresh click while unfocused:** The docs say a click is accepted before the focus notification arrives. The test only clicks after `focus=True`.
- **Untested doc claims:** That repeated identical notifications are idempotent, and that the IPC sequence advances without a redraw. I checked the code paths for both (`input.rs:20-24`, `bump_sequence`), but no test asserts either.
- **Fixed sleeps:** The 150 ms negative checks (`smoke_focus.py:129, 150`) are weak on their own, but the full-journal counts at the end close them.

## What I checked and found correct
- Duplicate notifications return early, and the sequence is bumped on every real change.
- Releases are sorted by keycode and go through the normal key filter. Releases of suppressed keys are intercepted, and the suppression set is cleared afterwards.
- Releasing held modifiers should reset the keyboard (XKB) state and send a modifiers event to the client. That part is inference about Smithay, but the client-side `depressed==0` check (`:104`) and the stale-Alt check (`:116`) test it.
- Keyboard input is blocked only while focus is known to be lost (`Some(false)`). Initial `None` lets input through. Pointer motion, wheel and buttons are never gated by focus.
- Unmatched and duplicate physical button events are dropped (`input.rs:251-257`). Clearing `pointer_presses` also invalidates press serials for popup grabs.
- The click grab ends, `pointer_focus_dirty` is set, and focus is repaired through `finish_dispatch`.
- The journal validates the key array in `keyboard.enter` (bounded, length a multiple of 4) and records key, modifier and leave events. Only the fixture knows key identities; the compositor reports counts.
- The helper's key and button whitelists, the root-targeted `activate=False` path, and the `XGetInputFocus` confirmation all match the docs. The final cleanup in `finally` releases keys without re-activating the window.

## Not verified
- How Smithay at the pinned revision actually handles: tracking of forwarded versus all pressed keys, keyboard-state updates for keys that aren't held, `ClickGrab` restore, and DnD release and unset.
- Whether winit filters synthetic and repeat keys (I'm taking that from your statement), and how X11 `NotifyGrab`/`NotifyUngrab` focus events map onto `Focus` events.
- The rest of `ipc.rs:105`.
- Evidence bundles, the frozen hashes, and every run result claimed in the docs. I didn't read or trust any of them.
