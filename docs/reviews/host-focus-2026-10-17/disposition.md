# Author disposition of the host-focus review

Claude inspected frozen `9c4c733140519176e933c7bc0261b7280c0c2d9c` in 170.76 seconds, exit zero, `is_error=false`, no permission denials. Actual trace records 17 Read/Grep calls. Author recomputation of all 1,188 frozen file hashes found no mismatches. Claude executed no tests, builds or hashes and did not inspect pinned Smithay implementation. Preserve the [unaltered report](claude-report.md), [raw result](stdout.json), [prompt](prompt.txt), [execution](execution.json) and [verified trace](verified-execution.json).

## M1: inferred DnD drop on loss

Not reachable in the current runtime. The reviewer inferred an active DnD grab from handler-trait presence. `handlers/mod.rs::dnd_requested` calls `source.cancel()` and never installs a DnD grab; capabilities advertise `drag_and_drop:false`. The pinned `wayland/selection/data_device/device.rs` start-drag path delegates to that callback and returns, without installing a grab itself. Author source checks identify exact file hashes in the evidence bundle. No supported drop target exists in this implementation.

The underlying warning is valid as a future integration requirement: normal final-button release can perform a drop when a DnD grab is enabled. Re-enabling DnD must implement grab-specific cancellation and the proposed source/target cancel/no-drop scenario first. Ordinary click cancellation currently uses normal releases, which can complete a toolkit click; this is stream cleanup, not rollback of application actions. Full toolkit/input policy remains open.

## M2: button bookkeeping was not a seat-state measurement

Accepted as an oracle/documentation gap. The new field is explicitly `authorized_buttons`: it reports the compositor's current press-authorization ledger. Keyboard pressed counts come from Smithay's actual keyboard pressed set. There is no claim to expose a direct hardware/PointerInnerHandle pressed-button query.

The fixture now also requires the real keyboard grab flag to be false, B enter before its fresh press, complete key/button counts and correct recipients over the entire seven-second journal. A valid popup-grab case adds actual keyboard/pointer wrapper flags and empty held keys on root re-entry. The author checked pinned `PointerHandle::button`: it updates its internal pressed-button list before invoking the grab, so a grab swallowing the forwarded event cannot prevent that update. This rebuts that specific example while retaining the absence of a direct diagnostic seat-button count. Arbitrary future grabs/devices remain unqualified.

## M3: popup-chain cancellation was source-only

Accepted. The new real case opens a popup from a genuine recorded pointer press, maps it, moves into it and verifies both grab wrappers. A held A release is observed on popup 0 after loss, followed by `popup_done` and root keyboard re-entry with an empty key array. Both grab flags clear. The old press serial is refused on a new committed popup request; its missing-active-press counter increases by one. A fresh press after return authorizes another popup and keyboard/pointer focus, providing a positive control.

The author also found that ended popup cleanup needed a redraw request independent of client resource destruction. `2a619fa` schedules it centrally. With the dismissed role retained and no new Wayland requests, the live case records compositions 4→5 and published popup surfaces 1→0. A one-line mutation removing that request fails `dismissed_popup_not_recomposed`, retaining composition 4 and one published popup. This is virtual composition/publication evidence, not an independent raw-pixel disappearance oracle.

## Lower observations and remaining limits

- Consumed C is now held through return and its late release never reaches the client. Forwarded A and button releases are counted over the full journal, closing delayed-duplicate holes beyond the short negative observation.
- Held modifiers are cancelled and not reasserted from synthetic focus-return events; a fresh modifier press is needed for a new chord. This is explicit development policy.
- Fresh pointer clicks are not gated on keyboard host focus. A separate live first-click-before-focus-notification scenario and duplicate-notification test remain open; idempotence is source-checked only.
- Virtual selected window/keyboard focus persist. Full XDG activation, IME, lock/keymap, native/multiple-device and all move/resize/grab policies remain open.
- Pinned winit's private `key_counter` has no focus reset and uses a raw increment; outside releases can diverge from it. FluxWM does not consume that count, but virtual-seat repair does not repair the backend counter. Reconciliation/overflow remains source-identified and unqualified.

Read the [follow-up review disposition](../host-focus-followup-2026-10-17/disposition.md) for later oracle challenges, source rebuttal and exact limits. Later author corrections are not part of this initial frozen review. The [evidence bundle](../../evidence/nested-milestone/2026-10-17-host-focus/README.md) preserves failed and passing comparisons, mutation, infrastructure failures and exact hashes.
