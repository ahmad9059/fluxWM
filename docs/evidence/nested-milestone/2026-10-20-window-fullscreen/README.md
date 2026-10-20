# Fullscreen evidence, 2026-10-20

[Final twelve checks](checks.json) passed, including default/all-feature workspace tests, all-target check,
Clippy, rustdoc, formatting, documentation integrity and final builds. The [inventory](native-test-list.log)
records **273** registered all-feature tests, including 41 layout tests and 10,000 deterministic mixed
commands. [Raw output](native-tests.log) is retained unchanged.

Final executable bytes ran nested mode on owned authenticated software-rendered Xvfb:

- [Default wrapper](default-gui.json): [fullscreen](default-fullscreen-summary.json),
  [maximize](default-maximize-summary.json), [minimize](default-minimize-summary.json),
  [configure](default-configure-summary.json) and [lifecycle](default-nested-summary.json).
- [Native-feature wrapper](native-feature-gui.json): [fullscreen](native-feature-fullscreen-summary.json)
  and [scene](native-feature-scene-summary.json). This also runs nested mode only.

The fullscreen client exercises requests before mapping/initial acknowledgement, repeated and held
acknowledgements, bounded configure replies, compatible maximize restoration, minimize/restore, focus away,
unmap and same-role wire-state reset on remap. Oversized fullscreen geometry errors only the offending
client; an independent healthy client still presents. Exact PPM pixel checks require black letterbox and
transparent areas despite false full-client opaque hints. Partial-page panning puts the entire client
outside the viewport while its remaining page edge must stay black.

[Executable hashes](executables.json) and [source/harness hashes](source-hashes.json) identify the final
bytes. Wrapper reports record private endpoint/authority cleanup, owned server reaping and unchanged parent
environment. Hardware paths, host input injection and package installation were not used.

The initial review led to canvas occupancy, strip navigation and coordinate-bound corrections;
[disposition](../../../reviews/window-fullscreen-2026-10-20/disposition.md) records remaining limits.
Full backdrop damage per capture is a known performance gap. Physical native hardware, actual toolkit
behavior, multi-output migration, transforms, animations and the full state matrix remain unqualified.
The raw test log retains original trailing spaces. No phase gate is closed.
