# Floating-window review: snapshot based on 9cb497f (source only)

**Verdict:** I found no HIGH issues. The core design holds up in the code: model edits are all-or-nothing, the grab checks are thorough, and windows are matched by generation-checked IDs plus the surface. There are five MEDIUM issues where layout state or visible output goes wrong in reachable cases. Fix those before calling floating complete. This is a read-only review: I didn't run anything, and I couldn't check Smithay's internals beyond what the code here shows.

## MEDIUM

**M1. Focusing a floating window changes which tab its old column shows.**
- `focus_window_inner` (`crates/flux-layout/src/lib.rs:1012`) sets `columns[i].active` to the floating window.
- The tab fallbacks (`lib.rs:884-888`, `lib.rs:1429-1433`) then skip that window and show the first ordinary tab instead.
- Trigger: a tabbed column [A, B, F] showing B; F is floating; focus F (by ID, by direction, or by click). The column now shows A. Unfloating F later makes F the active tab.
- `remove_inner`/`set_minimized_inner` (`lib.rs:511`, `lib.rs:573`) can also pick a floating sibling as `active`.
- `canvas_groups()` (`lib.rs:1137`) reports a floating window as a group's `active`.
- Fix: don't let `active` point at a floating or fullscreen window, or keep a separate "last ordinary active".

**M2. Moving a floating window shifts unrelated tiles when anything is fullscreen in Strip mode.**
- The fullscreen page start is `max(column right edge, rightmost floating edge) + gap` (`lib.rs:928-935`). Then `strip_x = page_x` (`lib.rs:956`), so every later column is placed past the floating windows too.
- Trigger: one window is fullscreen but not focused, and you drag or keyboard-move a floating window right.
- Result: the fullscreen page and every later tile slide with each motion event. The camera doesn't follow, because `set_floating_rect` only reveals when the focused window is fullscreen (`lib.rs:771`). Tiles you were looking at slide off-screen during a drag.
- The Canvas page bank (`lib.rs:847-860`) behaves the same way.
- Fix: limit the floating-edge check to the page itself, not the running strip position, or only count floating windows that vertically overlap the page.

**M3. Every auto-floated dialog leaves an empty, column-sized gap in the strip.**
- Admission runs `insert_with_focus` (which creates a new column next to the focused one) before `set_floating` (`state.rs:585`, `state.rs:861-863`).
- That column's width is still reserved even with no tiled members (`lib.rs:862-877`), and `check_strip_extent` counts it.
- Result: each transient dialog pushes all tiles to its right over by a default column width. Insertion also briefly reveals the empty slot, so the camera jumps twice.
- Reserving the slot is the intended behaviour for windows that were tiles first. For windows that were never tiled, it's a visible regression. Either give born-floating windows a zero-width slot or document it as accepted.

**M4. Floating a maximized or fullscreen window saves the overlay size as its floating size.**
- `set_window_floating` calls `initial_floating_rect(id, None)` (`state.rs:1010`), which uses the window's current committed geometry (`state.rs:951`).
- For a maximized or fullscreen window that geometry is the whole viewport. For a stack tile it's usually the full output height.
- After un-maximizing, the floating window stays viewport-sized and covers everything. The layout's own 0.6×viewport default (`lib.rs:734-737`) is never used, because the state code always passes `Some`.
- Fix: seed from the tile slot or the 0.6× default whenever the window is in an overlay, or always on manual toggle.

**M5. The `Resizing` flag survives an unmap during a resize and carries into the next mapping.**
- On unmap, `item.mapped = false` is set (`state.rs:615`) before `cancel_window_grab` (`state.rs:623`).
- `ResizeSurfaceGrab::finish` then fails `floating_role_live` and returns early (`resize_grab.rs:84`), so `Resizing` is never cleared.
- Nothing else ever clears `Resizing`: `relayout` only sets or clears fullscreen, maximized, suspended and activated (`state.rs:718-734`).
- Trigger: the client attaches a null buffer during a resize, then maps again. Every configure for the rest of that toplevel's life says `Resizing`.
- Fix: in `finish`, clear `Resizing` whenever the surface is still alive, regardless of the live-mapping checks.

## LOW

- **L1. `rebase_canvas` moves floating windows but not the strip camera** (`lib.rs:1716-1720`). Floating windows share one world rectangle across both modes, so after a Canvas rebase they jump in Strip mode. Nothing outside tests calls it today, so this is a latent API problem.
- **L2. One rectangle, two cameras.** A window floated in Canvas mode sits at Canvas-camera coordinates. In Strip mode, an unfocused floating window can be off-screen with no way to reveal it spatially. Revealing a focused floating window in Strip mode (`lib.rs:1980-1988`) moves `strip_camera.y` away from 0, which shifts every tile vertically until a tile is focused again.
- **L3. Move/resize authorization only checks the client, not the surface.** `check_grab` (`xdg_shell.rs:361`) and `owns_click_grab` (`state.rs:946`) only require the same client. A press on window A can start a move on the same client's floating dialog B. Exposure is limited to the same client.
- **L4. Left/top-edge anchoring can drift.** The resize grab clamps using the client's current hints (`resize_grab.rs:121-163`). `store_floating_rect` then clamps again using the layout's stored constraints. Those differ if a constraint update was rejected (`state.rs:635`), and then the right/bottom anchor moves.
- **L5. Dialogs can open off-screen if the parent is minimized.** The dialog is centred on the parent's hidden slot (`state.rs:965-971`). Without activation (keyboard grabbed), it can map off-screen.
- **L6. Pointer re-entrancy looks safe, but only by an unwritten assumption.** During a grab, motion calls `set_window_floating_rect` → `relayout` → `cleanup_popup_grab` (`state.rs:801`), which can call `pointer.with_grab` (`state.rs:1362`) while Smithay's pointer lock is held. I found no reachable trigger:
  - a popup grab can't coexist with a move/resize grab (a popup grab with the same serial replaces it, and one with a different serial fails `check_grab`);
  - a finished popup grab is cleaned up every loop.

  Worth a comment or a debug assertion so a future change doesn't create a deadlock.

## Checked and fine

- **Model atomicity:** edits run on a cloned candidate, both modes are pre-checked, and right/bottom edges are bounds-checked.
- **Removal and constraints:** `remove` clears the floating entry, and constraint changes re-clamp it.
- **Window lifetime:**
  - grabs end on unmap, destroy and the dead-window sweep (`state.rs:493`, `state.rs:623`, `state.rs:1418`);
  - `floating_role_live` checks the generational ID and surface equality, so a reused role isn't configured.
- **Grab admission:** requires this seat, a held press serial, an active ClickGrab, no keyboard grab, and a live floating window that isn't fullscreen, maximized or minimized. The legacy Space write is gone.
- **Actions during grabs:** geometry actions are rejected, and floating/fullscreen/maximize/minimize changes go through helpers that reject grabs other than the window's own ClickGrab.
- **Stacking:** `floating_z` holds only live floating windows, raises them above the focused tile in most-recently-focused order, and raises a focused fullscreen window last.

Your list of unverified areas (real drags, resize sequencing across toolkits, native hardware, multiple outputs, scaling) stands. As you noted, the magenta fullscreen frame doesn't prove overlap is suppressed — and given M2, the page position depends on where the floating windows are.
