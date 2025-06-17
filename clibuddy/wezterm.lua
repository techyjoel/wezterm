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
scrollbar_color = "#AAAAAA"
opacity = 0.8   # 0-1 range
blur = true

[window]
initial_width = 130    # columns
initial_height = 34    # rows
hide_tab_bar_if_only_one_tab = true

[border]
# Window border only works on MacOS and Windows
window_border_color= "#999999"

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

-- Function to adjust_intensity( color_string [, intensify [, change_amt ]] )
-- • color       –  "#rgb", "#rgba", "#rrggbb", "#rrggbbaa" (hash optional)
--                  or "rgba(r,g,b,a)"    ─ (case-insensitive, spaces ok)
-- • intensify    –  false / nil  → SOFTEN  (pull toward mid-grey)
--                  true          → INTENSIFY (push away from mid-grey)
-- • change_amt   –  absolute change per channel (0-255).  Default = 25.
--
-- EXTRA RULE: if a channel cannot move by ≥ change_amt/2 because of
--             clamping (0 or 255), its direction is flipped so it *can*.

local function adjust_intensity(col, intensify, change_amt)
    -----------------------------------------------------------------------
    -- helpers ------------------------------------------------------------
    local function parse(s)
        s = s:match("^%s*(.-)%s*$")                      -- trim
        -- ─── hex (#rgb/#rgba/#rrggbb/#rrggbbaa) ───────────────────────
        local hash, hex = s:match("^(#?)([%x]+)$")
        if hex and (#hex == 3 or #hex == 4 or #hex == 6 or #hex == 8) then
            if #hex == 3 or #hex == 4 then hex = hex:gsub("(.)", "%1%1") end
            local r = tonumber(hex:sub(1,2),16)
            local g = tonumber(hex:sub(3,4),16)
            local b = tonumber(hex:sub(5,6),16)
            local a = (#hex == 8) and tonumber(hex:sub(7,8),16) or nil
            return r,g,b, a and a/255 or 1, "hex", hash, a and hex:sub(7,8)
        end
        -- ─── rgba(...) string ─────────────────────────────────────────
        local r,g,b,a = s:lower():match(
            "^rgba%(%s*([%d%.]+)%s*,%s*([%d%.]+)%s*,%s*([%d%.]+)%s*,%s*([%d%.]+)%s*%)$")
        if r then return tonumber(r),tonumber(g),tonumber(b),tonumber(a),"rgba" end
    end

    local function emit(r,g,b,a,kind,hash,hex_a)
        if not kind then return col end                  -- invalid → unchanged
        if kind == "hex" then
            local out = string.format("%02x%02x%02x", r, g, b)
            if hex_a then out = out .. hex_a end
            return hash .. out
        end
        return ("rgba(%d,%d,%d,%s)"):format(r, g, b, tostring(a))
    end
    -----------------------------------------------------------------------

    local r,g,b,a,kind,hash,hex_a = parse(col)
    if not kind then return col end                      -- give up on bad input

    change_amt      = math.max(0, math.min(255, change_amt or 25))
    local avg       = (r + g + b) / 3
    local brighten  = (avg < 128) ~= (not not intensify) -- XOR: see table below
    local delta     = brighten and  change_amt or -change_amt
    local half_step = change_amt / 2

    local function shift(v)
        local nv   = v + delta
        local clamped = math.max(0, math.min(255, nv))
        if math.abs(clamped - v) < half_step then        -- moved < ½ target?
            nv       = v - delta                         -- flip direction
            clamped  = math.max(0, math.min(255, nv))
        end
        return clamped
    end

    r, g, b = shift(r), shift(g), shift(b)
    return emit(r, g, b, a, kind, hash, hex_a)
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
                fg_color = adjust_intensity(cust_fg_color),
                intensity = 'Normal',
                underline = 'None',
                italic = false,
                strikethrough = false,
            },
            inactive_tab = {
                bg_color = cust_inactive_tab_bg_color,
                fg_color = user_config.appearance.inactive_tab_fg_color or '#999999',
            },
            inactive_tab_hover = {
                bg_color = adjust_intensity(cust_inactive_tab_bg_color, true),
                fg_color = user_config.appearance.inactive_tab_fg_color or '#999999',
            },
            inactive_tab_edge = cust_inactive_tab_bg_color,
            new_tab = {
                bg_color = adjust_intensity(cust_inactive_tab_bg_color, true),
                fg_color = user_config.appearance.inactive_tab_fg_color or '#999999',
            },
            new_tab_hover = {
                bg_color = adjust_intensity(cust_inactive_tab_bg_color, true, 50),
                fg_color = user_config.appearance.inactive_tab_fg_color or '#999999',
            },
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
    left = 12,
    right = 22,
    top = 4,
    bottom = 8,
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

-- CLIBuddy Sidebar Configuration (stored separately from main config)
-- This will be read by our Rust code directly, not by WezTerm's config system
local clibuddy_config = {
    -- Left sidebar settings (settings/config sidebar)
    left_sidebar = {
        enabled = true,
        width = 350,
        show_on_startup = false,
        animation_duration_ms = 200,
        -- Colors  
        background_color = 'rgba(42, 42, 42, 1.0)',  -- Opaque dark gray
        border_color = 'rgba(64, 64, 64, 1.0)',
        -- Toggle button
        button = {
            visible = true,
            background_color = 'rgba(32, 32, 40, 0.8)',
            hover_color = 'rgba(50, 50, 60, 0.9)',
            icon_color = 'rgba(200, 200, 200, 1.0)',
        }
    },
    
    -- Right sidebar settings (AI assistant)
    right_sidebar = {
        enabled = true,
        width = 400,
        show_on_startup = true,  -- Set to true to show AI sidebar by default
        animation_duration_ms = 200,
        -- Colors
        background_color = 'rgba(5, 5, 6, 1.0)',
        border_color = 'rgba(64, 64, 64, 1.0)',
        -- Toggle button
        button = {
            visible = true,
            background_color = 'rgba(32, 32, 40, 0.8)',
            hover_color = 'rgba(50, 50, 60, 0.9)',
            icon_color = 'rgba(32, 128, 255, 1.0)',  -- Blue for AI
            position_x = 'right-10',  -- Distance from right edge
            position_y = 'top+10',    -- Distance from top
            size = 40,
        },
        -- AI-specific settings
        ai = {
            -- Status chip colors
            status_colors = {
                idle = 'rgba(100, 100, 100, 1.0)',
                thinking = 'rgba(255, 200, 0, 1.0)',
                gathering_data = 'rgba(32, 200, 255, 1.0)',
                needs_approval = 'rgba(255, 100, 100, 1.0)',
            },
            -- Component colors
            current_goal_bg = 'rgba(20, 40, 60, 0.3)',
            current_suggestion_bg = 'rgba(40, 20, 60, 0.3)',
            chat_input_bg = 'rgba(10, 10, 10, 0.5)',
            -- Activity log colors
            activity_log = {
                command_color = 'rgba(100, 200, 100, 1.0)',
                chat_user_color = 'rgba(200, 200, 200, 1.0)',
                chat_ai_color = 'rgba(100, 150, 255, 1.0)',
                suggestion_color = 'rgba(255, 200, 100, 1.0)',
                goal_color = 'rgba(200, 100, 255, 1.0)',
            }
        }
    },
    
    -- Form component colors (shared by both sidebars)
    forms = {
        text_input = {
            bg_color = 'rgba(5, 5, 5, 1.0)',
            border_color = 'rgba(64, 64, 64, 1.0)',
            border_color_focused = 'rgba(32, 128, 255, 1.0)',
            border_color_error = 'rgba(204, 51, 51, 1.0)',
            text_color = 'rgba(230, 230, 230, 1.0)',
            placeholder_color = 'rgba(128, 128, 128, 1.0)',
        },
        button = {
            primary = {
                bg_color = 'rgba(32, 128, 255, 1.0)',
                text_color = 'rgba(255, 255, 255, 1.0)',
                hover_color = 'rgba(40, 140, 255, 0.9)',
            },
            secondary = {
                bg_color = 'rgba(5, 5, 5, 1.0)',
                text_color = 'rgba(230, 230, 230, 1.0)',
                border_color = 'rgba(102, 102, 102, 1.0)',
                hover_bg_color = 'rgba(51, 51, 51, 1.0)',
            },
            danger = {
                bg_color = 'rgba(204, 51, 51, 1.0)',
                text_color = 'rgba(255, 255, 255, 1.0)',
                hover_color = 'rgba(220, 60, 60, 0.9)',
            },
        },
        toggle = {
            on_color = 'rgba(32, 204, 32, 1.0)',
            off_color = 'rgba(102, 102, 102, 1.0)',
        },
        dropdown = {
            bg_color = 'rgba(5, 5, 5, 1.0)',
            border_color = 'rgba(64, 64, 64, 1.0)',
            hover_color = 'rgba(32, 128, 255, 1.0)',
            selected_bg_color = 'rgba(32, 128, 255, 1.0)',
        },
        slider = {
            track_color = 'rgba(51, 51, 51, 1.0)',
            fill_color = 'rgba(32, 128, 255, 1.0)',
        },
    },
}

-- Export CLIBuddy config for our Rust code to access
wezterm.GLOBAL.clibuddy_config = clibuddy_config

-- Finally, return the configuration to wezterm
return config