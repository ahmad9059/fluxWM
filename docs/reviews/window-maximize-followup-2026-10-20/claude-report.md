I couldn't confirm that the M1 fix works end to end. The guard logic is right, but no call site for `finish_dispatch` turned up, and that's the only place queued minimizes get applied. M2 is fixed for the paths you described, with one LOW gap where a window can get stuck. I used 11 Read/Grep calls on the frozen source and nothing else.

## M1: requests made during a titlebar click
- **Maximize** (`state.rs:779-784`): fixed. A request is allowed during a pointer grab only if it's a `ClickGrab` that started on the same client as the target and no keyboard grab is active. Everything else gets "an input grab is active".
- **Queued minimize** (`state.rs:836-846`): the flag lifecycle holds up.
  - The flag is only set when the window is mapped, not already minimized, owns the click grab and there's no keyboard grab. Repeat requests don't queue it twice.
  - Restore clears it before checking whether the window is mapped (`:822-827`). Unmapping clears it (`:567`). A dead window's entry is removed outright (`:1065`), so a leftover flag can't fire later.
  - It's applied only when there are no input grabs and no shutdown, and the flag is cleared before `set_window_minimized` is re-entered (`:1076-1093`), so it can't loop.
- **Unverified, MEDIUM until confirmed:** the only match for `finish_dispatch` in any `.rs` file is its definition at `state.rs:1049`. I also found no `fn main` under `crates/fluxwm`, so the event-loop file seems to be outside what I could search. Check two things:
  - Whether `finish_dispatch` is called at all.
  - Whether it's called after input-backend events, not only after the Wayland client source. If it only runs after client requests, a queued minimize waits until that client sends something after the button release.
- **LOW:** if the queued minimize fails when applied, the flag is cleared without a `bump_sequence` (`:1087-1091`), so IPC can still show "pending".
- `owns_click_grab` matches on the client, not the window (`:759`). As you described, a click on one window lets the same client maximize or minimize its other windows. Fine if that's intended.

## M2: fixed-size client asked to maximize
- Mapped windows: `Layout::set_maximized` checks `maximize_size` first, and rejects without changing anything on failure (`lib.rs:593-595`).
- Not-yet-mapped windows: `set_window_maximized` checks `maximize_size` before setting the pending state (`state.rs:791-797`).
- Bufferless initial commits re-check against the hints just committed, and clear the maximized state on failure (`:483-506`).
- Later changes to constraints, viewport, camera or mode clear incompatible maximized windows (`lib.rs:342/360/653/714`). `relayout` then sends Maximized-unset to every mapped placement (`:665-669`).
- **LOW, reachable:** a client can send incompatible min/max hints in the same commit as its first buffer, after acking the maximized configure, with geometry that still matches the viewport. Then `set_maximized(id, true)` fails at `:544` and the window gets an admission error. The retry in `relayout` (`:642`) fails on every pass because `initial_maximized` stays true, and no unmaximize configure is ever sent. The window stays hidden forever. Only that client is affected and the error shows in IPC, but it contradicts the "auto-clear incompatible" rule that mapped windows follow. Suggested fix: on `InvalidConstraints` at both sites, clear `initial_maximized` and admit the window unmaximized.
- If the late hints come with different geometry, `valid_maximized_size` triggers a protocol error (`:517-523`). That follows the spec and is a deliberate limit, not a bug.
- **LOW:** the failure branch for bufferless commits changes `initial_maximized` without a `bump_sequence`, so IPC can briefly show a stale value.

## Stacking fix
`relayout` raises only the focused window after mapping all placements (`:694-700`). That stops a background maximized window from covering the focused one. Unfocused maximized windows still sit above unfocused tiles, ordered by placement. When the focused window isn't visible, a background maximized window covers everything. Both look like the intended overlay behaviour.

## Deliberate limits, not defects
- A queued minimize can wait for as long as the button is held.
- Requests during unrelated startup grabs are declined.
- The multiplexer (wm_base) error goes to the first bound `xdg_wm_base`.
- The exact-geometry check uses only committed, cached state.
- I haven't run anything. That includes real titlebar clicks, GTK/Qt, and the physical path, so none of that is proven.

## Not checked (outside the call budget)
- The fixture's remap and focus-handoff logic, and the sorted-set caps comparison.
- The fixed-size fixture.
- The IPC error text.

**Verdict:** M2 holds apart from the LOW stuck-window case. M1's guards are correct, but it isn't closed until you confirm that `finish_dispatch` is called after input dispatch.
