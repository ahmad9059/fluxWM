# Animation, input, and accessibility

## Motion model

FluxWM's animation engine is a design proposal informed by [Hyprland's curve concepts](https://wiki.hypr.land/configuring/core/animations/). It should feel responsive because input and motion remain continuous when interrupted, not because every transition is long or elaborate.

Animate camera position, tile presentation rectangles, workspace offsets, opacity, borders, and overview zoom. Keep target layout geometry separate from animated presentation geometry. Picking uses the visible presentation geometry; protocol configure requests use target client sizes. Closing snapshots are visual only.

Use monotonic timestamps and sample analytic curves at each presentation prediction. Never advance by a fixed fraction per rendered frame. A 60 Hz and 144 Hz output should reach the same state at the same elapsed time. Clamp excessive elapsed intervals after suspend; usually finish ordinary transitions rather than playing seconds of catch-up.

For a spring with position x, target q, mass m, damping d, and stiffness k:

`m*x'' + d*x' + k*(x-q) = 0`

Damping ratio is `d/(2*sqrt(k*m))`. Support underdamped, critically damped, and overdamped forms with stable analytic solutions; handle near-critical numerics carefully. On retarget, retain current position and velocity. Settle using both position and velocity tolerances expressed relative to physical-pixel scale. Stop scheduling once settled. Validate positive mass/stiffness, nonnegative damping, finite values, and a maximum lifetime.

For cubic Bézier easing, solve the x parameter for elapsed progress, then evaluate y. Use bounded Newton iteration with bisection fallback. Require x control points in [0,1] for monotonic time mapping; allow bounded y overshoot where appropriate. If duration is zero, apply the target immediately. Opacity is clamped to [0,1].

## Gesture state machine

States: idle → possible → compositor-owned or client-owned → active → settling/cancelled → idle. Ownership is decided once per gesture with thresholds and hysteresis. Never send the same active gesture simultaneously to a client and canvas navigation. Respect pointer constraints and shortcut inhibition policies.

During a compositor pan, camera position follows finger displacement in logical coordinates. On release estimate velocity from a bounded recent sample window; discard stale samples and cap velocity. Integrate deceleration or a friction curve, then snap to a suitable column/group if snapping is enabled. Keyboard focus changes cancel or retarget camera inertia. A lock, device removal, output removal, or gesture cancellation terminates ownership safely.

Suggested defaults, all provisional: three-finger horizontal pan for strip camera, three-finger vertical switch for workspaces, four-finger overview, and modifier plus pointer drag for canvas pan. In 2D canvas mode an explicit configured gesture owns both axes. Provide axis lock thresholds and configurable natural-scroll direction. Validate actual libinput gesture availability on hardware; nested backends may offer a smaller set. [libinput reference](https://wayland.freedesktop.org/libinput/doc/latest/).

## Keyboard and pointer

Use xkbcommon for keymaps, modifiers, compose, and layout groups. Distinguish physical keycode bindings from layout-dependent symbols. Define duplicate binding behavior at validation time. Track press ownership so a consumed press does not leave a stray release in a client; handle modifiers carefully across focus changes. Repeat policy belongs to the binding/client contract rather than blindly duplicating events.

Pointer routing traverses the current scene front to back, applying inverse transforms and clipping, then searches subsurfaces with correct offsets. A popup grab or compositor drag can override ordinary picking. Focus-follows-mouse is optional; default explicit focus avoids accidental changes while the camera pans under a stationary cursor. Relative pointer and pointer constraints require coherent lock/unlock and fullscreen behavior.

Touchscreen contacts track stable IDs, transforms, cancellation, and output mapping. Tablets need tool proximity, pressure, axes, pad buttons, and output mapping where supported. Start with a single seat, but prevent assumptions that device identity equals seat identity. Document whether additional seats are unsupported before exposing partial behavior.

## Accessibility requirements

Every operation has a keyboard action and discoverable name. Overview/search supports a visible focus indicator, high contrast, scaled text, and clear selection. Provide `reduced_motion` that resolves transitions immediately, avoids overshoot, and disables moving overview transitions. Optional magnification uses a separate accessible viewport transform with bounded zoom and a recovery shortcut.

Screen-reader operation needs integration research, not a checkbox from implementing AT-SPI in the compositor. Applications expose their own accessibility trees; FluxWM must coordinate focus, keyboard routing, and any required accessibility protocol/portal access. Phase 8 records the chosen stack, permission model, supported versions, and a real screen-reader demonstration. Do not infer compatibility from niri's support.

## Motion validation

Run deterministic fake-clock tests at 30/60/120/144/240 Hz, irregular intervals, and output refresh changes. Check duration independence, finite outputs, continuous retarget velocity, settling, and zero frames after quiescence. Visual tests cover opening during a pan, closing during overview, moving between differently scaled outputs, drag cancellation, and reduced-motion parity. Record a pointer-to-visible-response trace on physical hardware; screen recordings alone cannot establish input-to-photon latency.

## Numerical and UI additions

For overdamping, use stable real-root exponentials rather than an oscillatory formula with an invalid square root. The example spring (mass 1, stiffness 320, damping 36) is slightly overdamped, so omitting this branch would make the supplied configuration unsupported. Bound exponent calculations and handle near-critical cancellation, zero damping, zero duration, and large post-suspend intervals. Tests compare position and velocity, not merely eventual target equality.

Presentation and input use coherent scene snapshots; an animation's predicted next frame is not necessarily visible yet. Text UI requires shaping/bidi/fallback/glyph budgets and a semantic accessibility tree. See [integration contracts](16-integration-contracts.md). Touchpad gesture forwarding requires the selected pointer-gestures protocol integration; libinput event availability alone does not provide a client-facing protocol.
