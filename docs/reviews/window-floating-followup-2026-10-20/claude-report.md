I found no HIGH issues in the correction scope and one MEDIUM. I used 13 Read/Grep calls inside the snapshot, with no commands, writes or agents. This is a source review only; I ran no tests, and the tests the docs describe aren't proof that anything works.

## Corrections that check out in source

- **M1 (ordinary tab overwritten): fixed.** `ordinary_active` (`crates/flux-layout/src/lib.rs:1099`) skips windows that are minimized, floating or fullscreen. `focus_window_inner` (`:1134-1142`) only changes `active` when the target is an ordinary window, or when the current `active` is minimized. `remove_inner` and `set_minimized_inner` also prefer ordinary members first. `set_fullscreen(false)` and `set_floating(false)` (`:800`, `:852`) don't touch `active` while the other overlay still applies.
- **M2 (floating frontier shifts fullscreen pages and later tiles): fixed.** `fullscreen_projection` (`:957`) handles floating rectangles in a separate final loop (`:1076`). Neither the strip position nor the canvas page position depends on them.
- **M3 (born-floating dialog empty column): fixed for strip mode.**
  - A column whose members are all born-floating is "dormant". It doesn't advance the strip position (`:1040`, `:1067`, `:2051`), is left out of the canvas page frontier (`:966`) and the strip extent check (`:1971`), is skipped by the first canvas seeding (`:1214`), and gets no canvas group because `ordinary_active` returns nothing for it.
  - `insert_floating_with_focus` (`:417`) works on a cloned candidate and never exposes interim tiled focus.
  - `set_floating(false)` removes the born-floating mark and then runs the strip extent check on the same candidate, so materializing the slot is atomic. Line `:1896` checks that the born-floating set only contains floating windows.
- **Parent selection: correct.** `admit_layout` (`crates/fluxwm/src/state.rs:842`) picks a mapped, live parent from the same client before inserting. `initial_floating_rect` (`:961`) uses the actual client geometry, clamped to its size limits. It centers on the parent only if the parent is visible and overlaps the viewport; otherwise it centers on the viewport. Neighbouring tiles keep their geometry because the dormant column takes up no strip space.
- **M4 (manual float from overlay uses viewport buffer): fixed.** `set_window_floating` (`:1029`) passes `None` when the window is fullscreen or maximized, so the model uses its constrained 60% default (`lib.rs:830-840`).
- **M5 (Resizing persists after unmap): the original claim was wrong.** The pinned `upstream/smithay/src/wayland/shell/xdg/mod.rs:1633-1655` already resets the role state, the pending state and `server_pending` before `commit_window` runs. The explicit unset at `state.rs:622` is redundant but harmless. No configure goes out before the next bufferless commit:
  - `mapped = false` is set before `cancel_window_grab` (`:613-627`).
  - `ResizeSurfaceGrab::finish` therefore stops early at `floating_role_live` (`move_grab.rs:174`), which requires `mapped`.
  - The window is removed from the layout before `relayout`, so `relayout` sends no configure for it.
- **Pointer re-entry: correct.** Both grabs only call `set_window_floating_rect`, which uses `relayout_inner(false)` (`state.rs:1068`). `finish_dispatch` runs `cleanup_popup_grab` afterwards (`:1475`, `:1487`). The resize grab clamps against `member_constraints` and computes the opposite edge from the clamped size, which matches how `store_floating_rect` clamps.
- **Grab cancellation before ID reuse: correct.** `remove_toplevel`, the unmap path and the dead-window loop in `finish_dispatch` all call `cancel_window_grab` before anything new can be inserted. The typed downcast plus `window_id` match, together with the surface-identity check in `floating_role_live`, guards against a reused ID.

## Remaining findings

**MEDIUM — the first unfloat in Canvas mode places the tile at a stale dialog rectangle.**
- `insert_floating_with_focus` sets the dormant column's canvas group to the dialog's floating rectangle at birth (`lib.rs:465-471`).
- Nothing updates it later: floating moves and resizes only change `floating`, and first canvas seeding skips dormant columns (`:1214`).
- `set_floating(false)` doesn't reseed it.
- **Trigger:** a born-floating dialog is moved or resized, or the layout is in Canvas mode, and the user then unfloats it. The tile appears at the dialog's birth position and size, at dialog height rather than the normal canvas seeding, which places it next to the focused group (`:531-539`). It may overlap other groups. The canvas page frontier also jumps to include that rectangle.
- Strip mode is unaffected. The atomicity claim still holds; the geometry claim of a "normal slot" doesn't hold for canvas.
- **Suggested fix:** on the first unfloat of a born-floating window in Canvas mode, reseed the group's canvas rectangle the same way `insert_with_focus_inner` does.

**LOW — the materialized strip slot sits next to whatever was focused when the dialog was born.**
- Its index is fixed at birth from the focused column (`:436-439`), not from the parent or the current focus.
- After other columns are inserted or moved, unfloating can drop the tile at an unexpected position in the strip. The source doesn't document this.

**LOW — the parent-centred placement isn't clamped to the viewport.**
- `initial_floating_rect` centers on the parent's whole world rectangle when the parent only partly overlaps the viewport, and it doesn't limit the dialog's size to the viewport.
- As a result, a dialog can start partly off-screen even though its parent is visible.

I agree with the existing LOW about shared floating world coordinates and didn't re-raise it. I didn't check the fixture regressions (overlay-sized buffers then toggle with 60% restore, and the later same-parent neighbour staying unchanged) beyond confirming the layout tests at `lib.rs:4308` and `:4360` exist. I also didn't widen the review into the earlier native or security audit.
