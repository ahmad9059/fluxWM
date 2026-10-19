# Author disposition: native compositor review

The [initial report](claude-report.md) inspected a frozen worktree snapshot based on `328ac7c`.
[Verified execution](verified-execution.json) records 27 Read/Grep calls over 61 hashed input files,
with no requests outside the snapshot or permission denials. This is source review, not execution.

The medium topology finding remains an operational limitation: any selected-GPU topology change stops the
session and can leave scanout unrestored. The driver now emits `GpuChanged` rather than `GpuRemoved` and
skips GPU re-preparation when retiring live output. A pure lifecycle-policy test distinguishes these events;
no hardware recovery is claimed.

Startup now constructs the Context/Handle guard before the initial modeset, validates layout/ping setup first,
and explicitly shuts down after bootstrap or source-registration failure. Cleanup failures reach session
failure reporting. Shutdown avoids duplicate Drop logging. Cursor budget warnings are rate limited, pointer
clamping is consistent, the full-redraw dependency is documented and dead cookie code is removed. Retained
frame payloads and Wayland surface references are documented as failure containment.

The author also added an event-stream restart boundary: after successful blocking bootstrap, advance a
checked generation and drain the nonblocking stream within a fixed budget. Already-read events keep the old
stamp; drain failure seals startup and retains the output for rollback. An uncertain bootstrap invalidates
its ledger. Owned-stream tests exercise generation changes during a decoded batch and drain rejection.

The focused follow-up is recorded separately. Device-free tests and nested GUI regressions cannot qualify
native Context/draw/watchdog execution, cursor imports, output orientation, VT/hotplug recovery or drivers.
User-staged package files are excluded from this implementation commit.
