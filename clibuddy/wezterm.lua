-- clibuddy/wezterm.lua - Built-in configuration for CLIBuddy fork

-- Pull in the wezterm API
local wezterm = require 'wezterm'

-- Determine platform
local is_darwin <const> = wezterm.target_triple:find("darwin") ~= nil
local is_linux <const> = wezterm.target_triple:find("linux") ~= nil
local is_windows <const> = wezterm.target_triple:find("windows") ~= nil

local config = wezterm.config_builder()

-- Function to get cross-platform user config path
local function get_user_config_path()
    local home = wezterm.home_dir
    if is_darwin then
        return home .. '/Library/Application Support/CLIBuddy/config.toml'
    elseif is_windows then
        return os.getenv('APPDATA') .. '\\CLIBuddy\\config.toml'
    else
        local xdg_config = os.getenv('XDG_CONFIG_HOME') or (home .. '/.config')
        return xdg_config .. '/clibuddy/config.toml'
    end
end

-- Function to create default config file
local function create_default_config(path)
    local default_toml = [[
version = "1.0"

[font]
size = 11.0

[appearance]
color_scheme = "Cobalt2"
background_color = "#0C0C37"
foreground_color = "#FEFEFE"
inactive_tab_bg_color = "#01010A"
inactive_tab_fg_color = "#999999"
selection_fg_color = "#F0F0F0"
selection_bg_color = "#6565AA"
scrollbar_color = "#BBBBBB"
opacity = 0.8   # 0-1 range
blur = true

[window]
initial_width = 130    # columns
initial_height = 34    # rows
hide_tab_bar_if_only_one_tab = true

[border]
# Window border only works on MacOS and Windows
window_border_color= "#AAAAAA"

[terminal]
scrollback_lines = 300000

[behavior]
window_close_confirmation = "never"
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

-- Function to convert hex color to rgba with specified opacity
local function hex_to_rgba(hex_color, opacity)
    if not hex_color or not hex_color:match("^#%x%x%x%x%x%x$") then
        return hex_color -- Return original if not valid hex
    end
    
    local r = tonumber(hex_color:sub(2, 3), 16)
    local g = tonumber(hex_color:sub(4, 5), 16)
    local b = tonumber(hex_color:sub(6, 7), 16)
    
    return string.format("rgba(%d,%d,%d,%.1f)", r, g, b, opacity)
end

-- Function to make a color 10% less intense (brighter or darker)
local function soften_intensity(hex)
    local original  = hex
    local has_hash  = hex:sub(1, 1) == "#"
    hex             = hex:gsub("^#", "")

    -- expand shorthand (#abc ⇒ #aabbcc)
    if #hex == 3 then
        hex = hex:gsub("(.)", "%1%1")
    end
    if #hex ~= 6 or not hex:match("^[0-9a-fA-F]+$") then
        return original
    end

    local r   = tonumber(hex:sub(1, 2), 16)
    local g   = tonumber(hex:sub(3, 4), 16)
    local b   = tonumber(hex:sub(5, 6), 16)
    local avg = (r + g + b) / 3

    local function shift(v, lighten)
        return lighten
            and math.min(255, math.floor(v + 0.1 * (255 - v) + 0.5)) -- lighten 10 %
            or  math.max(0,   math.floor(v - 0.1 * v + 0.5))         -- darken 10 %
    end

    local lighten = avg < 128
    r, g, b       = shift(r, lighten), shift(g, lighten), shift(b, lighten)

    local result = string.format("%02x%02x%02x", r, g, b)
    return has_hash and ("#" .. result) or result
end


-- Load user configuration
local user_config = load_user_config()

-- Hardcoded font settings
-- Try to get fonts to look a bit bolder
config.font = wezterm.font('JetBrains Mono', { weight = 'Medium' })
config.font_size = user_config.font and user_config.font.size or 11.0

-- Color variables with defaults
local cust_fg_color = '#FEFEFE'
local cust_bg_color = 'rgba(20,20,20,0.8)'
local cust_bg_color_tab_bar = 'rgba(20,20,20,0.8)'
local cust_inactive_tab_bg_color = 'rgba(4,4,4,0.9)'

-- Get opacity from user config
local opacity = 0.8
if user_config.appearance and user_config.appearance.opacity then
    opacity = user_config.appearance.opacity
end

-- Apply appearance settings
if user_config.appearance then
    if user_config.appearance.color_scheme then
        config.color_scheme = user_config.appearance.color_scheme
    end
    if user_config.appearance.background_color then
        -- Convert hex to rgba with user-specified opacity
        cust_bg_color = hex_to_rgba(user_config.appearance.background_color, opacity)
        cust_bg_color_tab_bar = cust_bg_color
    end
    if user_config.appearance.foreground_color then
        cust_fg_color = user_config.appearance.foreground_color
    end
    if user_config.appearance.inactive_tab_bg_color then
        -- Convert hex to rgba with fixed 0.9 opacity for now until it's fixed
        cust_inactive_tab_bg_color = hex_to_rgba(user_config.appearance.inactive_tab_bg_color, 0.9)
    end
    
    -- Configure colors table
    config.colors = {
        foreground = cust_fg_color,
        background = cust_bg_color,
        selection_fg = user_config.appearance.selection_fg_color or '#F0F0F0',
        selection_bg = user_config.appearance.selection_bg_color or '#333333',
        scrollbar_thumb = user_config.appearance.scrollbar_color or '#BBBBBB',
        tab_bar = {
            background = cust_bg_color_tab_bar,
            active_tab = {
                bg_color = cust_bg_color,
                fg_color = soften_intensity(cust_fg_color),
                intensity = 'Normal',
                underline = 'None',
                italic = false,
                strikethrough = false,
            },
            inactive_tab = {
                bg_color = cust_inactive_tab_bg_color,
                fg_color = user_config.appearance.inactive_tab_fg_color or '#999999',
            },
            inactive_tab_edge = cust_inactive_tab_bg_color,
        },
    }
    
    -- Window opacity
    -- Currently set to this for things to work right (at least on MacOS)
    config.window_background_opacity = 0.999
    
    -- Blur settings
    if user_config.appearance.blur then
        config.macos_window_background_blur = 20
        config.win32_system_backdrop = 'Acrylic'
        config.kde_window_background_blur = true
    end
end

config.window_frame = {
    active_titlebar_bg = cust_bg_color_tab_bar,
    inactive_titlebar_bg = cust_bg_color_tab_bar,
}

config.inactive_pane_hsb = {
    saturation = 0.9,
    brightness = 0.6,
}

-- Hardcoded window settings
config.use_fancy_tab_bar = true
config.window_decorations = "TITLE|RESIZE|MACOS_USE_BACKGROUND_COLOR_AS_TITLEBAR_COLOR"
config.hide_tab_bar_if_only_one_tab = user_config.window and user_config.window.hide_tab_bar_if_only_one_tab or true
config.initial_cols = user_config.window and user_config.window.initial_width or 130
config.initial_rows = user_config.window and user_config.window.initial_height or 34
config.window_padding = {
    left = 10,
    right = 22,
    top = 4,
    bottom = 6,
}

-- OS window border (MacOS and Windows only)
if (is_darwin or is_windows) and user_config.border and user_config.border.window_border_color then
    config.window_frame.os_window_border_enabled = true
    config.window_frame.os_window_border = {
        width = "1px",
        color = user_config.border.window_border_color,
        -- May need to be adjusted for OSX version
        radius = "10px",
    }
end

-- Animation settings
config.max_fps = 120
config.animation_fps = 10

-- Terminal settings
config.scrollback_lines = user_config.terminal and user_config.terminal.scrollback_lines or 300000
config.enable_scroll_bar = true

-- Behavior settings
if user_config.behavior and user_config.behavior.window_close_confirmation == 'never' then
    config.window_close_confirmation = 'NeverPrompt'
elseif user_config.behavior and user_config.behavior.window_close_confirmation == 'always' then
    config.window_close_confirmation = 'AlwaysPrompt'
else
    config.window_close_confirmation = 'NeverPrompt'  -- Default to never prompt
end

config.automatically_reload_config = true
-- Should enable this in the future
config.check_for_updates = false


-- Advanced tab title formatting logic
local HOME = os.getenv('HOME') or ''
local LOCALHOST = wezterm.hostname()
local boring = {bash=1,zsh=1,fish=1,sh=1,dash=1,ksh=1,nu=1,nushell=1,ssh=1}
local function first_word(s) return (s or ''):match('^%S+') or '' end
local function glue(a,b) return (a=='' and b) or (b=='' and a) or (a..' | '..b) end
local function dirname(p) return (p or ''):gsub('/+$',''):match('([^/]+)$') or '' end

-- replace /home/username → ~   and   strip HOME for local paths
local function homify(path, user, remote)
    if remote and user ~= '' then
        path = path:gsub('^/home/'..user, '~')
        path = path:gsub('^/Users/'..user, '~')
    elseif not remote and HOME ~= '' then
        path = path:gsub('^'..HOME, '~')
    end
    return path
end

local MIN_TAB_WIDTH = 20
config.tab_max_width = 80

local function pad_center_dynamic(title, max_width)
    local text_w = wezterm.column_width(title)
    local target = MIN_TAB_WIDTH
    
    if text_w >= target then
        return title
    end
    
    local total_pad = target - text_w
    local left_pad = math.floor(total_pad / 2)
    local right_pad = total_pad - left_pad
    return string.rep(' ', left_pad) .. title .. string.rep(' ', right_pad)
end

local function make_title(tab)
    local p = tab.active_pane
    local uv = p.user_vars or {}
    
    local cmd = uv.WEZTERM_PROG or ''
    if cmd == '' then cmd = (p.foreground_process_name or ''):gsub('.*/','') end
    if boring[first_word(cmd)] then cmd = '' end
    
    local host,user = uv.WEZTERM_HOST or '', uv.WEZTERM_USER or ''
    local cwd_uri = p.current_working_dir and tostring(p.current_working_dir) or ''
    local url = (cwd_uri ~= '') and wezterm.url.parse(cwd_uri) or nil
    if (host=='' or user=='') and url and url.scheme=='ssh' then
        host,user = url.host or '', url.user or ''
    end
    local remote = (host ~= '' and host ~= LOCALHOST)
    if remote then host = host:match('^[^.]+') or host end
    
    local dir = url and url.path and homify(url.path,user,remote) or ''
    dir = dirname(dir)
    
    local head = remote and string.format('%s@%s: %s', user, host, dir) or dir
    return glue(head, cmd) ~= '' and glue(head, cmd) or p.title
end

-- TAB title
wezterm.on('format-tab-title', function(tab, tabs, panes, config, hover, max_width)
    local raw = make_title(tab)
    local centered = pad_center_dynamic(raw, max_width)
    return {
        { Text = " " .. centered .. " " },
    }
end)

-- WINDOW title (active tab drives the window title)
wezterm.on('format-window-title', function(tab, pane, tabs, panes, config)
    return make_title(tab)
end)

-- Add file watching for the user config to enable hot reload
wezterm.add_to_config_reload_watch_list(get_user_config_path())

-- Finally, return the configuration to wezterm
return config