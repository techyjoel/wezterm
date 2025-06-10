-- Test configuration to enable OS window border with debug
local config = {}

config.window_decorations = "TITLE|RESIZE|MACOS_USE_BACKGROUND_COLOR_AS_TITLEBAR_COLOR"

config.use_fancy_tab_bar = true


-- Enable OS window border
config.window_frame = {
  os_window_border_enabled = true,
  os_window_border = {
    width = "1px",
    color = "#ff0000",
    radius = "10px"
  }
}

-- Minimal config for testing
config.font_size = 14.0
config.initial_cols = 80
config.initial_rows = 24

return config
