# Native frame transport validation, 2026-10-19

All eleven [workspace checks](checks.json) passed: default and all-feature tests, test inventory, all-target checking and Clippy, rustdoc, formatting, documentation integrity, diff whitespace and final default/native builds. [All-feature output](native-tests.log) and [inventory](native-test-list.log) record 240 registered tests. Child-process test summaries also appear in the output; they are not additional registered tests.

The device-free [C/Rust ABI comparison](drm-abi.json) agrees on atomic/event sizes, cookie/CRTC offsets, ioctl number and TEST_ONLY flags. The [owned-child read-only probe audit](probe-audit.json) observed 439 checked opens, zero blocked calls and reaped its child. It executed `--check` only; no native `--session` command ran.

[Final executable hashes](executables.json) match the previously exercised nested compositor/helper/fixture bytes in the [native foundation run](../2026-10-18-native-foundation/runtime/execution.json). Those GUI scenarios were not repeated for this native-only change. The changed native probe was audited separately. [Source hashes](source-hashes.json) identify the implementation input.

See the [implementation contract and remaining integration](../../../development/native-frame-transport.md). These results do not establish a live native desktop, physical presentation, driver compatibility, complete teardown or production readiness.

Raw test output retains child-process interleaving and trailing whitespace. The final staged whitespace check excludes only `docs/evidence/nested-milestone/2026-10-19-native-frame-transport/native-tests.log`; authored source and documents pass.

The [independent Claude review](../../../reviews/native-frame-transport-2026-10-19/disposition.md) found no high/medium defect in its static scope and retained explicit low/future-integration limits.
