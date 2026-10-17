-- FluxWM schema v1 example; helper schema validation exists, compositor integration is incomplete.
-- Pure Lua 5.4: evaluate once, return a table, compile actions into Rust.
local mod = "Super"
return {
  schema_version = 1,
  layout = {
    mode = "strip", -- "canvas" enables independent x/y camera movement
    gaps = 12,
    outer_gap = 12,
    width_presets = { { fraction = 0.33 }, { fraction = 0.5 }, { fraction = 0.66 } },
    default_width = { preset = 2 },
    focus_margin = 24,
    canvas = { snap = true, snap_distance_physical_px = 12, insertion = "right" },
  },
  input = {
    keymap = { layout = "us", options = "caps:escape" },
    pointer = { focus_follows_mouse = false, acceleration = 0.0 },
    touchpad = { tap = true, natural_scroll = true },
    gestures = { pan_fingers = 3, overview_fingers = 4 },
  },
  outputs = {
    { match = { any = true }, scale = 1.0, adaptive_sync = "auto" },
    -- Output rules run in order; later matching fields override earlier ones.
    { match = { connector = "eDP-1" }, scale = 1.25 },
  },
  animations = {
    enabled = true,
    reduced_motion = false,
    curves = {
      camera_spring = { type = "spring", mass = 1, stiffness = 320, damping = 36 },
      gentle = { type = "bezier", points = { 0.2, 0.0, 0.2, 1.0 } },
    },
    camera = { curve = "camera_spring" },
    window = { curve = "gentle", duration_ms = 180 },
    fade = { curve = "gentle", duration_ms = 120 },
  },
  decoration = {
    border = { width = 2, active = "#89B4FA", inactive = "#45475A" },
    corner_radius = 10,
    shadow = { enabled = true, radius = 18, opacity = 0.25 },
    blur = { enabled = false, radius = 8, passes = 2 },
  },
  bindings = {
    { key = { sym = mod .. "+Return" }, action = { name = "spawn", argv = { "foot" } } },
    { key = { sym = mod .. "+h" }, action = { name = "focus", direction = "left" } },
    { key = { sym = mod .. "+l" }, action = { name = "focus", direction = "right" } },
    { key = { sym = mod .. "+j" }, action = { name = "focus", direction = "down" } },
    { key = { sym = mod .. "+k" }, action = { name = "focus", direction = "up" } },
    { key = { sym = mod .. "+Shift+h" }, action = { name = "move", direction = "left" } },
    { key = { sym = mod .. "+q" }, action = { name = "close-window" } },
    { key = { sym = mod .. "+f" }, action = { name = "fullscreen", toggle = true } },
    { key = { sym = mod .. "+space" }, action = { name = "overview", toggle = true } },
    { key = { sym = mod .. "+c" }, action = { name = "workspace-mode", mode = "canvas" } },
    { key = { sym = mod .. "+s" }, action = { name = "workspace-mode", mode = "strip" } },
    { key = { sym = mod .. "+r" }, action = { name = "reload-config" } },
    { key = { sym = mod .. "+Escape" }, action = { name = "lock" } },
  },
  rules = {
    { match = { app_id = "^org[.]example[.]Settings$" }, set = { floating = true } },
    { match = { is_dialog = true }, set = { floating = true, center_on_parent = true } },
    { match = { app_id = "^org[.]example[.]Secrets$" }, set = { capture = "deny" } },
  },
  -- These executables are companion dependencies to install separately.
  autostart = {
    { argv = { "waybar" }, restart_on_change = false },
    { argv = { "mako" }, restart_on_change = false },
  },
  security = { portal_capture = "consent", ipc = "same-user" },
}
