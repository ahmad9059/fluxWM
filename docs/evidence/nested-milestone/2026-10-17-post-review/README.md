# Evidence after independent runtime review

Implementation snapshot: `1b956fe`; new pointer fixture: `df4b4b5`. This bundle records author execution after [Claude's frozen review and disposition](../../../reviews/runtime-2026-10-17/disposition.md). Claude reviewed earlier `4a87922`, not these later changes.

`execution.json` identifies source, commands and final binary hash. `sha256.json` protects this compact bundle and excludes itself. `pinned-source-checks.json` records exact local upstream file hashes used for author verification; the reviewer did not inspect those files.

- **95 distinct test cases pass**: config 8, core 9, IPC 31, layout 18, process 10, CLI 3, config-helper 3, containment 7, fixture 6. Three additional filtered process-fixture invocations produce a naive passing-result sum 98; they are not extra unique cases.
- Build, Clippy with warnings denied, Rustdoc with warnings denied, formatting and documentation checks pass locally with populated caches. No hosted CI or clean-machine qualification is claimed.
- `nested/` repeats real map/unmap/remap/disconnect, copied offscreen quadrant pixels/orientation, empty idle and SIGTERM cleanup on the final binary.
- `ipc/` repeats pipeline/half-close, actual action sequence increase and later error sequence, operation-correlated missing-exec/recovery outcomes, child environment/masks, child absence after quit, socket collisions and socket cleanup. PID absence alone does not establish who reaped the process.
- `final-fixtures/slow-reader/` repeats real blocked-server callback recovery with no additional Wayland requests during drain.
- `pointer-before/` and `pointer-after-same-fixture/` preserve the actual failed/passing comparison using the identical fixture SHA. The reviewed compositor generated 33 stationary motions and 32 bounded feedback commits; the first corrected compositor generated 0/0. Full source/review boundaries are in the [pointer fixture documentation](../../../development/pointer-feedback-fixture.md).
- `final-fixtures/pointer-feedback/` repeats that mapped stationary-client test against the final binary: zero extra stationary motions, zero feedback commits, no changed-coordinate motion, 400 ms observation after 200 ms settling and one seed commit.

Original PPM files remain at the per-run source artifact paths with hashes and validation metadata in the nested summary. No giant image is duplicated in this bundle. Commands and local original artifact paths are retained in execution/results.

Popup-grab fixes remain source-checked and compiled without live input/grab conformance coverage. Non-grabbing popup-tree root-unmap dismissal, native/hardware, physical presentation, clipping/transactions, config integration, desktop support, resource/fairness, shutdown and complete phase gates remain open. This bundle is specific runtime evidence, not a production-readiness certificate.
