# Floating windows and tiled restoration

The focused action enters or leaves the floating layer:

```sh
fluxctl --socket /exact/instance.sock action '{"name":"floating","toggle":true}'
fluxctl --socket /exact/instance.sock action '{"name":"floating","toggle":false}'
fluxctl --socket /exact/instance.sock action '{"name":"focus-window","window":"0:0"}'
```

`true` toggles; `false` returns to tiling. Replace the example ID with a live mapped ID from `windows.list`.
`focus-window` selects that exact member and reveals it, including coincident floating/tiled windows that
directional navigation cannot distinguish. Minimized members require `restore-window` first. Stale IDs
and transitions during unrelated grabs are rejected. Both actions are available to configured bindings;
there is no new default floating shortcut. Queries report `floating` separately from fullscreen/maximize.

A floating window has one active world-space rectangle. Its original column/group retains dormant
restoration metadata and a reserved slot, including when empty. It leaves active tiled stack/tab allocation;
remaining tiled members use that allocation. Floating focus preserves the column's selected ordinary tab.
Returning to tiling restores the saved width, stack/tab and column placement. Dialogs born floating use a
dormant zero-advance restoration slot; their admission does not move ordinary neighbors. Their first explicit
return to tiling materializes a normal slot atomically. The strip slot retains its index from admission;
canvas materialization seeds a normal group beside an ordinary neighbor rather than reusing dialog birth
geometry. The model emits one placement per live member. Floating movement changes only its own
rectangle: keyboard move steps by 32 world units, and resize changes its width. Camera pan changes the view,
not saved floating coordinates. Mode switches retain its world rectangle; canvas rebasing shifts it with
the world origin. Shared floating coordinates across independently saved strip/canvas cameras still require
cross-mode rebase qualification; a strip reveal can move the vertical camera for a float. Scale/zoom
presentation remains separately unqualified.

Initial floating geometry uses committed client geometry, current min/max constraints and the viewport
center. If the window is already maximized or fullscreen, use a constrained 60%-viewport default rather
than saving an overlay-sized buffer as floating restoration geometry. A newly mapped same-client XDG child with a live mapped parent floats centered on the parent's
model rectangle. Parentage is checked through live server resources; app ID/title do not select this policy.
It is evaluated on admission, not every title/parent update. A hidden or offscreen parent uses the viewport
center instead. Fitting windows are clamped into the viewport; oversized client minima align to its
leading edges and retain their required size. Parent destruction leaves the child's own
geometry and membership intact. This is transient placement, not modal/input exclusion or xdg-dialog support.

Floating geometry survives minimize, maximize and fullscreen overlays. Floating edits do not move fullscreen
pages or unrelated tiles. Floating and fullscreen layers may overlap; focused fullscreen raises its entire
owner group and trusted backdrop above floats. Unfocused fullscreen retains its page/state while floats
remain above the ordinary workspace layer. Geometry edits and coordinate overflow preflight atomically.
Unmap clears floating and overlay state; remap starts a new admission cycle, where live parentage can apply
again. Ordinary floats render above tiled windows even while a tile has keyboard focus. Floating focus
history controls their mutual stacking. Focused fullscreen remains the final raised owner.

Client move/resize requests require this seat, an owned implicit click and a currently held backend press
serial. Tiled, minimized, maximized and fullscreen windows decline those interactive requests. Pointer
movement updates model geometry rather than an independent renderer location; top/left resize keeps the
opposite edge anchored using the same accepted model constraints as the geometry update. Resize state/configure replies remain bounded. Root unmap,
role destruction and dead-role cleanup cancel that window's grab before a remap can reuse it. Camera-changing
IPC and configuration publication wait for grabs; interactive deltas use the captured starting zoom. Pointer motion defers popup-grab repair until the
callback releases its pointer lock, avoiding cleanup re-entry.

Local evidence covers model/randomized commands and real nested lifecycle/SHM pixels, including overlapping
float/tile stacking and a fullscreen frame with no floating pixels. The fullscreen camera also relocates,
so that frame alone does not prove overlapping-layer suppression. Actual authorized drag/resize, delayed
resize transaction correlation, toolkits, active popup chains, native input/hardware, output migration,
fractional/rotated transforms and full window-state qualification remain open.
