# Independent verification of the FluxWM production-readiness audit (commit c2abdb5)

## 1. What I covered and what I didn't

**Tools and method.** I used only Read, Glob and Grep: 25 calls, single reviewer, no agents. I did not run any tests, builds, clippy or the IPC probe, and I did not recompute any SHA-256 hash. Everything I say about test results comes from reading the author's logs, which is not independent execution. Nothing here establishes runtime, protocol, hardware or GPU qualification.

**Read in full:**
- `docs/24-implementation-readiness-audit.md` and `commands.json`
- `tests.log` and `ipc-defect-reproduction.log`
- The three `docs/development/*production-audit.md` files
- `crates/flux-ipc/src/lib.rs` lines 1–700 (all non-test code and the start of the tests)
- `flux-config/src/supervisor.rs`
- `flux-config/src/schema.rs` lines 355–854
- `fluxwm-config/src/main.rs` lines 1–232
- In `crates/fluxwm/src`: `main.rs`, `state.rs`, `winit.rs` and `handlers/xdg_shell.rs`
- `docs/03-layout.md` lines 49–51
- `flux-layout/src/lib.rs` lines 270–399

**Grep-only (partial):**
- `flux-config/src/bounds.rs`
- The IPC probe's assertions
- Unwrap, unsafe and flush sites across `crates/fluxwm`, including `input.rs` and `grabs/`
- Other `flux-layout` invariant sites
- Readiness wording in `README.md` and `docs/development/README.md`
- 15 new case IDs in `edge-cases.json`

**Not covered:**
- **Source:** `input.rs`, `grabs/*`, `handlers/{compositor,mod}.rs` and `flux-core` beyond grep hits; most of `flux-layout`; `containment.rs`; `fluxctl`; the full probe source.
- **Smithay:** its upstream source is outside the repo, so I could not check the claims about `XdgShellState::new` capabilities or trait defaults.
- **Docs:** `docs/06/08/17/19/20/22*.md`, `config.md` and `ipc.md`.
- **Evidence files:** the remaining logs and `documentation-checks.json`.
- **`phase-gates.json`:** my grep for `prerequisite|"status"|"phase"` found nothing, so I did not check phase prerequisites.

## 2. Verdict

**Accuracy.** The audit is broadly accurate and, if anything, conservative. Where I checked the evidence, it matches:
- `tests.log` totals 8+2+9+11+0+3+0+3+7 = **43** tests, with 0 tests for the `fluxwm` binary.
- The probe log matches all five reported defect groups, and the probe source really does assert the defective behaviour (`dispatched == 0`, `[11, 10]`, `!decode_ok`, empty path accepted, unit actions accepted).
- IPC audit line references match the current `lib.rs` exactly.
- I found no materially incorrect high-priority finding and no overstated local control in the config helper.

**Readiness.** The code is not production-ready, and the README and `docs/development/README.md` say so honestly. A finite audit like this one cannot certify that nothing else is missing.

**Gaps.** I found four new medium-risk issues, listed next. Two of them are traps where the audit's recommended fix (idle redraw, map-on-buffer) would introduce a new failure unless it is paired with other work.

## 3. New or materially incorrect high/medium findings

### N1 (medium): client events are only flushed from the redraw path
- **Evidence:** `display_handle.flush_clients()` is called only inside `WinitEvent::Redraw` (`crates/fluxwm/src/winit.rs:106`). The `event_loop.run` callback is empty (`main.rs:33–35`), and grep finds no other flush.
- **Consequence:** keyboard and pointer events, configures and pings created during input or dispatch reach clients only when the next redraw happens. Today an unconditional redraw loop hides this. Once P04/RUN-02 is fixed with demand-driven redraw (or the host stops redrawing because the nested window is occluded or minimised), event delivery stalls.
- **Closure:** flush once per loop iteration (in the `run` callback or an idle source), separately from rendering.
- **Test:** with the renderer idle, send a key press and then a configure-triggering request; assert the client receives both within the latency budget and with zero redraws.

