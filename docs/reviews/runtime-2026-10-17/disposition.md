# Author disposition of independent runtime review

Date: 2026-10-17. Claude Opus 5.5 inspected frozen implementation commit `4a87922945decff827da5a18213d9f5d57fca6eb`. Read the [unaltered report](claude-report.md), [exact prompt](prompt.txt), [raw result](stdout.json), [execution record](execution.json) and [verified tool trace](verified-execution.json). This disposition records author checks and later changes; it is not independent verification of those changes.

## Review provenance and limits

The successful run exited 0 after 329.55 seconds, reports `is_error=false`, 30 turns and no permission denials. The local session trace records 29 tool calls, all Read/Glob/Grep, using `claude-opus-5-5`. No command/edit/agent tools were exposed. The author independently recomputed all 185 frozen input hashes after review: zero mismatches. The frozen copy remained unchanged. Claude did not execute tests, inspect pinned Smithay implementation source, verify the commit identity itself, or qualify hardware. The author supplies snapshot/hash verification separately.

An earlier minimal-mode invocation failed before inspection with “Not logged in”; its [raw result](attempt1-stdout.json) and [execution metadata](attempt1-execution.json) are retained as a failed attempt. The configured CLI invocation succeeded. The successful report's claims about upstream behavior were explicitly labeled inference; author source checks below distinguish confirmed paths from unexecuted scenarios.

## Four medium findings

| Review finding | Author verification and change | Closure status |
| --- | --- | --- |
| R1: synthetic motion on every commit/redraw can feed back into commits | Confirmed in pinned Smithay `PointerInternal::motion`: unchanged focus still forwards motion. A real mapped client reproduced33 stationary motions and32 feedback commits after one seed. Refresh now compares actual focus plus cached target surface/origin, retaining scene-induced local-coordinate changes while suppressing unchanged synthetic motion; real motion updates the cache | Same fixture binary before/after:33→0stationary motions,32→0feedback commits over 400 ms. Final integrated fixture also passes. This qualifies that bounded scenario, not arbitrary host input/scene/grab sequences |
| R2: ended popup wrappers block actions/focus until another input | Confirmed in pinned `PopupKeyboardGrab` and `PopupPointerGrab`: release happens on input/motion/button paths; `PopupManager::cleanup` alone does not unset seat wrappers. Runtime now retains a `PopupGrab` clone/root generational ID, observes ended chains before IPC, releases only matching typed popup wrappers (keyboard first), refreshes pointer pending focus and restores authoritative keyboard focus. Surviving nested grabs repair focus to the current parent | Compiled/source-checked; no live grabbing-popup destruction/input regression yet. Remains unqualified |
| R3: active-grab serial is weaker than a currently pressed button; several denied requests never dismiss | Accepted. Runtime now tracks current backend button press serials, requires the requested serial to remain pressed and pass same-client grab checks, and sends `popup_done` on every local denial before/after manager admission. Keyboard/touch initiation remains unsupported | Compiled/source-checked. Valid nested chains, stale serial replay, background clients and denial/dismiss ordering need a real input fixture |
| R4: invisible/unmapped root can retain popup focus | Accepted for tracked grabbing chains. Root mapped/visible membership is checked; invisible/dead roots dismiss the active chain, release wrappers and restore focus without waiting for physical input | Compiled/source-checked only. Non-grabbing popup trees need separate root-unmap/hide dismissal; complete popup lifecycle remains open |

The source/API check also verified that the pinned `popup_destroyed` hook runs outside backend/popup locks. Cleanup uses `Option::take`, never calls seat methods while a `with_grab` mutex closure runs, and checks concrete PopupKeyboardGrab/PopupPointerGrab types plus serial ownership before releasing wrappers. These are source constraints, not a runtime race proof.

## Lower findings and test-oracle corrections

- **Startup ECHILD:** accepted; the separate startup-child RAII wrapper now avoids signaling a potentially reused PID on `ECHILD`, matching the managed-worker policy. No external global reaper is installed. Startup command remains one additional direct child beyond the 64 worker live/pending bound and is not in worker outcome history.
- **Slow reader wakeups:** accepted limitation. A permanently blocked client can keep the selective 16 ms retry active indefinitely. The retry is absent after drain; it is not a slow-client timeout, queue-memory policy or power/fairness qualification. A per-client stall/resource policy remains required.
- **Sequence churn:** accepted conservative semantics. Buffer commits can advance the sequence without metadata changes. No guarantee of exactly one increment per visible change is made. Coalescing and subscription policy remain future work.
- **DnD authorization:** author inspected the pinned `data_device/device.rs::StartDrag`: it checks an active grab serial before the generic callback, but that callback does not give this implementation a proven source-client/current-press admission boundary. The previous adapter could replace a popup grab. Runtime now cancels client DnD requests and advertises `drag_and_drop=false` until complete authorization/lifecycle policy is integrated. No claim of secure functioning DnD is made.
- **Absolute input:** accepted; non-finite coordinates are rejected and coordinates are clamped to positive output geometry. Native-device/NaN injection tests remain open.
- **Empty idle test:** accepted. The original idle comparison covers only an empty scene. The new mapped stationary-pointer fixture closes R1's concrete feedback case; it does not establish mapped idle power, arbitrary host occlusion, every slow-reader policy or full redraw invariants.
- **Child cleanup oracle:** accepted distinction. The live IPC test observes the direct child absent after compositor exit; that alone does not prove who reaped it. Unit process ownership tests and source wait/try_wait paths are separate evidence. No reaping inference is made from PID absence alone.
- **Sequence advancement oracle:** corrected. The IPC smoke now queries before the action and asserts an actual increase, then checks the later transport error carries the post-action sequence.
- **Unmap/remap oracle:** reviewer independently accepted the fixture's wait-for-new-configure check. Runtime evidence remains author-executed, not reviewer-executed.

## Post-review verification

The fixes are committed in `1b956fe`; the new fixture is committed in `df4b4b5`. [Post-review evidence](../../evidence/nested-milestone/2026-10-17-post-review/README.md) identifies the later binary, commands, tests, live runs and before/after pointer artifacts. These changes were checked by the author and a pinned-API source peer review; **Claude did not re-review them**. The reviewed 185-file manifest must not be presented as their manifest.

No whole implementation phase is closed. Read [runtime findings and remaining production gates](../../development/runtime-findings.md) for native/transaction/clipping/input/config/desktop/resource/operations blockers. Independent review improved the work; it did not certify production readiness or establish that no other findings exist.
