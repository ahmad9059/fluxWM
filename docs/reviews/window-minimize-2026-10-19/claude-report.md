I found no reachable HIGH or MEDIUM defects in the code I read. There are five LOW findings, mostly gaps in what the fixture actually proves.

I used 13 Read/Grep calls inside the snapshot and ran nothing. I did not open or check the manifest, so I can't confirm the snapshot's exact bytes.

## What I checked in the source

**Layout** (`crates/flux-layout/src/lib.rs`)
- `set_minimized` (`:497`):
  - Minimizing keeps the window's slot, group and constraints. It removes the window from focus history and moves the column's active member to a visible one, or keeps it if everything in the column is minimized.
  - A real restore goes through `focus_window`, so it focuses and reveals the window. A repeated restore returns early without changing focus.
- `focus_window` rejects minimized windows. Focus fallback (`replacement_focus`), tab cycling, directional focus in both modes, `join_column` and `remove` all skip or clear minimized members.
- Canvas placements report `visible:false` for minimized windows but keep their full, non-zero geometry. `validate` enforces the rules: a column's active member can only be minimized if all its members are, stale entries are rejected, and focus must exist whenever any window is visible.

**Frontend** (`crates/fluxwm/src/state.rs`)
- `relayout` (`:589-621`) sets Suspended on minimized windows, clears Activated on anything unfocused, and unmaps hidden windows from the space while dismissing their popups.
- The pinned Smithay drops Suspended for clients older than protocol version 6 (`upstream/.../xdg/mod.rs:966-972`).
- `set_window_minimized` (`:635`):
  - Rejects requests while shutting down, and for unknown, unmapped or dead windows.
  - Declines while an unrelated keyboard or pointer grab is active, but lets the window owning the popup grab minimize itself; `cleanup_popup_grab` then runs inside `relayout` before keyboard focus is restored.

**XDG requests**: `minimize_request` (`handlers/xdg_shell.rs:251`) only looks up the requesting toplevel, so a client can only minimize its own window.

**Input and frame callbacks**
- `RuntimeScene::hit` (`scene.rs:206`) checks `layout.is_minimized` on every hit, so a minimized window can't receive input even through an older presented scene.
- `relayout` → `request_redraw` sets `pointer_focus_dirty`.
- Both backends skip frame callbacks for minimized windows (`winit.rs:309`, `native.rs:319`).

**Unmap and remap**
- A null-buffer unmap removes the window from the layout, which clears its minimized flag. `relayout` skips unmapped windows, so no stale configure is sent.
- Smithay resets the pending server state on unmap (`xdg/mod.rs:1646-1652`), so the next initial configure does not carry Suspended.
- A hidden commit with a buffer only updates constraints; it never restores the window.

**IPC and wire format**
- The action and restore paths look correct.
- The strict parser rejects `minimize` with a `window` field and bad `restore-window` IDs (`action.rs:189-192`).

## Findings (all LOW)

1. **Suspended is never checked on the wire.** The fixture does not mention Suspended at all. The claim "relayout sets Suspended" is supported by source reading only.
2. **The "nothing focused when all windows are hidden" pointer check is weak.** `pointer_focus_surface.is_none()` (`fluxwm-fixture/src/main.rs:3122`) is checked right after a roundtrip. Pointer focus is only recomputed on the next frame or motion, and the fixture never shows the pointer was over a window beforehand, so this check could pass even if pointer focus were broken.
3. **Restoring from the all-hidden state only checks frames.** The test after `:3127` waits for a new frame but never checks that keyboard focus comes back or that Activated is set again.
4. **The stale-ID test uses a destroyed window, not a reused slot.** `:3183` restores an ID whose window was destroyed. It never checks that an old ID is rejected once a new window reuses the same slot index with a different generation.
5. **Moving past a minimized column swaps over it** (`lib.rs:835-861`). In strip mode, moving left or right exchanges with the nearest visible column: with A, then minimized M, then B, moving A right gives B, M, A. B jumps over the reserved slot. That doesn't break the "no compaction" rule, but it's a choice worth writing down; the same applies to up/down within a stack.

Not a defect, just to note: `:729` is a shared check that now expects every window's capabilities to be exactly `[Minimize]`, which affects other fixture modes too. That's correct for a compositor-wide setting, but I couldn't confirm those modes still pass because I didn't run anything.

## Remaining gaps
- **Desktop and task UI**: there is no taskbar or minimized-window list beyond IPC `windows.list`, no shortcut-driven restore, and no foreign-toplevel or other external protocol for restoring windows.
- **Native hardware**: the native backend's frame-callback filter was only read, not run. The evidence is the private, authenticated Xvfb run only, and I'm not claiming anything about real hardware.
- **Not reviewed**: the docs' claims and the smoke scripts' output. I didn't re-audit the native foundation, and I'm not certifying the full phase.

**Verdict:** within this scope, the minimize/restore ownership, focus and input-hiding logic matches the stated policy, and I found nothing reachable at MEDIUM or above. Tightening the fixture on items 1–4 would make it actually back the claims it is meant to support.
