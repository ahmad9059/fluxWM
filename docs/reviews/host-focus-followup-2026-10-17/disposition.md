# Author disposition of the host-focus follow-up

Claude inspected frozen `d91db8e967ca9ef31798cec9644402381dcc50a2` in 119.34 seconds, exit zero, `is_error=false`, no permission denials. Actual trace records 14 Read/Grep calls. Author recomputation of all 1,188 frozen hashes found no mismatches, and all recorded absolute read/search paths stay inside the snapshot. Claude executed no tests/hashes. Preserve the [unaltered report](claude-report.md), [raw result](stdout.json), [prompt](prompt.txt), [execution](execution.json) and [verified trace](verified-execution.json).

## M1: alleged unconditional event redraw masks the fix

Not established. The cited `state.request_redraw()` is outside the event-source closure, at the end of `init_winit`; it executes once during initialization. The focus event arm calls only `host_focus_changed`. It is not an unconditional redraw after each event. The reviewer explicitly had not checked the caller and marked this causal assumption as a caveat.

The requested one-off mutation is stronger evidence: starting from this same frozen source, the author changed only the ended-popup cleanup redraw into pointer-focus dirtiness. The live popup case then fails solely with `dismissed_popup_not_recomposed`: composition stays 4 and the published scene retains one popup surface. The valid binary records 4→5 and 1→0, with no new Wayland requests while the dismissed resource remains alive. Metadata/patch and raw results are in [the focus bundle](../../evidence/nested-milestone/2026-10-17-host-focus/README.md). This does not assert a separate raw-pixel oracle or physical scanout result.

## M2: popup journal ownership was incomplete

Accepted. Complete seven-second ownership maps now require A pressed/released exactly once on popup 0 and nowhere else, S pressed/released exactly once on root 0, and left-button presses twice on root 0 with cancellation releases once each on popup 0 and the fresh popup 2. No other recipients/events for those keys/buttons are permitted. Thus late real releases cannot hide on another surface or outside the short observation window. The second press/release pair belongs to the new positive-control grab, not a duplicate of the first.

## M3: any denial could satisfy the stale-serial assertion

Accepted. Runtime diagnostics now count only denials lacking a currently held matching press (`popup_grabs_denied_missing_press`); the stale request must increase that counter by exactly one. A fresh genuine press after return must additionally map popup 2, get keyboard focus and both grab wrappers, accept pointer entry, and cancel cleanly on another loss. This verifies that the manager/root did not simply reject every post-return request. The first positive-control attempt correctly failed because the fixture only mapped popup index 0; mapping the new valid popup fixed that fixture error. The failed trace is retained.

## Other findings and infrastructure

The short-runtime helper separates socket paths from artifact paths, checks that endpoints are absent before cleanup and removes its owned directory. Cleanup exceptions can still prevent a result file from being written; no universal filesystem-failure reporting guarantee is claimed. A quota-interrupted copy and a SUN_LEN startup failure are preserved as infrastructure failures. Verified duplicate executables from completed runs were retired without changing public evidence, source or the preserved pre-fix binaries.

Later ownership/positive-control/counter corrections are author work after this frozen review and were not independently re-reviewed. DnD remains unsupported and cancelled on request, rather than an active drop-grab path. Native/host Wayland, every popup chain, keyboard-layout/lock/IME/multiple-device policy, backend counter reconciliation and full production phase gates remain open.
