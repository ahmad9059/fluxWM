# Nested host-focus cancellation

The winit backend emits keyboard host-focus events. FluxWM now uses focus loss to cancel the nested seat's held input state. The pinned Smithay winit adapter filters synthetic keyboard transitions and repeats, so an outside physical release cannot be assumed to repair virtual keys automatically.

On an observed focus loss, the runtime releases every key still held in its XKB state through the normal keyboard handler. Keys whose press was consumed by a compositor shortcut have their release intercepted too; forwarded keys get a release and coherent modifier update. The suppression set is then cleared. This releases depressed modifiers; it does not force-toggle locked modifiers or replace the keymap. Keyboard events are ignored after a known loss until focus returns. Initial host-focus state is explicitly unknown (`null`); before the first notification, attached-backend input remains authoritative.

Held pointer buttons receive releases through the current grab, initiating-press authorization is retired, tracked popup chains are ungrabbed and any remaining nested pointer grab is removed. Picking is refreshed against the last successful composed scene. Later physical button releases with no matching held press are ignored, as are duplicate presses; this prevents duplicate/orphan events after cancellation. Pointer hover and wheel routing remain independent of keyboard host focus. A fresh pointer click is accepted even when keyboard focus notification has not yet arrived.

Logical selected window and virtual keyboard focus persist across host activation changes. This feature is input-state cancellation, not complete XDG activation, focus/lock/security policy, keymap reconfiguration or native multi-device recovery. Native device removal, touch/gestures, pointer constraints, IME grabs and all popup/move/resize cancellation cases remain separate qualification.

## Diagnostics

`diagnostics.input_state` reports the last observed `host_focus`, held/suppressed key counts, `authorized_buttons` count and current keyboard/pointer grab flags. It does not expose key identities. The button count is FluxWM’s current press-authorization ledger, not a direct hardware/Smithay pressed-button query. Client event ordering and complete journals independently check supported routing. Final runtime counters add `host_focus_losses`, `focus_cancelled_keys` and `focus_cancelled_buttons`. Repeated identical focus notifications do not repeat cancellation. Actual transitions advance the IPC state sequence. Ordinary held-state cancellation does not require pixel changes; ending a tracked popup chain schedules composition so its removal does not wait for client destruction. Valid popup-grab dismissal pixels remain separately unqualified.

## Reproduce

```sh
cargo build -p fluxwm -p fluxwm-fixture
python3 tools/smoke_focus.py --bundle /path/to/verified-xvfb-bundle
```

Omit `--bundle` to use the existing verified official package preparation. The [private-input infrastructure](private-input.md) authenticates an owned Xvfb, disables TCP, verifies display/session/target PID and window title, and never opens the host display for injection. The helper now supports a fixed whitelist of test key names and explicit focus to either the owned compositor window or its private X root. Root-targeted physical releases use `activate:false`, so they do not silently reactivate FluxWM. Every other action still verifies the exact owned target. The focus action confirms the resulting X input-focus target with `XGetInputFocus`.

`input-probe` now requests both pointer and keyboard and records bounded key/modifier/enter evidence in its explicitly named mode0600 JSONL journal. This fixture receives only the synthetic private session's keys. The compositor itself records counts, never key identities. Test binaries are copied and hashed before launch; harness/helper hashes must remain unchanged. Each of three cases has a 20-second deadline, the fixture a ten-second deadline/seven-second observation, and the compositor an 18-second exit timer. Package preparation and bounded process cleanup are separate from those deadlines. Parent environment stays unchanged; XKB defaults are set to US only in test subprocesses.

| Case | Required observations |
| --- | --- |
| Keyboard | Hold Alt+C, consume C and enter canvas; focus private root and receive Alt release/cleared depressed modifiers; C never leaks; physically release outside, return and deliver plain S without changing canvas; fresh Alt+S works; a forwarded A held through another focus cycle gets one cancellation release, its late real release does not duplicate, and a fresh A works; consumed C held through return never leaks a release |
| Pointer | Hold A's left button, focus private root and observe cancellation release; regain focus and enter B without old click capture; late physical release is discarded; a fresh B click works with enter preceding press |
| Popup | A real initiating press authorizes a popup; held key release goes to it on loss, `popup_done` arrives, root re-enters with empty keys, both grab flags clear, a new composition removes the retained popup without client requests, and a stale-serial grab is denied |

Both cases additionally require zero held/suppressed state and no pointer grab after loss, successful normal fixture completion and owned client/server cleanup. The test checks real client events, not successful XTest calls alone. Before/after results and exact hashes are preserved in [the focus evidence bundle](../evidence/nested-milestone/2026-10-17-host-focus/README.md).

The preserved pre-change executable genuinely fails: no Alt cancellation release, plain S becomes a stale-Alt workspace shortcut, and a pointer click grab survives loss. The first corrected run passes both cases with cancelled-key/button counts 2/0 and 0/1 respectively. A strengthened run adds held-through-return key and modifier oracles; the original runs remain separate observations. These are nested X11/software-rendering results. Host Wayland, native devices, all layouts/locks/grabs, multiple buttons/devices and full application compatibility remain open. A held modifier is cancelled on loss and is not reasserted from a synthetic focus-return event; release and press it again to start a fresh chord.

Pinned-backend note: Smithay’s private winit `key_counter` updates only on non-synthetic key events and is not reset by its focus callback. It can therefore diverge after outside releases. FluxWM does not use `KeyboardKeyEvent::count`; diagnostics use the actual Smithay keyboard pressed set. Upstream counter reconciliation/overflow remains a recorded backend qualification concern rather than an assertion that this application reset repairs private backend counters.

`--case keyboard|pointer|popup` runs one scenario. The popup probe is an explicit test-only `FLUXWM_FIXTURE_GRAB_POPUP=1` setting on `input-probe`: it opens a popup from its own genuine pointer press, retains it after `popup_done` until the composition check, and requests a stale-serial popup only after an explicit S event. The pinned helper applies requested popup grabs on surface commit, so the stale request includes a bufferless commit. The harness first supplies a real pointer motion into the mapped popup; stationary picking is intentionally held while a grab owns routing.

`diagnostics.composed_scene` reports counts from the last successfully published virtual render snapshot, including `popup_surfaces`. The popup test observes 4→5 compositions, 1→0 captured popup surfaces and unchanged Wayland-request count while the dismissed role is still retained. This proves recomposition/publication, not a separate raw-pixel disappearance oracle.

Private test harnesses now create separate short mode0700 runtime directories under `/tmp`; artifact directories may be long. Endpoints must already be gone before runtime-directory cleanup. A quota-interrupted executable copy and an overlong-runtime-path startup failure are preserved as infrastructure failures, separate from input regressions. Verified duplicate executables from completed old runs were retired to recover space; public evidence and pre-fix binaries were retained.
