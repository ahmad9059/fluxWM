I found no reachable HIGH or MEDIUM defects in the default-width change. This is a read-only review based on 17 Read/Grep calls inside the snapshot. I ran nothing, including the runner script at the end of your message. So the pass recorded in the doc (`docs/development/config-widths.md:35`: "first private run passes … 400/760/320/600/600/480") is the author's claim, and I haven't confirmed it. Plan mode asked for a plan file and an ExitPlanMode call, but this session has only Read and Grep, so the review is below instead.

## Defects
**HIGH / MEDIUM:** none.

**LOW (real but minor):**
1. **Wrong function comment.** `crates/fluxwm-fixture/src/main.rs:2937-2938`: the comment on `config_width` is copied from a rejection/retry fixture ("Layout rejection … retry smaller hints"). That isn't what this fixture does.
2. **Doc overstates how Canvas sizes a new group.** In `crates/flux-layout/src/lib.rs:408-418` the new Canvas group copies the window's strip rectangle *after* the client's min/max limits are applied, not the raw policy width. The doc (`config-widths.md:16`) says the group's size is seeded "from that policy". The two differ only for clients with limits. The behaviour itself is older.
3. **Possible rounding mismatch in the harness.** `tools/smoke_width.py:81` compares with Python `round()`, which rounds .5 to the nearest even number. The compositor uses Rust `.round()`, which rounds .5 away from zero (`state.rs:574`). If the viewport width is 2 more than a multiple of 4, a quarter of it ends in .5 and the two can differ by 1px. The 1280 viewport the doc implies gives exactly 320, so this only matters on other viewport sizes.

## Test gaps (oracle, not product defects)
- **Invalid preset 99 is caught by the schema, not the new code.** `schema.rs:474` rejects `preset=99` before the worker runs. So the harness re-proves atomicity on schema-invalid reloads, which already existed. It does not reach `resolve_default_width` errors or the dispatch error "configuration layout would invalidate live geometry". In practice the default width can't trigger that dispatch path anyway: widths are capped at 65536px and 16 × a realistic viewport is far below `MAX_COORD` (1e9). It is checked only by unit tests.
- **Remap and retry of rejected windows are untested at runtime.** The doc claims both. The code path is there: `relayout` (`state.rs:525-563`) is called after a successful apply and re-inserts with the current default. But no harness drives an unmap/remap or a rejected-then-recovered admission.
- **Only each window's final configure width is checked.** Brief wrong-size configures sent to existing windows during a reload would go unnoticed, and heights aren't checked. IPC geometry and identity checks (`smoke_width.py:47-50`) do cover keeping existing widths at every step.
- **No runtime test of a viewport change with fraction columns.** Only the unit test at `lib.rs:1284-1291` covers it.
- **Fixtures don't exercise client size limits together with a configured default.** Only the unit test at `lib.rs:1343` does.
- **The `{preset=2}` desired shape in `smoke_config.py:191` is exact-matched.** Any change to the schema default breaks it on purpose; that strictness is documented at line 188.

## Controls checked and holding
- **Atomic publication.** `reconfigure_with_default_width` (`lib.rs:265-297`) checks the default first, builds a full candidate copy, and validates it, including the strip extent and the saved geometry for the inactive mode in both modes, before `*self = candidate`. Dispatch also works on a cloned layout and swaps it in only on success (`config.rs:385-425`).
- **Single source for admission width.** `insert_with_focus` (`lib.rs:376-406`) uses the same `self.default_width` for the space check and for the new column. Errors before any change leave the model untouched, which the unit test `lib.rs:1324-1329` asserts.
- **Validation and viewport changes.** `validate` and `set_viewport` reject a default that can't be resolved. For schema-legal values this can't happen, so the winit resize path (`winit.rs:120-125`) gains no reachable new `fail`.
- **Preset resolution.** `checked_sub(1)` handles 0, out-of-range values and `usize::MAX`. Preset-in-preset is rejected in both the schema (`allow_preset=false`) and the worker.
- **Defaults match.** The schema default (preset 2 = fraction 0.5) equals `Layout::new`'s 0.5. Failed startup reports `{fraction: .5}` (`smoke_config.py:223`).
- **Diagnostics.** "desired" keeps the requested reference; "effective" reports the resolved fixed or fraction policy. Settings that aren't applied are still listed explicitly.
- **Existing columns.** Each keeps its own copied policy. Fixed columns stay fixed, and fraction columns follow the viewport.
- **Harness design.** The six fixture steps wait on effective state, and step 4 also waits for status `failed`. It cross-checks real IPC IDs and geometry, fixture frame counts, and the configure widths actually received over the wire.

## Limits
- I did not read all the rest of `lib.rs` (`set_mode`, `strip_placements`, the body of `check_strip_extent`), the `smoke_config.py` `Case` helpers, or the config unit tests in full.
- Not assessed, as you scoped: full schema coverage, devices/outputs, native and presentation behaviour, scheduling, resource use and soak testing.
- This is not a production certification.
