-- Runnable nested-runtime subset. Other schema fields keep defaults but are
-- explicitly reported unsupported by config.effective. Requires the tested US
-- startup seat map; keymap replacement and device/output settings remain open.
-- Install foot separately, or replace its argv with an installed Wayland client.
return {
  schema_version = 1,
  layout = { gaps = 12, mode = "strip" },
  bindings = {
    { key = { sym = "Alt+Return" }, action = { name = "spawn", argv = { "foot" } } },
    { key = { sym = "Alt+r" }, action = { name = "reload-config" } },
    { key = { sym = "Alt+q" }, action = { name = "close-window" } },
    { key = { sym = "Alt+Shift+q" }, action = { name = "quit" } },
    { key = { sym = "Alt+h" }, action = { name = "focus", direction = "left" } },
    { key = { sym = "Alt+j" }, action = { name = "focus", direction = "down" } },
    { key = { sym = "Alt+k" }, action = { name = "focus", direction = "up" } },
    { key = { sym = "Alt+l" }, action = { name = "focus", direction = "right" } },
    { key = { sym = "Alt+Shift+h" }, action = { name = "move", direction = "left" } },
    { key = { sym = "Alt+Shift+j" }, action = { name = "move", direction = "down" } },
    { key = { sym = "Alt+Shift+k" }, action = { name = "move", direction = "up" } },
    { key = { sym = "Alt+Shift+l" }, action = { name = "move", direction = "right" } },
    { key = { sym = "Alt+c" }, action = { name = "workspace-mode", mode = "canvas" } },
    { key = { sym = "Alt+s" }, action = { name = "workspace-mode", mode = "strip" } },
    { key = { sym = "Alt+minus" }, action = { name = "resize", delta = -48 } },
    { key = { sym = "Alt+Shift+equal" }, action = { name = "resize", delta = 48 } },
  },
}
