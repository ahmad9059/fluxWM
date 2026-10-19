# Minimize and restore

Mapped XDG toplevels now support client `set_minimized` requests and these IPC actions:

```sh
fluxctl --socket /exact/instance.sock action '{"name":"minimize"}'
fluxctl --socket /exact/instance.sock action '{"name":"restore-window","window":"0:0"}'
```

Use the actual generational ID from `windows.list`. `minimize` targets the focused window. Restore targets
an explicit live mapped ID, focuses it and reveals it. Repeating an unchanged request is a no-op, so restoring
an already visible window does not steal focus. Unknown, stale and unmapped IDs fail. The actions are also
available to typed configuration bindings.

Minimization retains the mapped role, buffer ownership, layout membership, column slot, nonzero target
geometry, constraints and canvas group. Other windows do not compact into the reserved slot. Reordering swaps with the nearest selectable neighbor,
skipping minimized columns or stack members while retaining their reserved positions. `windows.list`
reports `minimized` separately from `mapped`; it remains possible to discover and restore every hidden member.
Null-buffer unmap removes membership and minimized state; remapping starts an ordinary window cycle.

Minimized windows are excluded from render/input eligibility, focus navigation and frame callbacks. Popup
trees are dismissed, activation is removed and the suspended hint is sent where the client version supports
it. Existing pending configure flow remains bounded; an ack is not proof the new state was committed. A client
buffer commit does not restore its window. All minimized means no selected window or keyboard focus.

Unrelated active input grabs reject IPC transitions and decline advisory client minimize requests. A root's
own tracked popup grab is dismissed through the existing popup cleanup path. There is no automatic retry.
Only Minimize is advertised in XDG optional capabilities; maximize, fullscreen and window menu remain absent.
The pinned Smithay state serializer filters version-dependent hints.

This implements the minimize/restore portion of [phase 4](../phases/04-scrolling-layout.md). It does not close
that phase, qualify native hardware, or implement task UI, maximize/fullscreen restoration, output migration,
capture or accessibility. The pure model retains randomized invariant checks; the owned private-display
fixture exercises real protocol and IPC transitions. Their actual results are recorded with the milestone.
