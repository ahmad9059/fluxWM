# Infinite canvas and scrolling layout

All algorithms here are FluxWM proposals. The horizontal column model is inspired by [niri's documented workflow](https://github.com/niri-wm/niri); the two-axis canvas is an explicit FluxWM extension.

## Model

`Output` owns an ordered workspace list and active workspace ID. `Workspace` owns its layout mode, window membership, focus history, mode-specific camera state, and floating layer. `Column` owns width policy and an ordered stack or tab group. `Tile` references a live window ID and constraint metadata. A canvas `Group` owns a world rectangle, local column layout, stacking key, and anchor. Window membership remains unique when modes change.

Represent widths as `FixedLogical`, `ViewportFraction`, or `PresetIndex`. Fractions use usable output width after panel reservations and outer gaps. Window minimum/maximum constraints and user widths may conflict. Preserve minimums, clamp valid maximums, then permit scrolling or clip oversized content under a documented policy. Never shrink unrelated columns just to fit the screen.

For a stack of n windows, available height is usable height minus internal gaps and decorations. Allocate minimum heights first, then distribute the remainder by weights, respecting maxima. If the minimum sum exceeds the available height, retain minimum heights and expose internal vertical overflow scrolling, keeping every stack member reachable; tab mode remains an explicit user choice. Do not create negative heights. Keep the previous valid layout if the usable region is temporarily empty.

## Coordinate spaces

| Space | Type and purpose |
| --- | --- |
| Canvas/world | `f64` logical positions or checked fixed point; arbitrary user placement |
| Output logical | Camera-relative positions and UI geometry |
| Physical framebuffer | Integer pixel bounds after scale and output transform |
| Surface local | Coordinates expected by the client surface tree |
| Buffer | Buffer transform, viewport, and buffer scale mapping |

Keep the long-range camera as an anchor ID plus a local offset where possible. For spatial canvas coordinates, rebase world origin when magnitudes threaten precision. Shift all group positions, camera positions, gesture anchors, animation endpoints, and spatial indices by the same delta atomically. Store the accumulated origin separately with checked arithmetic. Test precision at large positive and negative positions.

For camera `c`, world point `w`, overview zoom `z > 0`, and viewport origin `o`, output logical point is `p = o + z*(w-c)`. To pick, invert: `w = c + (p-o)/z`. Apply scale and output transform through typed conversion helpers. Round shared edges consistently in physical space; do not independently round position and width and create seams. Never send world coordinates to xdg-shell clients.

## Strip behavior

Maintain column order with widths and gaps. Column starts are prefix sums. Begin with a vector and cached prefix positions; profile before adding a Fenwick tree or balanced sequence. Position changes after insertion can be O(n), but visibility lookup should use binary search and visit only intersecting columns. Expected scene work is O(log n + k), where k is visible/prefetched tiles; this is a target to validate.

On open: choose workspace/rule, insert after active column, assign default width, configure the new client, set focus if activation policy permits, and minimally pan. On close: remove membership, select a live neighbor using deterministic focus history, and adjust camera only if necessary. On reorder: preserve camera anchor to the chosen focused item so the world does not jump unexpectedly.

Let a focused rectangle span `[L,R]`, visible camera range be `[c,c+W]`, and comfort margin be m. If width fits within `W-2m`, keep c when fully visible, otherwise choose the smallest c shift that places the rectangle inside margins. If it is oversized, align the requested edge or center based on explicit command. Camera overscroll is temporary elastic motion, not permanent unreachable space. Empty workspaces retain a valid origin and show the launcher/background.

Fullscreen follows the [client-state/backdrop matrix](22-window-state-matrix.md) and stores prior tile geometry and width policy. It uses the full output rather than usable work area when focused, while retaining a place in scrolling order. Panels hide according to fullscreen policy. Leaving fullscreen or focusing another tile restores expected layers without changing unrelated window sizes.

## Two-axis canvas behavior

Canvas mode provides unbounded x/y group placement and panning. It is required in phase 8. New groups spawn adjacent to the active group in the current insertion direction; resolve overlap using a bounded local search, then allow explicit overlap if no slot fits. Groups retain real world coordinates. The viewport is independent of group arrangement.

Directional focus scores eligible candidates in the requested half-plane: angular deviation first, projected edge distance second, perpendicular distance third, and stable ID last. Provide a predictable alternative nearest-center policy and document ties. Hidden tabs are eligible through tab actions, not as coincident directional targets. Focus search lists all windows regardless of viewport.

Use a spatial index once profiling justifies it. Index conservative animated bounds and actual target bounds separately if needed. Only visible groups and an explicit prefetch margin produce render elements. Offscreen windows remain protocol objects and may retain client buffers; culling does not make their application memory disappear. Reclaim compositor thumbnails/import caches by LRU and fences.

Dragging a group uses a world-space grab offset, not a screen-space constant, so zoom changes remain stable. Snap to neighboring edges with a physical-pixel tolerance converted through scale/zoom. Release snapping when the pointer exceeds hysteresis. Edge pan has speed caps and pauses if focus/lock/gesture ownership changes.

## Switching modes and overview

Maintain `strip_state` and `canvas_state` per workspace. First entry to canvas seeds group positions from strip order. First return to strip sorts groups deterministically by x then y and restores saved widths when possible. Subsequent mode switches restore each mode's stored positions; newly added windows receive placement in both modes lazily. Remove stale IDs from both states on close.

Overview is a presentation transform with its own camera and zoom. It does not resize client buffers on every zoom frame. Selection/drag uses the inverse overview transform. Cap thumbnail resolution, update rate, and visible workspace count. Keep overlay/lock layers outside workspace zoom. Keyboard overview offers search, directional navigation, activate, move to workspace, and exit.

## Multi-output and recovery

No automatic strip overflow crosses output boundaries. `MoveWindowToOutput` transfers ownership explicitly, resolves scale/constraints, and updates focus atomically. When a monitor disappears, move its workspaces to a survivor while retaining preferred output identity. Match reconnections using connector plus EDID hints with collision handling; do not trust display names as unique. Do not yank a workspace back if the user explicitly reassigned it while disconnected.

If every output disappears, keep the server alive in an outputless state where feasible, stop presentation scheduling, retain windows, and accept new output creation. On reconnection clamp camera/zoom to valid finite values and restore discoverability. A recovery command centers on focused content; another lists every window with output/workspace metadata.

## Algorithm acceptance cases

Property-test unique membership, valid focus, finite geometry, nonnegative sizes, and invertible transforms. Replay 10,000 random open/close/move/resize/mode/hotplug commands. Include no outputs, zero usable area, oversized client minimums, negative coordinates, million-column synthetic models, closing mid-drag, fullscreen while switching output, and stale configure commits. Measure large synthetic layout models separately from real application counts.
