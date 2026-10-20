# Caller and final correction disposition

[Final narrow review](claude-report.md) verifies `main.rs` calls `finish_dispatch` after every calloop
iteration, including input events. It closes the earlier source-level medium evidence gap and accepts the
late-hint admission fallback. [Verified execution](verified-execution.json) records **6** calls over 28
hashed inputs; the report self-reports 5. There were no permission denials or outside-snapshot requests.
[No input bytes changed afterward](post-review-changes.json).

One optional low remains: pending minimize is checked before popup cleanup. If a later popup grab ends only
during that cleanup, an already queued minimize can wait for the next event-loop wakeup. It is bounded and
retained, with no busy polling; immediate completion in that interleaving is not qualified. Explicit
unrelated startup grabs still decline requests. The protocol error uses a live same-client wm_base rather
than proving the originating binding when a client binds more than one.

The [final local evidence](../../evidence/nested-milestone/2026-10-20-window-maximize/README.md) records 264
tests and both executable profiles' nested GUI checks. Actual title-bar clicks, delayed click minimize,
toolkits and native hardware remain untested at runtime. Reviews are source inspection, not certification
or phase-gate closure.
