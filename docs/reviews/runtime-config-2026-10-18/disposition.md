# Author disposition: nested runtime config integration

Claude inspected frozen `96138f89c86cdedd552a8abd2cd48a6c426fd7f0` with Read/Grep only. The [report](claude-report.md), [completion](stdout.json), [prompt](prompt.txt), [command](command.json), [execution](execution.json), [input manifest](input-manifest.json) and [author verification](verified-execution.json) retain the actual record. Exit was zero after 199.99 seconds. All **61 frozen input hashes match**, with no permission denials, outside-snapshot paths or error result. The actual trace has **20 calls**; the report self-describes 21. Claude executed no tests or commands and did not inspect author logs. Its code reading is not runtime evidence or production certification.

The reviewer found **no reachable high or medium code defect within the read paths**, but identified two medium test gaps. Both are accepted as gaps in the original source's verification.

## M1: live-model rollback and exact gap boundaries

The original gap2048 case failed at the worker's model-range preflight, so it did not reach live geometry rollback in `drive_config`. The report's assertion that every smoke case had no mapped windows is incorrect: the input-publication case maps two windows. The actual gap remains valid because it did not attempt a geometry-invalid publication with those live members.

Later author changes add a dedicated real protocol fixture with minimum width 999,999,984 and a successfully mapped SHM buffer/frame under gap13. Gap32 is schema/model-range valid but exceeds that live strip's aggregate extent. Its candidate also requests canvas and an extra binding. The runtime must return the exact live-geometry failure and retain the applied generation, effective mode/binding count and window ID/geometry/focus/workspace, then recover with gap8. Separately gap1024 succeeds and gap1025 returns the exact nested-model limit diagnostic while retaining gap1024. The first fixture attempt used an incorrect CLI timeout range and failed before creating a window; that was corrected before qualification, not counted as rollback evidence.

## M2: superseded prepared candidates and separate stale gates

The original runtime suite waited for each generation to terminate before accepting the next. Later author changes hold Alt, observe generation N prepared, accept N+1 while still held, observe it prepared, then release and require only N+1 to apply. Actual configured Alt+a routing and consumed/plain press/release ownership remain checked. This is a real prepared-supersession path, not an exhaustive thread-scheduling proof.

Separate deterministic tests park an owned test worker to exercise the exact interval after a new request is accepted but before that worker observes it. The dispatch-facing completion accessor must hide the old prepared value without dropping its large payload on dispatch. Another test checks stale and closed worker publication separately. Four independent mutations remove worker-latest, worker-closed, dispatch-latest and termination-seal guards. Each fails its intended assertion; source is restored byte-for-byte and the repaired tests pass. These are author experiments, not tests run by Claude. See [retained evidence](../../evidence/nested-milestone/2026-10-18-runtime-config/README.md).

## Lower-severity notes and scope corrections

The source open now adds `O_NOCTTY`, preventing terminal acquisition before a nonregular descriptor is rejected. The retirement comment now distinguishes replaced live-model storage dropped on the worker from temporary layout clones still created/dropped on dispatch. Work still scales with admitted windows; no universal dispatch latency is inferred.

The first review did not inspect the real file-source tests. Component tests already cover symlinks, FIFO, directory, oversized/non-UTF-8 source, non-UTF-8 filenames and path admission, with an independently failing nonblocking-open mutation. Those are component checks, not whole-compositor coverage of every file fault. Holding input can defer publication indefinitely; host-focus cancellation qualifies a particular recovery path, not a universal promise that any stuck key eventually clears.

Superseding a reload does not cancel active evaluation; inner-worker failure observation, all held pointer/grab combinations, automatic filesystem debounce, full schema/rules/autostart, keymap/device/output transactions, native sessions, global resources and soak remain open. A [focused follow-up](../runtime-config-followup-2026-10-18/disposition.md) reviews the later corrections separately.
