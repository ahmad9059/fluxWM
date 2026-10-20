# Fullscreen pages

Fullscreen now supports client set/unset requests and the focused IPC/binding action:

```sh
fluxctl --socket /exact/instance.sock action '{"name":"fullscreen","toggle":true}'
fluxctl --socket /exact/instance.sock action '{"name":"fullscreen","toggle":false}'
```

`true` toggles and `false` leaves fullscreen. `windows.list.fullscreen` reports intent; configure diagnostics
separately report desired, sent and committed state. The only eligible output is the current output, so other
or stale client output preferences fall back to it. Fullscreen, Maximize and Minimize capabilities are enabled.
Requests before the first buffer, repeated requests and held acknowledgments use bounded configure replies.

Each fullscreen member gets an output-sized page. Strip pages follow their saved column; canvas pages form
a separate bank beyond all saved groups, so they cannot overlap normal windows. Canvas pages move when the
group frontier changes. The original slot stays reserved, even when empty. Other column widths, stack/tab state, group geometry and maximize restore
intent remain saved. Multiple fullscreen members get separate pages. Focus changes retain fullscreen; page
navigation and reveal use projected geometry. Minimize hides the page and restore reveals it. Unfullscreen
restores ordinary or compatible maximized presentation; null-buffer unmap clears both intents.

Smaller client geometry is centered within the page. An opaque black backdrop fills the page independently
of client alpha, opaque-region claims and whether any client pixels remain on screen. Fullscreen surfaces,
including popups and subsurfaces, cannot use client opaque hints to occlude that backdrop. Popups and content
clip to the same page/output intersection. Letterbox input stops at the backdrop; frozen backdrop pixels keep
blocking lower windows until a replacement scene presents, even after the old owner disappears.

Committed fullscreen geometry must be positive and no larger than its acknowledged configure. Older valid
fallback buffers remain valid while a new configure is unacknowledged. Oversized committed geometry causes
`xdg_wm_base.invalid_surface_state` for the offending client. Client minima that cannot fit decline initial
fullscreen; later incompatible environment/constraint changes clear it. Coordinate overflow rejects edits
atomically. Unrelated input grabs decline transitions; owned implicit clicks remain eligible.

Backdrop elements currently receive a new render identity on each capture, causing full backdrop damage.
Minimal damage and power/performance budgets remain open.

Local evidence covers real nested-client lifecycle and exact full/partial-page pixels, including a transparent
client falsely claiming opacity. Native hardware, actual toolkit/title-bar behavior, multi-output migration,
fractional/rotated transforms, animation transitions and the complete phase-4/7 matrices remain unqualified.
See [the state contract](../22-window-state-matrix.md) and [phase 4](../phases/04-scrolling-layout.md).
