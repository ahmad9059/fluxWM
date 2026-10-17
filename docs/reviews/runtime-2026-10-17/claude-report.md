# FluxWM runtime review: snapshot `4a87922945decff827da5a18213d9f5d57fca6eb`

## Scope and limits

**What I read:**
- `docs/development/runtime.md` and `processes.md`, in full.
- `crates/fluxwm/src/{main,state,winit,input,actions,ipc}.rs`, `handlers/{mod,compositor,xdg_shell}.rs`, in full.
- `crates/flux-process/src/lib.rs`, lines 60–300 (the non-test code).
- `crates/flux-ipc/src/driver.rs`, lines 80–216, plus a grep of `flux-ipc/src/lib.rs` for how the state sequence is handled.
- Parts of `flux-layout/src/lib.rs`: `insert_with_focus`, `remove`, `focus_window`.
- Spot checks of `tools/smoke_nested.py`, `tools/smoke_ipc_runtime.py` and the lifecycle script in `fluxwm-fixture/src/main.rs`.

**Limits:**
- I ran no commands. I did not build, run tests, run the smoke harnesses, or recompute any hashes.
- The directory is not a git repo, so I couldn't confirm that these files match snapshot `4a8792…`. I'm relying on the snapshot being frozen as stated.
- Smithay is pinned to git rev `023b78b` (`Cargo.lock:1447`) and isn't vendored here. Any claim that depends on Smithay's internal behaviour (how pointer motion is forwarded, when popup grabs are released, serial checks on drag-and-drop) is my **inference** from memory of upstream code, not something I checked in source.
- The grabs (`grabs/*.rs`) and most of the fixture were not inspected.
- I did not use earlier audits (`docs/24-*`, `docs/reviews`) as evidence.

## Verdict

**Documentation accuracy:** mostly accurate and properly hedged. I checked the process/signal ownership, worker shutdown order, spawn admission bounds, IPC sequence handling, null-buffer unmap/remap configure gating and dead-role pruning against source, and they match. There are two places where the docs say more than the code delivers:
1. Popup grab authorization (finding 3).
2. "Scene changes refresh a stationary pointer" (finding 1). The code actually refreshes on every redraw request, not just scene changes.

**Runtime maturity:** an early nested prototype. The `fluxwm` crate has **zero** `#[test]` functions. Neither the fixture nor any test opens a popup, so popup grab authorization is backed only by reading the source. This is not production-ready, and I'm not claiming the review is complete.

## New or materially incorrect findings (medium)

### 1. Every client commit sends a fake pointer motion; possible feedback loop that keeps rendering

**Source-confirmed path:**
- `handlers/compositor.rs:50` (`commit`) and `state.rs:400` (`commit_window`) call `request_redraw()` on every commit.
- `state.rs:287` (`request_redraw`) always sets `pointer_focus_dirty = true`.
- `winit.rs:129` does the same on every host `Redraw` event.
- `input.rs:17-39` (`refresh_pointer_focus`) then sends `pointer.motion` with a fresh serial, followed by `pointer.frame`, whenever no pointer grab is active.

**Inference:** in Smithay 0.7, when the focus surface hasn't changed, `motion` sends `wl_pointer.motion` unconditionally. If so, any client that commits (a video, a blinking cursor) gets motion+frame events under a pointer that hasn't moved. A client that redraws or commits in response to motion would create a self-sustaining loop at about 60 Hz. That contradicts "static scenes do not create a perpetual redraw loop" (`runtime.md:58`), and the docs' "Scene changes refresh…" (`runtime.md:38`) doesn't describe what the code does.

**Not executed.**

**Closure test:**
- Fixture client maps under a stationary pointer.
- It commits one buffer without any damage.
- On each `wl_pointer.motion` it commits again.
- Assert the motion count stays at most 1 and `redraws` stops growing after N ms.
- Also: only set `pointer_focus_dirty` from `relayout`, map/unmap, or destroy, not from every commit.

### 2. A popup grab that has ended but not been released still blocks focus, shortcuts and IPC actions

**Source-confirmed path:**
- These all gate on `keyboard.is_grabbed()`, not on whether the popup grab is still live:
  - `actions.rs:21` (`apply_action`)
  - `state.rs:446` (`focus_window_id`)
  - `state.rs:370` (whether a newly mapped window is activated)
  - `state.rs:460` (`restore_keyboard_focus`)
- `input.rs:60-76` intercepts shortcut keys in the input filter, so the popup keyboard grab never receives them.
- Nothing calls `restore_keyboard_focus` when a grab ends.
- `popup_destroyed` (`xdg_shell.rs:209`) only cleans up and redraws.

**Inference:** Smithay's `PopupKeyboardGrab` and `PopupPointerGrab` only release themselves on the next input event they receive. If that's right, then after a client destroys a grabbing popup (an ordinary menu selection):
- Alt+arrow and IPC focus/move/pan/resize/mode actions fail with "a popup owns keyboard focus" until some other key or pointer event clears the grab.
- The first click on another window doesn't move focus (`input.rs:160`).
- Keyboard focus can end up out of step with the layout's focus, which is what IPC reports as `focused`.

**Not executed.**

**Closure test:**
- Open a grabbing popup, then have the client destroy it (or its parent toplevel).
- With no pointer motion, send an IPC `focus` action and an Alt+Right shortcut.
- Assert both are accepted and that keyboard focus matches `layout.focused()`.
- Fix direction: check whether the popup grab is still live (e.g. the `PopupGrab`/`has_ended` state), and restore focus once it has ended.

