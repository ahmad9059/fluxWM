# XDG window-state, visibility, and backdrop contract

Required FluxWM design, not an implemented protocol claim. Compare the inspected xdg-shell XML with the distribution-provenance record before selecting advertised versions. The [protocol source inventory](evidence/protocol-inventory.json) records source maximums, not supported FluxWM versions.

## Version gating

Only send events/states available at the client's bound interface version: tiled-edge states require version 2, `configure_bounds` version 4, `wm_capabilities` version 5, `suspended` version 6, and constrained-edge states version 7 in the inspected XML. The phase-1/2 protocol inventory and synthetic clients must check these exact thresholds against chosen bindings. Unsupported flags are omitted, never emulated by unknown enum values.

For version 5+ send the supported capabilities before initial configure, then update if capabilities change. Advertise maximize/fullscreen/minimize only when those commands exist; window-menu support requires a working menu. Earlier phases/clients receive only available behavior. For version 4+, bounds are usable-region hints for ordinary windows and full output extents for fullscreen; update when output/work-area changes. Bounds are not a destructive resize command.

## State mapping

`A` means activated only when actual keyboard focus/activation policy selects that toplevel. `S` means suspended hint when fully nonpresented and no active capture/thumbnail obligation needs fresh content. Suspension is not a guarantee the app stops rendering and is not a substitute for correct callbacks. Set constrained/tiled edge flags only where actual placement constrains adjoining geometry; do not mark a floating edge tiled merely because it touches a panel.

| Layout/presentation state | Client XDG states | Bounds / size intent | Visibility and backdrop |
| --- | --- | --- | --- |
| Tiled column | A as appropriate; truthful tiled/constrained edges | Target column rect; usable region bound | Ordinary workspace scene |
| Stack member | A on focused member; edge flags from stack geometry | Allocated stack rect | Visible stack content; respect real popup tree |
| Hidden tab | No A; S when no live capture/thumbnail use | Preserve allocated/restoration size, no forced zero-size | No ordinary input/render; task/search entry retained |
| Floating | A as appropriate; no invented tile flags | Client/user geometry, usable region bound | Floating layer with normal clipping |
| Maximized | `maximized`; A as appropriate | Exact configured usable-area geometry | Own usable-area state and stored restore geometry |
| Fullscreen focused/stationary | `fullscreen`; A | Full output extent as maximum geometry | Center smaller content; opaque backdrop independent of client alpha |
| Fullscreen unfocused/offscreen | `fullscreen` retained; no A; S if wholly hidden with no active use | Preserve fullscreen geometry; do not falsely send unfullscreen | Cull hidden tree; retain its opaque page/backdrop for any visible portion |
| Minimized | No A; S when no active use; preserve restore-state intent | Saved bounds/geometry; no zero-size sentinel | Hidden membership remains recoverable; task/search exposes restore |
| Offscreen canvas | A only if deliberately focused and reveal pending; S when eligible | Preserve world group's client-local target size | Cull outside view; do not transmit world coordinates |
| Overview | No fake maximize/fullscreen toggles solely for zoom; activation follows UI focus policy | Original target sizes; no per-frame configure spam | Snapshot/live thumbnail presentation; native UI focus separate |

For a new buffer committed in response to an acknowledged maximized configure, size mismatch must follow protocol validation; do not reject an older valid fallback buffer just because a new configure is pending; do not blindly stretch invalid new content and claim compliance. Fullscreen content may be smaller within its allowed geometry: center it and fill uncovered area safely. Neither client alpha nor stale opaque-region metadata can reveal unrelated content beneath its fullscreen tree.

## Fullscreen scrolling policy

Fullscreen stays an output-sized **page** in strip order or a saved output-relative presentation in canvas mode, with a compositor-owned opaque backdrop. Focus changes remove `activated` without automatically unsetting fullscreen. During a camera transition, the fullscreen page and its backdrop move/clip together; unrelated tiles can appear outside that page, never through/below its client tree or unfilled letterbox. At stationary fullscreen focus the page covers the output and top panels/floating content are suppressed according to product policy. Closing snapshots and overview thumbnails inherit privacy and backdrop requirements.

If an actual toolkit/renderer path cannot satisfy this policy, record a B19 failure and revise the client-state transition design explicitly; do not silently strip fullscreen state to make a visual test pass. Test transparent/undersized clients, popup trees, workspace/output movement, fractional transforms, and fullscreen games. `set_fullscreen(output)` is a preference: honor a live eligible requested output when policy permits; otherwise choose the current focused/owning output and configure actual state. Destroyed outputs cannot remain targets.

Fullscreen/maximize requests received while another state is active still get protocol-required configure replies and update restore intent according to selected policy. Unmaximize/unfullscreen restores saved geometry where valid, resolving removed outputs/changed constraints. Preserve a bounded state history rather than an unbounded stack of repeated requests.

## Verification and blocker

B19 design belongs in phase 4; native toolkit/fullscreen/bridge behavior is qualified in phase 7; canvas/overview rows in phase 8. SURF-09 and LAY-09 compare exact state arrays, configure serials, version gates, geometry, alpha backdrops and focus. Bind versions 1 through selected maximum in synthetic clients. Hidden-surface callback policy must be tested with video, browser timers, capture and restoration; `suspended` is a hint rather than a license to deadlock clients.

Minimize first becomes implemented in phase 4; do not advertise its wm_capability earlier. SURF-09:strip-native qualifies phase-7 native/X11 rows, while full SURF-09 qualifies canvas/overview in phase 8.
