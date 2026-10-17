# Layout model implementation

`flux-layout` is a pure workspace model implemented from the FluxWM design. It has no Wayland, Lua, graphics, or device dependencies. This is incremental implementation evidence, not a phase 4 or phase 8 completion claim.

## Integration

Create `Layout::new(Size)` from the positive usable output dimensions, then insert mapped `WindowId` identities and validated `SizeConstraints`. `insert` currently explicitly activates the new window; the compositor must call it only after its activation policy permits this, or restore prior focus immediately while integrating a future insertion-policy API. Never insert untrusted protocol object IDs without the owning generational arena.

Use `placements()` for client target sizes. Its rectangles are in workspace world coordinates. Use `camera().rect_to_view(rect)` to build the scene in output-local logical coordinates. Apply the output usable-region offset, physical scale, output transform, surface offsets, and buffer conversions in the renderer. `visible_placements()` excludes offscreen content and hidden tabs. Offscreen members remain live; hidden tabs keep their allocated nonzero target size. `hit_test` accepts viewport-local logical coordinates; the compositor must additionally apply input-region and surface-tree tests.

Focus through `focus_window` or `focus_direction`. Strip directional movement crosses columns horizontally and visits stack/tab members vertically. Canvas directional movement searches all members, including distant windows, using angular deviation, forward center distance, perpendicular distance, and stable ID. Focus reveals content using minimum camera motion; oversized content aligns its leading edge. `join_column` moves a window without duplicating membership. `move_focused` reorders columns/members in strip mode and moves 32 logical units in canvas mode.

Stack allocation starts at each client's minimum height and distributes remaining height equally while respecting maxima. If minimums overflow, each member retains its minimum and focusing scrolls the internal column to reveal it. Tab mode presents only the active member. Column width policies support fixed logical widths and viewport fractions. Widths of unrelated columns are never reduced to fit the output. A client maximum can leave blank space within its column; maximum widths do not force other clients to violate their minima.

Canvas rectangles and camera are independent from strip state. New members seed from strip order or adjacent to the active canvas member. Mode switching retains both geometries. Canvas has independently movable x/y world coordinates, negative placement, independent camera zoom, and inverse picking. `rebase_canvas` moves every canvas member and its camera atomically. The caller must apply the same delta to any external animation endpoints, gesture anchors, spatial indices, and accumulated-origin record before presenting again.

## Input and capacity policy

Invalid, nonfinite, zero-size, conflicting min/max, and out-of-range inputs return typed errors. Empty usable regions retain the last valid layout. Logical sizes and coordinate magnitudes are bounded by `MAX_COORD` (one billion logical units). Strip mutations reject widths whose combined extent exceeds that bound, before changing state. This is a finite precision safeguard, not a hardware qualification limit or claim of infinite physical storage. Explicit rebasing supports long-range canvas navigation; the compositor still needs its checked accumulated-origin representation.

Membership is unique in column lists, constraints, canvas geometry, and focus history. Removing an active member chooses the latest live history entry, then a deterministic neighbor; closing the last member resets both cameras. `validate()` checks cross-representation membership, focus, active tabs, nonzero finite geometry, and cameras and is available to replay harnesses.

## Verification and remaining work

The crate tests independent-width scrolling with 20 members; oversized stack minimums and internal reachability; tabs and tab close; maximum-height redistribution; real negative/two-axis canvas positions and mode restoration; transform inversion and large-coordinate rebasing; rejected mutations; generational identity separation; deterministic spatial focus; and a 10,000-command replay that checks membership, focus, and geometry after every command.

Run `cargo test -p flux-layout` and `cargo clippy -p flux-layout --all-targets -- -D warnings` from the workspace root.

Remaining production work includes multi-workspace/output ownership and migration; floating/dialog layers; fullscreen/maximized/minimized state and backdrop policy; configure transactions and stale-buffer handling; compositor activation policy integration; group-level canvas stacks and overlap resolution; gestures and snap hysteresis; overview/search UI and accessibility; weighted stack heights; output fractional/physical shared-edge rounding; viewport resize compatibility with stale client buffers; cached/binary-search visibility and measured spatial-index thresholds. Current scene queries allocate and scan all members, so no O(log n + k) performance claim is made. Pure model tests do not establish real-client, GPU, output hotplug, or phase gate qualification.
