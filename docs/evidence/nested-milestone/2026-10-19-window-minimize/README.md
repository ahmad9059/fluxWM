# Window minimize/restore evidence, 2026-10-19

[Twelve local checks](checks.json) passed, including default/all-feature workspace tests, all-target checks,
Clippy, rustdoc, formatting, documentation integrity and final builds. [Inventory](native-test-list.log)
records **260** registered all-feature tests; [raw test output](native-tests.log) preserves its trailing spaces.
The final whitespace check excludes that raw artifact only.

Both final executable profiles ran **nested mode only** on owned authenticated software-rendered Xvfb:

- [Default profile wrapper](default-gui.json), [minimize fixture](default-minimize-summary.json),
  [existing lifecycle regression](default-nested-summary.json).
- [Native-feature profile wrapper](native-feature-gui.json), [minimize fixture](native-feature-minimize-summary.json),
  [existing scene regression](native-feature-scene-summary.json).

The real client checks exact Minimize capabilities, background XDG minimize, focused IPC minimize,
restore identity/nonzero geometry, focus fallback/deactivation, hidden commits and a 120 ms frame-suppression
observation, mapped popup dismissal, all-hidden keyboard focus, pending-frame recovery, null-buffer
unmap/remap and stale-ID rejection after actual slot reuse. Suspended and restored activation/focus are checked
on the negotiated protocol version. The layout tests include minimize/restore in 10,000 deterministic
randomized commands and named tab/stack/group/constraint cases.

[Source/harness hashes](source-hashes.json) and [executable hashes](executables.json) identify the exercised
bytes. Private server reaping, environment preservation and endpoint cleanup passed. These tests do not
qualify a full protocol version matrix, pixels, active-grab transitions, toolkit compatibility, native
hardware or phase 4. The empty-pointer assertion lacks a prior-focus precondition and does not prove pointer revocation.
Capability/state version filtering was inspected in pinned Smithay source.

After source review, [affected fixture checks](post-review-checks.json) and both GUI profiles were repeated
for strengthened protocol/state and reused-slot tests. Their results above identify the final fixture bytes.

See [behavior and limits](../../../development/minimize.md) and the independent review records. No physical
DRM session, host input injection, push or package installation was performed.