### 3. The popup-grab authorization docs say more than the code enforces

**Source-confirmed:**
- `check_grab` (`xdg_shell.rs:215-236`) accepts any active pointer grab whose start serial matches and whose start focus belongs to the same client. That includes a `PopupPointerGrab` that is still in place after the button was released.
- So a grab request does not require an "active same-client pointer press", which is what `runtime.md:56` says.

**Also source-confirmed:** `runtime.md:56` says invalid or absent authorization dismisses the request, but several denial paths don't dismiss. After authorization passes, these return without calling `send_popup_done`, leaving a live popup with no grab:
- `find_popup_root_surface` fails (`xdg_shell.rs:162`)
- the root window isn't tracked (`xdg_shell.rs:165`)
- the root window isn't mapped (`xdg_shell.rs:168`)
- `grab_popup` returns an error (`xdg_shell.rs:171`)

xdg-shell says a denied grab must dismiss the popup.

**Closure tests:**
- After the button is released, request a second grab with the original press serial, and assert the documented policy is enforced.
- Request a grab on a popup whose parent is unmapped, and assert `popup_done` is sent.

### 4. Unmapping or hiding a toplevel doesn't dismiss its popup grab (unexecuted scenario)

**Source-confirmed:** the unmap path (`state.rs:383-391`) and the hide path in `relayout` (`state.rs:439`) call `space.unmap_elem` but never ungrab or dismiss the window's popups. `restore_keyboard_focus` then returns early because a grab is active.

**Inference:** keyboard focus can stay on a popup belonging to an invisible window until the user clicks somewhere.

**Closure test:**
- Open a grabbing popup, then attach a null buffer to its parent (or pan the parent off-screen).
- Assert the popup gets `popup_done` and keyboard focus moves to the window that has layout focus.

## Lower-severity notes (source-confirmed)

- **Startup child signal on `ECHILD`:** `SupervisedChild::drop` (`main.rs:103-110`) still calls `kill()` when `try_wait` returns `Err` (e.g. `ECHILD`). That's the PID-reuse risk `ManagedChild::drop` deliberately avoids (`flux-process/src/lib.rs:90`).
- **Startup child not counted:** it isn't counted in the 64-application limit or the launch outcome history.
- **Flush retry can run forever:** `flush_wayland` (`state.rs:116-119`, `581-584`) re-arms the 16 ms retry for as long as any client stays blocked. A stopped client means a permanent ~60 Hz wakeup. The docs' "retries stop after output drains" is literally true but hides this.
- **Sequence bumps on every commit:** `commit_window` (`state.rs:392-394`) bumps the state sequence on every buffer commit, so IPC sees churn of about 60/s from any animating client.
- **Drag-and-drop not checked locally:** `dnd_requested` (`handlers/mod.rs:65-87`) does no serial or same-client check of its own and replaces whatever pointer grab is active, including a popup grab. That's only safe if Smithay checks in `start_drag`, which I couldn't verify (inference).
- **Absolute pointer motion:** `PointerMotionAbsolute` (`input.rs:123-149`) has no finite-value check and no clamp, unlike relative motion.

## Overstated regression checks in the harnesses

- **Idle check only covers an empty scene:** `smoke_nested.py:237-244` compares idle counters only for runs with no client. It doesn't cover a mapped static client, a pointer resting over a client, `flush_retry_timer_fired`, or host occlusion. So it can't catch finding 1 and doesn't fully back the "static scenes" claim.
- **Child cleanup check proves death, not reaping:** `smoke_ipc_runtime.py:134-139` uses `kill(pid, 0)` after the compositor exits. That shows the child is gone, not that the compositor reaped it (init could have reaped it after reparenting).
- **Sequence check doesn't check advancement:** `smoke_ipc_runtime.py:104` checks that the error reply has the same sequence as the action reply, but not that the action actually advanced the sequence.
- **Null-buffer remap check holds up:** the fixture's remap check (`fixture main.rs:549-572`) would indirectly catch a premature configure during unmap (the remap wait would time out). That check is reasonable.

## Controls checked with no defect found

- **Signals:** set up before the event loop, display, winit and worker threads are created. The dispatcher is declared first so it is dropped last. SIGCHLD only wakes the loop, and there is no `waitpid(-1)`. The mask is unblocked before exec in both spawn paths.
- **Worker shutdown order:** `shutdown`, then dropping the event loop (which drops any delivered outcomes), then `spawned.clear()`, all outside dispatch. Stop and delivery are serialized under the control mutex, and a backpressured worker can be woken.
- **Spawn admission:** `spawned + pending >= 64` is checked before `try_spawn`. The pending count is only incremented after admission succeeds. Each completion is `try_wait`ed when it arrives. The outcome history is capped at 64 and strings are cut on character boundaries.
- **IPC:** handlers return the current `self.sequence`, and the server rejects a sequence that goes backwards. Operation IDs equal the post-action sequence. Dead roles are pruned before `drive_ipc`. IPC is driven only by wake events, with no polling.
- **Null-buffer unmap:** keeps the window's role and ID. The initial configure is held back by the `just_unmapped` guard.
- **Toplevel cap:** limited to 1024 by disconnecting the client.
- **Redraw scheduling:** the redraw timer is one-shot and only armed while there's pending work. Host submission is gated on `host_ready`, and the pixel budget is enforced.
- **Socket and output files:** checked for collisions before Smithay unlinks anything. Output files are published with `persist_noclobber`.
