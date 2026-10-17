I found **no new HIGH or MEDIUM issues** in the automatic recovery added in `6435fcb`, based on reading the source only. I used 12 Read/Grep calls on the frozen snapshot and ran nothing. I did not read the copied Smithay source, so anything about Smithay internals below is my inference, not something I checked. This is not a production-readiness sign-off.

## What I checked

**Which code changes a window's state.**
- `commit_window` (`crates/fluxwm/src/state.rs:414-480`) and the new retry pass in `relayout` (`state.rs:500-533`) are the only places that set `mapped` / `admission_error` for a window that has a buffer.
- A commit with no buffer clears `admission_error` (`state.rs:429-431`). The retry pass only picks windows that are `buffered && !mapped && admission_error.is_some() && alive()`, so it can't pick up an unbuffered or dead window.
- In `remove_toplevel` (`state.rs:395-411`) and `finish_dispatch` (`state.rs:747-773`), the window is removed from the arena and the layout before `relayout` runs. So the retry pass never sees the window being destroyed. Windows that are dead but not yet collected are skipped by `alive()`.
- Every window the layout admits also gets `mapped = true` (`state.rs:448`, `526`). I found no way for the layout and the `mapped` flag to disagree.

**Failure and retry behavior.**
- A failed `insert_with_focus` changes nothing. `check_strip_extent` runs before any change, the `next_group` overflow check happens before `members`/`columns` are modified, and `focus_window` can't fail on a column that was just inserted (`crates/flux-layout/src/lib.rs:316-373`).
- So a failed retry leaves the layout and window state exactly as they were. The `expect` in the retry loop can't panic, because the loop body never removes anything from the arena.
- There is no recursion or loop between retries: one pass, no timer, and `relayout` is never called from inside itself.
- Every path where capacity can come back calls `relayout`: destroying a window, a null-buffer unmap, a constraint commit, focus/move/pan/resize/mode actions, and viewport resize (`winit.rs:126`, `actions.rs:32-53`).

**Focus and grabs.**
- Retries insert with `activate = false`. A recovered window only becomes focused if the layout had no focus at all, which is the documented rule for an empty workspace.
- `restore_keyboard_focus` does nothing while a keyboard grab is active (`state.rs:592`).
- A popup grab can't be rooted on a window that was rejected, because popups need a mapped root (`state.rs:371-375`). `cleanup_popup_grab` still runs after the new placements are published.
- Recovered columns are inserted after the focused column without calling `reveal`, so the focused window's position and the camera stay as they were.

**Whether the fixture test can actually fail** (`crates/fluxwm-fixture/src/main.rs:3069-3152`).
- The test can tell the old behavior apart from the new. Before this commit, the rejected window gets no configure and isn't in the layout after the blocker is destroyed, so the `mapped == true` check would fail.
- After the blocker is destroyed, the test sends no commit and no new buffer for the waiting window. It checks the same window ID, `focused == false`, and `admission_error == null`.
- Frame callbacks only go to windows in the scene (`winit.rs:300-315`). So `frames > 0` is a real check that the window became part of the scene, not just a counter going up.
- This is based on reading the test code. I did not run it, and I did not take the author's results on trust.

## Lower-severity notes (below the HIGH/MEDIUM bar)

1. **Retry cost.** Each `relayout` now retries every rejected window, P of them. Each attempt takes the `with_states` lock plus about C units of capacity-check work, where C is the number of columns.
   - Any commit from any client triggers this, so one client holding hundreds of rejected roles makes every other client's commits more expensive.
   - In `finish_dispatch`, `relayout` runs once per dead window (`state.rs:754-772`). A mass disconnect therefore costs roughly (dead windows) × P attempts.
   - All of this is limited by the 1,024-role cap, and the existing per-relayout placement loop already costs C per call. I don't see a clear regression.
2. **Insertion order.** When several windows recover in one pass, each is inserted right after the focused column, so they end up in reverse arena order. This is only cosmetic.
3. **Starvation.** Windows are retried in arena order, so a smaller rejected window with a lower index can keep taking the capacity a larger one is waiting for. You already excluded fairness from scope.

## Limits of this review
- I didn't build or run anything. All conclusions about runtime behavior come from reading the code.
- I didn't look at the rest of the configure-bounding code (`crate::configure::send_pending`) beyond the call sites, or at how IPC derives `focused`.
- The wall-clock fairness, global memory and full transaction/native claims you excluded are still unverified.

I didn't write the plan file: this was a review-only task and I only had Read/Grep available.
