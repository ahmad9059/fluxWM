# Fullscreen review of snapshot `466c8b1` (read-only, not certification)

**Verdict:** I found no HIGH issues. The core invariants hold in the source:
- Layout changes go through a clone, are checked against the projected extents, and only then committed.
- Each live ID gets exactly one placement.
- On the wire, Fullscreen suppresses Maximized.
- The configure reply is bounded: one slot plus one deferred reply.
- The size validator uses the committed ack state.
- A protocol error goes only to the offending client.
- The black backdrop is drawn and blocks input in the right order.

There are two MEDIUM behaviour issues and a few LOWs.

## MEDIUM

**1. In Canvas mode a fullscreen page lands on top of the neighbouring group and isn't reliably on top.**
- **Where:** The page is placed at `group.rect.x + width + gap`, same `y` (`crates/flux-layout/src/lib.rs:822-830`). New canvas groups are placed at exactly that spot (`lib.rs:460`), so the overlap is the normal case, not a corner case.
- **Stacking:** It comes from `map_element` order plus raising only the focused window (`crates/fluxwm/src/state.rs:707-725`).
- **Trigger:**
  1. In Canvas mode, map A, then B. B sits immediately right of A.
  2. Fullscreen A while A has focus. B is now hidden under A's page and the backdrop blocks clicks to it (`scene.rs:336-343`).
  3. Keyboard navigation can still reach B, because the projection says it is visible. Focusing B raises it above A's "fullscreen" page.
- **Same problem on insert:** If a new window is mapped while the focused window is fullscreen, it is placed from the saved group rect, i.e. on top of the page.
- **Fix options:** Treat the page area as occupied when placing canvas groups, or define a z-order rule for fullscreen owners.

**2. In Strip mode, directional focus changes behaviour as soon as any window anywhere is fullscreen.**
- **Where:** `lib.rs:1251` only uses the column-order path when `self.fullscreen.is_empty()`. Otherwise it scores candidates geometrically by perpendicular/forward distance.
- **Trigger A:** Columns `[A][B: 2-stack][C]`, with any window fullscreen. From A, Right picks C (perpendicular distance 0) and skips B.
- **Trigger B:** From a single-window column, Up/Down can now jump to the top or bottom window of a neighbouring column. Without fullscreen it returns `None`.
- **Fix:** Keep the column-order traversal and insert each page as a virtual column right after its owner's column.

## LOW

- **Full repaint of the backdrop on every capture:** A new `SolidColorBuffer::new` is created per capture (`scene.rs:243`), so the backdrop element gets a new `Id` each time. The damage tracker then sees the whole page as damaged on every capture. This is a performance cost, not a correctness bug. It also means damage or texture-difference tests can pass without proving minimal damage. Caching the buffer per window and page size would fix it.
- **Right edge of normal windows not bounded after pages:** For ordinary windows after pages, only the origin is checked against `MAX_COORD` (`lib.rs:842-848`, `valid_rect`). Their right edge can exceed `MAX_COORD` by up to one column width. It still fits in an i32, but reveal clamps the camera to `MAX_COORD` (`lib.rs:1731`), so the window may never be fully revealed.
- **Maximize intent can be silently dropped:** If a window is unfullscreened before it maps and `maximize_size` fails at that moment, `initial_maximized` is cleared (`state.rs:926-935`). That contradicts keeping maximize and fullscreen intent independent; the intent should be kept and the size left unset.
- **Possible stale Fullscreen state after unmap (not checked):** The unmap path clears `initial_fullscreen` and the layout state (`state.rs:582-593`), but nothing clears `State::Fullscreen` from the pending toplevel state. Whether the next initial configure resends a stale Fullscreen depends on what Smithay resets on null-buffer unmap. The Smithay source isn't in the snapshot and I couldn't confirm either way. Check the pinned `ToplevelSurfaceData` reset.

## Verified in source

- **Transactional edits:** `edit_fullscreen` / `set_fullscreen` validate the clone against both modes before committing.
- **Incompatible minimum size:** clears fullscreen on viewport, camera and constraint changes (`lib.rs:717-733`).
- **Pages:** sized to the viewport in world units; normal stack heights are allocated without the fullscreen members. An inactive original tab gets a visible page, and the original slot stays reserved.
- **Maximize overlay:** skips fullscreen windows (`lib.rs:1334`).
- **Reveal:** uses the projection.
- **Remove/unmap:** clears fullscreen.
- **Focus:** fullscreen doesn't move focus.
- **Centering:** uses the actual geometry clamped at ≥0 (`state.rs:702-706`).
- **Wrapper element:** reports empty opaque regions and no underlying storage (both trait defaults), and `draw` passes `&[]` as the opaque slice (`scene.rs:65-113`).
- **Draw order:** each owner's popups and root tree come before its backdrop, which comes before lower owners.
- **Offscreen root:** a page edge is still drawn when the root is fully offscreen (`owners` is pushed before the root/input skip).
- **Popup clipping:** popups clip to page∩viewport; ordinary popups still clip to the viewport.
- **Input barrier:** the backdrop blocks input unconditionally, while per-surface eligibility stays dynamic.
- **Frame callbacks:** `windows()` counts only client nodes, not the backdrop.
- **Validator:** fullscreen size is an upper bound and maximized must match exactly, both using `last_acked`. An unacked old-normal size is allowed.
- **Configure timing:** the forced reply is cleared on the initial configure. `commit_window` sets the pending FS size before the initial configure is sent.

## Deliberate reserved slots and documented gaps (not defects)

- An empty reserved slot stays when every member of a column is fullscreen.
- Canvas zoom ≠ 1: the client is configured at `viewport/zoom` but drawn at scale 1, so it is cropped or centered rather than scaled. This is the same as maximize and falls under "scale unqualified".
- Hardware, real toolkits, multi-output and animation are unqualified.

Unit tests and smoke scripts weren't run under this read-only scope, and they wouldn't be runtime proof anyway.
