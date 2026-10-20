# Client-close and disconnect evidence, 2026-10-20

The [twelve checks](checks.json) cover default/all-feature workspace tests, all-target check, Clippy, rustdoc,
formatting, docs integrity and final builds. [Inventory](native-test-list.log) records 273 all-feature tests;
existing strict action roundtrip/unknown-field tests now include explicit disconnect and malformed targets.
After review, the final fixture assertions were strengthened, rebuilt and checked with
[Clippy](fixture-final-clippy.log); both final nested wrappers below use those bytes. Runtime source did
not change after the frozen independent review. [Final docs check](final-docs.json) includes all retained links.

Final actual binaries ran on owned authenticated software-rendered Xvfb:

- [Default wrapper](default-gui.json): [close/disconnect](default-close-summary.json),
  [configure](default-configure-summary.json) and [minimize](default-minimize-summary.json).
- [Native-feature wrapper](native-feature-gui.json): [close/disconnect](native-feature-close-summary.json)
  and [fullscreen](native-feature-fullscreen-summary.json), still in nested mode only.

The real fixture receives one graceful-close event, refuses it, navigates and presents another frame.
Explicit recovery targets an unmapped live role with mapped and minimized siblings on the same connection.
The reply names exactly those three IDs. With no further peer requests/flush or healthy-client traffic,
the victim must reach transport hangup within a 350 ms fixture observation budget. The role IDs disappear;
a same-process, same-app-id second connection presents again. A stale disconnect target fails without
publishing another sequence. This budget is a regression deadline, not a production latency guarantee.

The [negative wrapper](silent-peer-negative-gui.json) and [negative summary](silent-peer-negative-summary.json)
preserve the failed quiet-peer test against the first general-dispatch wake implementation. It accepted
the exact target and then timed out waiting for hangup. The pinned Rust backend only runs general-dispatch
cleanup after readable events. Final code explicitly dispatches queued retired clients, which unconditionally
drains their backend cleanup. The initial weaker fixture sent a roundtrip and masked this issue.

[Executable hashes](executables.json), [source/harness hashes](source-hashes.json), wrapper cleanup and raw
logs retain the scope. No host input injection, native device access or package installation was used.
Hardware, toolkits, authorized active grabs, complete selection/clipboard cleanup and protocol-version
matrices remain unqualified. This action closes a Wayland connection; it does not terminate a process.
Raw test logs retain original trailing spaces. See [behavior](../../../development/client-close.md) and
[review disposition](../../../reviews/client-disconnect-2026-10-20/disposition.md). No phase gate is closed.
