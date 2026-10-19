# Optional native compositor: local evidence, 2026-10-19

The frontend now connects the native driver to Wayland scenes, input, output publication and software
cursor rendering. Native hardware execution remains unqualified. These records cover source/build and
nested regression checks only.

- [Twelve local checks](checks.json) passed: default/all-feature workspace tests, inventory, all-target
  check, Clippy, rustdoc, formatting, documentation links, diff check and three final builds.
- [Inventory](native-test-list.log): 257 registered all-feature workspace tests; [test log](native-tests.log).
- [Five affected checks](post-review-checks.json) were rerun after the one-line review correction;
  the retained test log and native-feature GUI/probe records are from these final bytes.
- [Final executable hashes](executables.json) distinguish default and native-feature builds.
- Both binaries ran **nested mode only** on owned authenticated software-rendered Xvfb:
  [default lifecycle](default-gui.json), [suite results](default-nested-summary.json),
  [native-feature scene regression](native-feature-gui.json), [suite results](native-feature-scene-summary.json).
  Parent environment preservation, owned-server reaping and private endpoint cleanup passed.
- [Read-only probe audit](probe-audit.json): 439 checked opens, zero blocked calls, owned child reaped.
  This audits `--check`, never `--session` or the live native compositor.
- [Source and harness hashes](source-hashes.json) identify the implementation bytes exercised by the builds.

New tests cover stale DRM batch generations, bounded nonblocking drain, exact-completion content privacy,
selected topology/removal policy, options and cursor-coordinate arithmetic. They do not exercise native
Context/draw/watchdog or client cursor imports against physical hardware. No DRM modeset, seat acquisition,
input injection into the host, VT switch, hotplug recovery or physical presentation test was performed.
CI configuration was updated; local success is not evidence that remote CI ran.
The raw test log preserves subprocess output, including trailing spaces; the final staged whitespace check
excludes that artifact only. [Final documentation checks](final-docs.json) passed.

The [implementation scope](../../../development/native-compositor.md) and
[production gaps](../../../development/production-status-2026-10-18.md) remain authoritative limits.
