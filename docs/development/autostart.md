# Nested autostart ownership

A successfully published runtime configuration schedules its `autostart` list on a separate process worker. Lua/helper, binding and layout rejection schedule nothing. Logical configuration publication precedes external process effects: `configuration.status=applied` does **not** mean a companion has started or become ready. Query `diagnostics.autostart` separately. This is optional direct-child supervision, not a production desktop/session manager.

```lua
return {
  schema_version = 1,
  autostart = {
    { id = "panel", argv = { "/absolute/path/to/panel", "--flag" }, restart_on_change = true },
    { id = "once", argv = { "/absolute/path/to/setup" } },
  },
}
```

This example requires installed executables; it is not a runnable bundled desktop. Arguments are passed directly, without shell expansion. Executables without a slash use inherited PATH, so use absolute paths where selection must be deterministic. Child environment inherits the parent except that `WAYLAND_DISPLAY` and `FLUXWM_SOCKET` identify this compositor, and stale `DISPLAY`/`WAYLAND_SOCKET` are removed. INT, TERM and CHLD are unblocked before exec. No parent environment mutation occurs.

## Identity and reload contract

Optional `id` is a unique, nonempty, NUL-free UTF-8 string of at most 128 bytes. A named identity survives reordering. Without an ID, the zero-based list position is the identity; removing or inserting an entry can change which command that position denotes. Use IDs for reorderable companions. Named `"0"` and positional index zero are distinct.

| Desired change | Behavior |
| --- | --- |
| First appearance of an identity | One execution attempt |
| Unchanged argv, whether running, exited or exec-failed | No retry, including when `restart_on_change=true` |
| Changed argv with restart false | Retain the old child/attempt history; report `change_deferred=true` |
| Changed argv with restart true | Retire the old owned child before attempting the replacement |
| Removal | Terminate/reap the owned child; retain a bounded identity tombstone |
| Reintroduction with restart false | Suppress a second attempt for that identity in this compositor session |
| Reintroduction with restart true | Attempt again |
| Replacement exec failure | Old child stays stopped; record the failed attempt without rollback |
| Child exit/crash | Record status; no automatic respawn/backoff |

The last attempted argv, rather than the last desired argv, determines changes. Enabling restart after a deferred argv change therefore attempts the desired command. A newer plan can supersede one during retirement. A deliberately retired restartable identity retains its restart requirement even if the newer plan restores the original argv. An execution already admitted can finish after supersession or stop; the worker then reconciles the latest plan or shuts down. There is no instantaneous exec cancellation or cross-process transaction/readiness guarantee.

## Bounds, diagnostics and shutdown

A plan contains at most 128 entries and 4 MiB of total argv bytes, subject to the existing per-command argv limits. A session retains at most 256 identities and 8 MiB of last-attempt argv. Removed entries release argv storage but retain their identity tombstones until session exit. A plan exceeding the session budget is rejected before its process effects: previous autostart ownership remains, while the already-published logical configuration remains applied. Remove entries to release retained argv; a new compositor session is required to reset identity history.

Admission retains one replaceable pending plan and one worker-owned discard slot. Backpressure makes the compositor retry its latest pinned plan after notification. Executable work, termination, waits and large plan/history disposal occur on workers. Snapshot history is bounded; no argv or source path is included in diagnostics. Explicit IDs are public diagnostic labels, so avoid secrets in IDs.

`diagnostics.autostart` distinguishes latest root `requested_generation`, worker `observed_generation`, and `active_generation`. Generations and attempt/start counters are decimal strings. `settled` means the latest observed plan finished its decisions, including failed exec; it is not readiness. A session-budget failure has an observed generation newer than the retained active generation and a static error. Entries expose ID/current desired index, stable history slot, desired membership, PID, attempts, successful starts, deferred-change flag, static error/errno, exit code/signal and state. An empty accepted list removes current companions. `--no-config` disables this worker; the explicit command after `--` has separate ownership.

SIGCHLD wakes targeted owned-child checks; the worker does not reap arbitrary children or run an idle polling timer. Removal/replacement/shutdown broadcast TERM to the affected owned children before one shared 250 ms grace interval, then kill/reap remaining direct children. The grace is not a universal shutdown deadline: kernel exec/kill/wait and worker scheduling can stall. Quit/signal/host-close/fatal paths seal new admission. Final stats include the closed snapshot after join.

Descendants, daemonization, process groups, required companion dependency order, readiness protocols, crash recovery, restart backoff, environment activation, a session bus and native logout remain unimplemented. A child that forks a daemon can leave that descendant alive. These limitations block treating this worker as complete session supervision.

## Verification scope

Rust tests use real owned sleep/fast-exit/missing-executable children; verify stable named reorder, deferred/restarted argv, removal/reintroduction, failure no-retry, stale/closed admission, identity/argv budgets, supersession during real TERM retirement, and six TERM-ignoring children sharing a grace interval. Schema tests cover optional IDs, byte bounds, NUL and duplicates. The private harness is `tools/smoke_autostart.py`; its retained results identify actual executions rather than implying qualification from this source description. No native, descendant, sustained reload/soak, whole-loop latency or production-session pass is claimed.
