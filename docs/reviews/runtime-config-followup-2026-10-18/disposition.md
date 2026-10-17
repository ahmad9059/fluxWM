# Author disposition: live rollback and generation follow-up

Claude inspected frozen `0d12c40a97ab145d7c5ad0b98adc269249ccdacb` using only Read/Grep. The [report](claude-report.md), [completion](stdout.json), [prompt](prompt.txt), [command](command.json), [execution](execution.json), [manifest](input-manifest.json) and [author verification](verified-execution.json) retain the actual record. Exit was zero after 104.34 seconds; all **68 input hashes match**, with no permission denials, outside-snapshot paths or error result. The actual trace has **16 calls**; the report self-describes 15. Claude executed no tests and did not rely on author execution logs.

The reviewer found **no reachable high or medium product defect in its focused scope**. It accepted the live-model rollback closure, gap1024/1025 boundary, prepared-generation deferral/supersession, separate controller stale/closed gates, retirement clarification, NOCTTY flag and hermetic non-config smoke commands. It found a further **medium binding-swap oracle gap**, accepted below. This remains a focused source review, not production certification.

## Accepted medium: retained initial bindings could satisfy the test

The initial, prepared and newly published tables all contained the same Alt+a action and four entries. A runtime retaining its initial table during subsequent reloads would still pass the input test, even though the actual swap implementation was correct by source reading.

Later author-only commit `33deac3` starts with three entries and no Alt+a. After the fourth-entry replacement is prepared while Alt is held, A must still be delivered to the real client and mode stay strip. A newer prepared generation supersedes it. After release, the table must have four entries; Alt+a must now switch to canvas while consuming A press/release. A later plain A reaches the client. Complete A and Alt counts are each two presses/two releases, with the configured chord's A absent from client events.

A separate deliberate mutation preserves the old table **only on subsequent publications**, while still installing the initial three-entry table and updating layout/generation. The corrected real private-input case fails specifically with `user binding publication differs`. Thus startup behavior cannot mask the missing reload swap. Source and the exact original executable hash are restored; repaired checks and the six-case configuration suite pass. [Raw evidence](../../evidence/nested-milestone/2026-10-18-runtime-config/README.md) records both normal and deliberately faulty executable identities.

This correction and its experiments occurred **after the frozen follow-up**. Claude did not review or run them. Production logic remains unchanged from `0d12c40`; the later changes affect test assertions and harness behavior only. The binding fix is qualified on source `33deac3`; later `8152a96` adds the runnable example to the helper's existing integration test and standalone Lua checker. The final private suites requalify its actual rebuilt executable/helper/fixture hashes rather than assuming identity from a reviewed or earlier build. The 157 named tests remain unchanged in number.

## Lower-severity findings

Later author assertions now require `prepared_generation` to be null after a newer request but before the parked worker observes it, require gap1025 failure to retain the gap1024 applied generation, and recheck the wide member's ID, mapping and width after recovery.

The real supersession observations are samples and cannot exclude arbitrary unobserved transient histories; the deterministic controller tests separately check the latest gates. Geometry rollback checks the exact dispatch-side error and retained model state, but does not count wire configures across rejection or distinguish every internal layout validation sub-check. NOCTTY has source inspection and ordinary file tests, not a private-PTY side-effect test. Held pointer/grab deferral, automatic file watching, full schema/rules/autostart, keymap/device/output transactions, native input/session, global resource timing and soak remain open.
