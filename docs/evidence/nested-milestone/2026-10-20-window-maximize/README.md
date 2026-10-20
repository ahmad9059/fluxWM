# Maximize/restore evidence, 2026-10-20

[Final twelve checks](checks.json) passed: default/all-feature workspace tests, all-target check, Clippy,
rustdoc, formatting, docs integrity, diff check and final builds. [Inventory](native-test-list.log) records
**264** registered all-feature tests; [raw test output](native-tests.log) is retained unchanged. The layout
suite has 33 tests and includes 10,000 deterministic mixed commands.

Final binaries ran nested mode on owned authenticated software-rendered Xvfb:

- [Default wrapper](default-gui.json): [maximize](default-maximize-summary.json),
  [minimize](default-minimize-summary.json), [configure lifecycle](default-configure-summary.json),
  [existing lifecycle](default-nested-summary.json).
- [Native-feature wrapper](native-feature-gui.json): [maximize](native-feature-maximize-summary.json),
  [existing scene regression](native-feature-scene-summary.json). This still runs nested mode only.

The maximize client checks preinitial and held-initial-ack requests, repeated set/unset replies, bounded
unacknowledged requests, legal old-normal fallback commits, actual exact-size SHM buffers, restoration,
minimize/maximize interaction, unmap reset, wrong-size commit error and independent-client survival. Overlap
coverage establishes pointer and keyboard focus on a later maximized peer, then requires both to move to an
earlier peer. Fixed-size requests must decline safely; hints introduced with the first correctly promised-size
buffer must admit normally and produce a corrective configure and another normal-size frame.

[Source/harness hashes](source-hashes.json) and [executable hashes](executables.json) identify final bytes.
Owned-server reaping, private endpoints, authority cleanup and parent environment checks passed. The
[initial capability-oracle failure](initial-capability-oracle-failure.json) preserves a fixture bug: capabilities
were incorrectly compared in wire order. Sorted set comparison now accepts either order while rejecting
wrong, duplicate or malformed values; raw event bytes remain in the report.

Actual title-bar clicks, queued-click minimize, toolkit/protocol-version matrices, physical native input,
KMS/presentation, output migration and fullscreen combinations remain unqualified. Source review accepted
click authorization and event-loop caller ordering; it does not replace those runtime tests. The raw test
log contains original trailing spaces and is the only exception to the staged whitespace check.

See [behavior](../../../development/maximize.md) and
[review disposition](../../../reviews/window-maximize-caller-2026-10-20/disposition.md). No phase gate is closed.