### N2 (medium): move/resize handlers panic if the window isn't in `Space`; the grab check compares clients, not surfaces
- **Evidence:** `handlers/xdg_shell.rs::move_request` (lines 70–76) and `resize_request` (lines 102–108) call `.find(...).unwrap()` and `element_location(...).unwrap()` on `Space`. `check_grab` (line 149) only checks `same_client_as`, even though its comment says "different surface".
- **Why it's latent today:** every toplevel is put into `Space` as soon as it is created (line 39), so the `find` never fails.
- **Why it matters:** P02's required fix (map only on a buffer) makes this reachable. A client holding a click grab on window A could send `xdg_toplevel.move` for its own unmapped window B and crash the compositor. SURF-11 describes the scenario, but no audit finding ties it to these lines or to P02's closure.
- **Closure:** make the lookups fallible and ignore the request (no panic). Require the grab focus to belong to the same toplevel tree, not just the same client.
- **Tests:** move/resize on an unmapped sibling toplevel, on a destroyed one, and with a stale serial. Assert the server stays alive and the grab state is unchanged.

### N3 (medium): `Server::poll` gives the event loop no way to know when to call it
- **Evidence:** `flux-ipc/src/lib.rs:414–494`.
  - `Server` exposes no listener or connection fds, no "work remaining" flag and no next-deadline value.
  - `process` decodes at most 16 frames per call (line 563). A 17th complete frame left in the buffer won't make the socket readable again.
  - Deadlines (lines 537–543) only fire when `poll` happens to run.
  - `accept` errors such as EMFILE just `break` (line 489), which spins forever if the listener is registered level-triggered.
- **Consequence:** an integrator must either busy-poll on a timer, which conflicts with RUN-02's idle goal, or accept requests stalling. IPC-A05 and A07 cover fairness and visibility but not this wakeup contract.
- **Closure:** expose fds (or a calloop source), return `{more_work, next_deadline}` from `poll`, and back off accepts after EMFILE.
- **Tests:** 17+ frames in one write followed by silence; a deadline with no further I/O; fd exhaustion. Assert completion without timer polling and no CPU spin.

### N4 (medium): no read backpressure, so pipelining clients get disconnected after their actions already ran
- **Evidence:** `process` reads up to `IO_BUDGET` (64 KiB) every poll no matter how much is still undecoded (lines 547–562), but handles only 16 frames. `Decoder::push` then fails once the buffer passes `MAX_PAYLOAD + 4 + IO_BUDGET` (line 364).
- **Consequence:** a client that pipelines many small, valid requests is cut off after about 1.1 MiB of backlog, losing queued replies for mutations that were already applied. The kernel socket buffer's natural backpressure is bypassed. This makes IPC-A04's ambiguity worse; the receive-buffer limit is really a pipelining limit with no stated contract.
- **Closure:** stop reading while a complete frame (or a maximum-size frame) is buffered, and document a pipelining limit.
- **Test:** 10,000 pipelined unique-ID requests from a reader that keeps up. Assert every response arrives in order and the connection is never dropped.

### Medium-low (partly in A04/A09)
- **Malformed envelopes get no typed error.** `decode(&frame)?` (line 567) closes the connection on any envelope decode failure: an unknown top-level field, `version` as a string, a v2-shaped request. So `UnsupportedVersion` is only reachable for envelopes shaped like v1, and future clients can't negotiate a version. The v1 compatibility policy needs a minimal, permissive pre-parse of `{version, id}`.
- **`set_var` timing.** `main.rs:28` calls `set_var` after `init_winit`, by which point EGL/Mesa may already have started threads. That breaks the single-threaded precondition behind `set_var` being `unsafe`. P06 flags the mutation but not this soundness condition; its required closure (pass the environment only to spawned children via `Command::env`) fixes both.

