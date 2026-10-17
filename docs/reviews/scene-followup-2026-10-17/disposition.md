# Author disposition of the scene/input follow-up

Claude inspected frozen `adfa7a7ad503ce8c795b3d3000ed1094edb7f2d5` in 127.48 seconds using 14 Read/Grep calls over 15 turns. Exit was zero, `is_error=false`, and permission denials were empty. Author recomputation of all 331 frozen file hashes found no mismatches. The [unaltered report](claude-report.md), [raw response](stdout.json), [prompt](prompt.txt), [execution](execution.json) and [verified tool trace](verified-execution.json) preserve actual scope. Claude executed no commands, tests or hashes and did not inspect the popup exact-row oracle within its budget.

## Medium M-1: release-specific oracle was insufficient

Accepted. The first `held-motion` scenario observed a count but never required composition stability. Every successful composition marks focus dirty; retained dirtiness could hide removal of the separate release-time assignment. Earlier before/after failures remain genuine observations of their binaries, but they did not isolate the release-specific change.

Correction `62ca6ed` records counts before press, after held motion and after release observation and requires equality. Changed counts fail as `oracle_composition_changed`, rather than being called a successful input regression. Pressing an already-focused window no longer queues redundant relayout; keyboard ownership is still reasserted. The harness also requires ordered journal indices for A release, A leave and B enter. The [private input documentation](../../development/private-input.md) specifies the strengthened oracle.

A mutation removed only the release-time dirty assignment while retaining grab-preservation and the unchanged-focus correction. The strengthened test fails `M1_missing_focus_after_release` with composition counts **3 / 3 / 3**. There is no composition to repair focus in this scenario. The repaired binary passes the strengthened test. The [evidence bundle](../../evidence/nested-milestone/2026-10-17-shared-scene/README.md) preserves mutation source/binary identity, public event journal and fixed runs. This is author-run mutation evidence, not Claude-executed verification.

## Lower findings

- **Event ordering:** accepted; journal-index assertions now require release < leave < enter, rather than mere presence after injection.
- **Constructor cookie leak:** accepted; exceptions after cookie creation close the log and delete the authority file. Author fault injection for log-open failure and `Popen` failure confirms removal without starting a server.
- **Runtime deadline:** input helper timeout now clips to the remaining runtime budget; bounded teardown remains outside it. This does not assert a universal kernel-operation deadline.
- **Stale comment:** corrected. Button routing uses the last composed scene; queued relayout becomes eligible after composition.

Claude source-checked dirty preservation, release scheduling, zero-size clearing, private display ownership/authentication and post-release delivery assertions. The reviewer found no new high-severity finding within this narrow scope. Its report explicitly leaves Smithay behavior, runtime results and the popup exact-row oracle unverified. Author pinned-source checks and real execution remain separate evidence.

The later oracle, cleanup and redundant-focus corrections are author work after this frozen review. No third independent inspection or full production certification is implied. Native/hardware, physical presentation, transactions, scale, complete popup/device/input coverage, resource fairness and release operations remain open.
