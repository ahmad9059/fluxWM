# Native live submission validation, 2026-10-19

All eleven [workspace checks](checks.json) passed. [All-feature output](native-tests.log) and [inventory](native-test-list.log) record 249 registered tests. Child-process summaries are not additional registered tests. New fixtures cover exact-cookie synchronous completion, rollback-error retention and successful retirement, CLOSEFB preflight rejection and clone ordering, raw GETFB2 handle deduplication, rollback property restrictions, live commit flags and explicit preview argument rejection.

The [read-only owned-child audit](probe-audit.json) executes `--check` only and records 439 checked opens, zero denied calls, successful exit and reaped child. It does not execute `--session` or the live preview. [Final executable hashes](executables.json) retain the previously exercised nested compositor/helper/fixture bytes from the [native foundation run](../2026-10-18-native-foundation/runtime/execution.json); those GUI scenarios were not rerun. [Source hashes](source-hashes.json) identify implementation input.

The [live adapter contract](../../../development/native-live-submit.md) records native behavior and limits. Physical GBM/GLES rendering, DRM commits/events, rollback/CLOSEFB, driver support, seat recovery and hardware timing remain unexecuted. Main FluxWM remains nested-only. These checks do not qualify a production desktop.

Raw test output preserves child-process interleaving and trailing whitespace. The final staged whitespace check excludes only this directory's `native-tests.log`; source and authored documents pass.

[Independent Claude review and focused follow-up](../../../reviews/native-live-submit-followup-2026-10-19/disposition.md) record two medium defects, their source corrections and the remaining process-exit recovery risk.
