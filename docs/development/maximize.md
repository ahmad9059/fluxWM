# Maximize and restore

Mapped and initially unmapped XDG toplevels accept maximize/unmaximize requests. Optional capabilities now
advertise Maximize and Minimize; fullscreen and window-menu capabilities remain absent. Typed bindings and
IPC accept the focused-window action:

```sh
fluxctl --socket /exact/instance.sock action '{"name":"maximize","toggle":true}'
fluxctl --socket /exact/instance.sock action '{"name":"maximize","toggle":false}'
```

`true` toggles, while `false` explicitly unmaximizes. Client XDG requests set the requested state explicitly
and do not steal focus. `windows.list.maximized` reports desired state; configure diagnostics separately
report desired, sent and committed Maximized flags.

Maximize is a viewport presentation overlay over retained layout membership. The underlying column slot,
width policy, stack/tab state, constraints and canvas geometry remain intact. Its rectangle and clip follow
the usable viewport; ordinary focused windows can be raised above a background maximized window. Navigation
uses saved layout geometry, including when several maximized windows share the presentation rectangle.
Inactive tabs and minimized members remain hidden even when their Maximized state is retained.
This policy does not implement fullscreen pages or an opaque privacy backdrop. Minimize preserves maximize
intent; restore reveals that state. Null-buffer unmap clears both states. Viewport changes recompute the
maximized target; extreme unsupported coordinate/zoom combinations fail instead of overflowing.

Repeated XDG state requests receive configure replies, including unchanged state. One unacknowledged
configure and one coalesced required reply bound the wire queue; the latest desired state is sent when an ack
frees the slot. Initial requests fold into the first configure. A stale ack after unmap cannot start a new
mapping cycle. Unrelated active input grabs decline state changes while still replying with actual state. An owned implicit
click grab permits maximize; minimize waits for that click to end before hiding its owner. Actual title-bar
click behavior and toolkit compatibility remain unqualified.

A buffer commit is checked against its committed acknowledged configure. If that configure is Maximized,
its actual window geometry must equal the configured size. A newer unacknowledged maximize target does not
invalidate an older normal fallback buffer. Invalid committed geometry posts `xdg_wm_base.invalid_surface_state`
and terminates the offending client, without terminating the compositor. Requests whose exact target conflicts with client min/max size hints are declined. Later constraint or
viewport/camera/mode changes clear an incompatible maximize state and preserve base layout geometry.
Hints introduced with the first promised-size buffer fall back to normal admission and a corrective configure,
rather than leaving the window stuck in an admission retry. Client size hints remain saved for restoration.

The implementation has one usable output and no panel reservations. Native hardware, output migration,
maximized/fullscreen combinations, full protocol-version/toolkit coverage and phase 4 remain unqualified.
See [the window-state contract](../22-window-state-matrix.md) and [phase 4](../phases/04-scrolling-layout.md).
