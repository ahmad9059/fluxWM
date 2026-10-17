I found no reachable HIGH or MEDIUM product defect at 0d12c40. There is one MEDIUM test-oracle gap: the held-Alt case can't tell the old binding table from the new one. I used 15 Read/Grep calls, read source only, and did not run anything or rely on author logs. Your earlier M1 is closed by source; M2 is mostly closed, apart from the gap below.

## Findings

**MEDIUM (test gap, not a product bug): the Alt+a check doesn't prove the new binding table is live.**
- In `tools/smoke_config.py:279`, `lua(13, 'strip', route=name == 'input-publication')` gives the *initial* generation the same `Alt+a → workspace-mode canvas` binding. Generations 31 and 37 have it too.
- The test then:
  - checks the new generation with `binding_entries == 4` (`smoke_config.py:383`), but the initial generation also has 4;
  - presses Alt+a, which switches to canvas from any of the three tables (`:386-393`).
- So the comment at `:384` ("Execute a shortcut from the newly published user table") isn't backed by the oracle. If `drive_config` updated layout and generation but left `self.bindings` unchanged (the `mem::replace` at `crates/fluxwm/src/config.rs:374-377`), every smoke case would still pass. The baseline `reload-retention` case can't catch it either: it reloads 3 entries to 3 entries with the same chords.
- By reading the code, the swap is correct today. This is a coverage hole, not a bug.
- **Fix:** build the initial config with `route=False` (3 entries) and keep `route=True` for N and N+1. Then assert that Alt+a while N is prepared but unapplied either delivers `key 30` to the client or leaves the mode as strip. Assert the canvas switch only after release, and adjust the key counts at `:408` to match.

## What I checked (by source)

**Geometry rollback (your M1): closed.**
- The fixture sets minimum width 999 999 984 (`fluxwm-fixture/src/main.rs:2950`).
- `check_strip_extent` adds `min + gap` per column and rejects anything over `MAX_COORD = 1e9` (`flux-layout/src/lib.rs:1037-1044`). The numbers work out:

| Gap | Extent | Result |
|---|---|---|
| 13 | 999 999 997 | accepted |
| 32 | 1 000 000 016 | rejected |
| 8 | under the limit | accepted |

- Gap 32 is under the worker's 1024 cap (`config.rs:239`), so the only way to get the asserted error string is `Layout::reconfigure` on the dispatch-side clone (`config.rs:350-357`).
- The harness waits for `applied` before starting the fixture, gates on title, `mapped` and width, and then asserts:
  - the exact error string;
  - an unchanged `applied_generation`;
  - unchanged effective gap, mode and `binding_entries` (3, where the rejected config had 4, so leaked bindings would be caught);
  - unchanged window `id`, `geometry`, `mapped`, `focused` and `workspace`;
  - recovery at gap 8 under `assert_applied`.
- The fixture stays alive for 6.5 s after ready, exits 0 and checks `state.error`.

**Gap 1024/1025 boundary:** 1024 is checked with `assert_applied`. 1025 is checked with the exact message (`smoke_config.py:302`), and the effective state matches the 1024 snapshot.

**Held Alt / supersession (your M2):**
- `prepared_generation` is shown only when `completion.gen == latest` and the result is Ok (`config.rs:151`). Waiting for `prepared == N` therefore proves the worker finished N.
- The test samples for 0.35 s that the status is `pending` and the applied generation is still the old one.
- After N+1 is requested, it waits for `prepared == N+1` and checks the applied generation is still the old one.
- `drive_config` runs every dispatch (`state.rs:821`) and stops while any key is pressed (`config.rs:333`).
- After release it polls until `applied == N+1` and checks the gap-37 config.

**Unit tests:**
- `worker_publication_rejects_stale_and_closed_generations` exercises the `closed || latest != generation` gate in `publish` (`config.rs:283`) for stale, current and closed cases.
- `accepted_request_hides_prepared_generation_before_worker_observes_request`:
  - The parked worker keeps `is_finished()` false, so the stopped-worker fallback at `config.rs:105` can't hide the result.
  - It fails if the `completion.0 != latest` check at `:108` is removed.
  - Shutdown sets `closed` and notifies, then joins.

**Other controls:**
- `retire` asserts its slot is empty, and `completion()` returns nothing while that slot is occupied. The new comment about the temporary layout clone is accurate: on success the clone becomes `self.layout`, and the clone made inside `Layout::reconfigure` drops on dispatch.
- `O_NOCTTY` is added before the descriptor's `is_file` check (`flux-config/src/coordinator.rs:147-154`). Real-file, symlink, non-UTF-8, directory, oversize and FIFO tests exist in `crates/fluxwm-config/tests/coordinator.rs:290-413`.
- `--no-config` appears in every other harness that launches fluxwm (focus, nested, scene, input, ipc_runtime, health, shutdown). `--config` and `--no-config` can't be combined (`main.rs:63-72`).

## Remaining test gaps (LOW)
1. The `prepared_generation` filter in the diagnostics output (`config.rs:151`) isn't asserted by the unit test, which only checks `completion()`. The smoke never looks between a request and the worker noticing it. Adding `assert!(config.diagnostics()["prepared_generation"].is_null())` after `request()` would cover it.
2. The "superseded generation never applied" check samples by polling, so a superseded generation that was applied and then replaced between polls would be missed. Structurally, `completion()` can only return the latest generation, so this is mainly a gap in what the evidence can show.
3. The geometry case doesn't count wire configures across the rejected reload, so "the rejected reload sent nothing to the client" isn't checked at the protocol level. It also doesn't re-check window geometry after the gap-8 recovery.
4. The geometry error string doesn't say which check fired (the extent check or a later validity check), so the test proves the dispatch-side rollback but not the exact sub-check.
5. `O_NOCTTY` itself has no test (no tty or pty source). Opening a character device with `O_NONBLOCK` before the type check can have open-time side effects. This is speculative hardening only: the path is chosen by the user and following symlinks is deliberate.
6. The 1025 failure doesn't assert that `applied_generation` equals the 1024 generation; only the effective state is compared.

## Limits
- None of this is a deterministic all-threads race test, and no production certification is claimed.
- These areas remain unreviewed or open: the full schema and device/output/keymap transactions, automatic watch/debounce, the native backend, and arbitrary scheduling, resource and soak behaviour.
- Your brief names `crates/fluxwm-config/tests/coordinator.rs` (which exists), but the source code is in `crates/flux-config/src/coordinator.rs`.
- I didn't check whether the older docs still describe the earlier three-case harness.
