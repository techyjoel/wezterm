-- clibuddy/wezterm.lua - Built-in configuration for CLIBuddy fork
local wezterm = require 'wezterm'
local config = wezterm.config_builder()

-- Function to get cross-platform user config path
local function get_user_config_path()
    local home = wezterm.home_dir
    if wezterm.target_triple:find('darwin') then
        -- macOS
        return home .. '/Library/Application Support/CLIBuddy/config.toml'
    elseif wezterm.target_triple:find('windows') then
        -- Windows
        return os.getenv('APPDATA') .. '\\CLIBuddy\\config.toml'
    else
        -- Linux/Unix
        local xdg_config = os.getenv('XDG_CONFIG_HOME') or (home .. '/.config')
        return xdg_config .. '/clibuddy/config.toml'
    end
end

-- Function to create default config file
local function create_default_config(path)
    local default_toml = [[
version = "1.0"

[font]
family = "JetBrains Mono"
size = 12.0

[appearance]
theme = "dark"
color_scheme = "Tomorrow Night"
opacity = 1.0
blur = false

[window]
decorations = "full"
startup_mode = "windowed"

[window.padding]
left = 8
right = 8
top = 8
bottom = 8

[terminal]
scrollback_lines = 3000
cursor_style = "block"
cursor_blink = true

[behavior]
close_behavior = "close_on_clean_exit"
confirm_close = true
]]
    
    -- Create directory if needed
    local dir = path:match("(.+)[/\\][^/\\]*$")
    if dir then
        -- Try both Unix and Windows style directory creation
        os.execute('mkdir -p "' .. dir .. '" 2>/dev/null')  -- Unix
        os.execute('mkdir "' .. dir .. '" 2>NUL')           -- Windows
    end
    
    local file = io.open(path, "w")
    if file then
        file:write(default_toml)
        file:close()
        wezterm.log_info("Created default config file at " .. path)
        return true
    end
    return false
end

-- Function to load user config
local function load_user_config()
    local config_path = get_user_config_path()
    wezterm.log_info("Looking for user config at: " .. config_path)
    
    local file = io.open(config_path, "r")
    if not file then
        -- Try to create default config
        wezterm.log_info("No user config found, creating default...")
        if create_default_config(config_path) then
            file = io.open(config_path, "r")
        end
        if not file then
            wezterm.log_warn("Could not create or read config file")
            return {}
        end
    end
    
    local content = file:read("*all")
    file:close()
    
    -- Parse TOML content
    local success, parsed = pcall(wezterm.serde.toml_decode, content)
    if success then
        wezterm.log_info("Successfully loaded user config")
        return parsed
    else
        wezterm.log_error("Failed to parse TOML config: " .. tostring(parsed))
        return {}
    end
end

-- Load user configuration
local user_config = load_user_config()

-- Apply font settings
if user_config.font then
    if user_config.font.family then
        config.font = wezterm.font(user_config.font.family)
    end
    if user_config.font.size then
        config.font_size = user_config.font.size
    end
end

-- Apply appearance settings
if user_config.appearance then
    if user_config.appearance.color_scheme then
        config.color_scheme = user_config.appearance.color_scheme
    end
    if user_config.appearance.opacity then
        config.window_background_opacity = user_config.appearance.opacity
    end
    if user_config.appearance.blur ~= nil then
        -- Platform-specific blur settings
        if wezterm.target_triple:find('darwin') then
            config.macos_window_background_blur = user_config.appearance.blur and 10 or 0
        elseif wezterm.target_triple:find('windows') then
            config.win32_system_backdrop = user_config.appearance.blur and 'Acrylic' or 'None'
        end
    end
end

-- Apply window settings
if user_config.window then
    if user_config.window.padding then
        config.window_padding = user_config.window.padding
    end
    if user_config.window.decorations then
        config.window_decorations = user_config.window.decorations
    end
    if user_config.window.startup_mode then
        -- Handle startup mode (windowed, maximized, fullscreen)
        if user_config.window.startup_mode == "maximized" then
            config.initial_rows = 999
            config.initial_cols = 999
        elseif user_config.window.startup_mode == "fullscreen" then
            -- This would need to be handled differently per platform
            wezterm.log_info("Fullscreen startup mode requested")
        end
    end
end

-- Apply terminal settings
if user_config.terminal then
    if user_config.terminal.scrollback_lines then
        config.scrollback_lines = user_config.terminal.scrollback_lines
    end
    if user_config.terminal.cursor_style then
        config.default_cursor_style = user_config.terminal.cursor_style:gsub("^%l", string.upper)
    end
    if user_config.terminal.cursor_blink ~= nil then
        config.cursor_blink_rate = user_config.terminal.cursor_blink and 800 or 0
    end
end

-- Apply behavior settings
if user_config.behavior then
    if user_config.behavior.close_behavior then
        -- Map close_behavior to wezterm exit_behavior
        if user_config.behavior.close_behavior == "close_on_clean_exit" then
            config.exit_behavior = "CloseOnCleanExit"
        elseif user_config.behavior.close_behavior == "close_always" then
            config.exit_behavior = "Close"
        elseif user_config.behavior.close_behavior == "hold_on_error" then
            config.exit_behavior = "Hold"
        end
    end
    if user_config.behavior.confirm_close ~= nil then
        config.window_close_confirmation = user_config.behavior.confirm_close and 'AlwaysPrompt' or 'NeverPrompt'
    end
end

-- Set some built-in defaults that are good for CLIBuddy
config.check_for_updates = false
config.automatically_reload_config = true
config.enable_tab_bar = true
config.use_fancy_tab_bar = false
config.hide_tab_bar_if_only_one_tab = true

-- Add file watching for the user config to enable hot reload
wezterm.add_to_config_reload_watch_list(get_user_config_path())

return config