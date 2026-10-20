# Graceful close and client disconnect

`close-window` sends `xdg_toplevel.close` to the focused live toplevel. The client can refuse; the window
remains mapped, navigable and eligible for frames. No timeout automatically escalates to disconnect.

Explicit recovery targets a window ID from this instance's `windows.list`:

```sh
fluxctl --socket /exact/instance.sock action '{"name":"disconnect-client","window":"0:0"}'
```

Replace the example ID with the chosen live role. Targeting does not depend on current focus. A minimized
or unmapped live role is eligible. A destroyed/stale ID or a client already disconnecting is rejected.
There is no default disconnect shortcut; the action is available to explicit IPC and configured bindings.

The response reports `accepted`, `scope: "wayland-client-connection"`, `affected_windows` (the current live
toplevel IDs on that connection) and `other_windows_may_be_lost: true`. The effect covers the whole Wayland
connection, including its popups, selections and other protocol objects. The ID list is a snapshot of
managed toplevels, not an inventory of every affected protocol object. Removal completes asynchronously
through the event loop; callers can query until those IDs disappear.

The target is resolved from the owned server resource/client identity, never app ID, title or PID metadata.
Other connections remain independent even if the same process owns them. This action closes the Wayland
connection; it does not signal or terminate the operating-system process. The backend queues destructors,
and a coalesced one-shot wake dispatches the retired client directly, draining destructors without further
peer traffic. A general readable-client dispatch alone does not guarantee this cleanup on the pinned Rust backend.
Repeated requests are declined as soon as the backend reports disconnection. Existing lifecycle cleanup
repairs layout, popup trees and focus.

Local qualification uses owned authenticated nested Xvfb: a client refuses graceful close and still
presents, then explicit recovery through its unmapped role removes its mapped and minimized siblings.
A second connection with the same app ID in the same fixture process keeps presenting. Stale-ID recovery
is rejected without publishing another sequence. Actual toolkit behavior, authorized active grabs, native
hardware, complete clipboard/selection cleanup and protocol-version matrices remain unqualified.

Within one pipelined IPC batch, other window actions and queries can still observe a retiring role until
the cleanup wake runs. Only close/disconnect directly check its disconnected flag. Recovery acceptance
is not a completion barrier; wait for ID disappearance before acting on the remaining layout. Expected
retirement IO errors currently share a debug log with unexpected errors. If teardown stops before the
wake, remaining resources close with display ownership rather than a separately guaranteed final
destructor pass. These operational limits remain open.