## 4. Author findings I checked and confirmed
- **Test count:** 43 tests, matching the log; the `fluxwm` binary has 0 tests and was not launched.
- **IPC-A01** (EOF discards complete requests): confirmed. `Ok(0)` returns an error before decoding (line 549).
- **IPC-A02** (sequence can go backward): confirmed. Transport errors use the `state_sequence` value captured before the poll (lines 493 and 574).
- **IPC-A03** (encode/decode mismatch): confirmed. `encode` checks only length and serialises in full before checking it (lines 348–352).
- **IPC-A06** (queue age, not stall): confirmed. `write_since` is cleared only when the queue is empty (lines 589–590 and 618–619).
- **IPC-A07** (silent failures): confirmed. Accept drops are silent and processing errors are discarded.
- **IPC-A08** (bind cleanup): confirmed. A failure after `bind` leaks the socket, because `Drop` needs a constructed `Server`.
- **IPC-A10** (no total deadline in `call`): confirmed. Only per-operation timeouts, no overall deadline, no peer UID check.
- **IPC-A11** (path validation): confirmed. No NUL or emptiness check on `config.check` (lines 127–137).
- **IPC-A13** (unit actions accept unknown fields): confirmed. IPC uses raw `serde_json::from_value::<Action>`.
- **Config defect 1 fix:** confirmed. An exact per-action key allowlist exists (`schema.rs:362–391`), with a regression test at line 873. It covers config only, as the audit says.
- **Config defect 2 fix:** confirmed. A 32 MiB budget is shared across all patterns, capped at 1 MiB each, applied to both `size_limit` and `dfa_size_limit`.
- **Helper process isolation:** confirmed. Supervisor:
  - Environment, working directory and pipes: `env_clear` plus `LANG`, `cwd /`, and nonblocking stdin/stdout/stderr.
  - Deadline: measured from before spawn and checked every 2 ms loop.
  - Limits and pre-exec: `no_new_privs`, `close_range(CLOEXEC)` and CORE/FSIZE/CPU/NOFILE/AS rlimits, all async-signal-safe.
  - Reaping: kill-and-reap via a guard type, and a cached exit status means no PID-reuse risk.
  - CFG-10 is correct that parsing and regex compilation happen after the deadline.
- **Helper Lua conversion:** confirmed. Tables with metatables are rejected; traversal uses `lua_next`; cycle, depth, value and byte budgets apply; only the math/string/table libraries load, with `random` and `dump` removed; the instruction hook stays exhausted after `pcall`.
- **P01:** confirmed. The `fluxwm` binary depends only on `flux-core`, not on layout, IPC or config.
- **P02:** confirmed. `new_toplevel` maps into `Space` immediately, and the popup `grab` handler is a TODO.
- **P03:** confirmed locally (`XdgShellState::new` with no fullscreen, maximize or minimize overrides). The upstream Smithay part is unverified.
- **P04 / LP-08:** confirmed. Full-frame damage, unwraps on `bind`/`render`/`submit`, an unconditional `request_redraw`, and a hardcoded 60 Hz mode.
- **P05:** partly confirmed. I saw the output unwrap in `input.rs:35–37` but did not read the rest of the file.
- **P06 and P07:** confirmed. Every `unwrap()` and `unsafe` site the audit names exists in the current source.
- **LP-11 / LP-12:** confirmed. `set_mode` only flips the mode (`flux-layout:390–393`), and `docs/03-layout.md:51` says the conversion state machine isn't implemented. "Documentation decision, not implementation fix" is accurate.

## 5. Low-priority corrections
- **Stale line numbers in the layout audit.** Its references point at an older snapshot: `xdg_shell` 35–38 is now 37–40, the TODO at 121–123 is now 128–130, and `winit` 63/66/83/85/100 are now 71/74/91/93/109. Doc 24:67 discloses the reformatting, but the references should be refreshed.
- **Output transform exposed to clients.** `winit.rs:43` sets the output transform to `Flipped180`, which is visible to clients over the protocol. This is a backend rendering artifact. Check it under INP-09/GPU-10, in particular for clients that pre-transform their buffers.
- **Probe only half-asserts the depth mismatch.** It asserts `!decode_ok`, but I found no explicit assertion of `encode_ok`. It may be an `unwrap`; worth confirming.
- **Short check/clippy times mean cached builds.** `check` took 0.36 s and `clippy` 0.47 s, so they came from cache; this is not evidence of a clean build (B01).
- **Regex rules use unanchored `is_match`.** Confirm the documented semantics for `app_id`/`title` rules (substring vs. full match); I did not read `06-lua.md`.
- **`load_with_helper` is public and relies on SIGPIPE being ignored.** That holds in a normal Rust binary; document it for anyone embedding the library differently.
- **`set_constraints` can push a canvas rectangle past `MAX_COORD`.** Resizing (`flux-layout:380–386`) can make `x + width` exceed `MAX_COORD` without rechecking. Unverified against the full validity check; add a boundary test.
