# Floating-window evidence, 2026-10-20

[Twelve checks](checks.json) passed: default/all-feature workspace tests, all-target check, Clippy, rustdoc,
formatting, docs integrity and final builds. [Inventory](native-test-list.log) records **281** all-feature
tests, including 49 layout tests and 10,000 deterministic mixed commands extended with floating state,
rectangle and constraint edits. [Raw output](native-tests.log) is retained unchanged.

Final executable bytes ran on owned authenticated software-rendered Xvfb:

- [Default wrapper](default-gui.json): [floating](default-floating-summary.json),
  [fullscreen](default-fullscreen-summary.json), [maximize](default-maximize-summary.json),
  [minimize](default-minimize-summary.json), [close](default-close-summary.json) and
  [configure](default-configure-summary.json).
- [Native-feature wrapper](native-feature-gui.json): [floating](native-feature-floating-summary.json)
  and [close](native-feature-close-summary.json), still in nested mode only.

The lifecycle fixture uses real XDG roles and SHM commits: floating dimensions follow actual content,
keyboard move/resize persists through commits, camera pan retains world geometry, canvas/strip changes,
maximize/fullscreen/minimize restoration, tiled restoration and null-buffer remap are checked. Same-client
parent dialogs center on their parent model geometry and survive parent destruction. Exact-ID focus,
malformed and stale IDs are exercised. Review regressions require a committed overlay-sized buffer to
restore a 60%-viewport float, and ordinary parent/neighbor geometry to remain unchanged when a dialog maps. A born-floating dialog first
retiled in Canvas must get ordinary dimensions beside the unchanged neighbor.

Pixel checks require the complete 320×240 magenta float above an explicitly overlapping focused tile.
A focused fullscreen frame requires exact content/black pixels and no magenta. That camera also relocates
to the fullscreen page, so the latter is an observed-frame check, not proof of overlapping-layer exclusion.

[Initial wrapper](initial-fixture-failure-gui.json) and [summary](initial-fixture-failure-summary.json) retain
two fixture bugs: a `maximize`/`maximized` query-key typo and a settle comparison that included changing
configure/ack counters. Corrected oracles compare named state/geometry. No compositor defect is inferred
from those fixture failures. Independent review separately found model and admission defects; the retained
disposition records their corrections and regression cases. [Source/harness hashes](source-hashes.json) and [executable hashes](executables.json)
identify final bytes. Wrappers retain authentication, cleanup, owned-server reaping and unchanged environment.

No host input injection, native-device access or package installation was used. Interactive move/resize
is source-integrated but actual authorized pointer grabs, delayed resize transaction correlation, toolkits,
complete popup interactions, output migration, transforms and hardware remain unqualified.
See [behavior](../../../development/floating.md) and
[review disposition](../../../reviews/window-floating-2026-10-20/disposition.md). Raw test logs retain
original trailing spaces. No phase gate is closed.
