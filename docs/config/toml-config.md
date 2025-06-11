# TOML Configuration (CLIBuddy Fork)

CLIBuddy uses a simplified TOML-based configuration system for user-facing settings. This provides an easy-to-edit configuration file for the most commonly adjusted settings, while maintaining the powerful Lua configuration system internally for advanced features.

## Quick Start

CLIBuddy automatically creates a default configuration file when you first run it. The file is located at:

- **Linux:** `~/.config/clibuddy/config.toml`
- **macOS:** `~/Library/Application Support/CLIBuddy/config.toml`  
- **Windows:** `%APPDATA%\CLIBuddy\config.toml`

The default configuration looks like this:

```toml
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
window_close_confirmation = "never"  # "never", "always", or "auto"
```

## Configuration Reference

### [font]

Controls font settings.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `size` | Number | `11.0` | Font size in points |

### [appearance]

Controls visual appearance including colors and transparency.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `color_scheme` | String | `"Cobalt2"` | Built-in color scheme name |
| `background_color` | String | `"#0C0C37"` | Background color (hex format) |
| `foreground_color` | String | `"#FEFEFE"` | Text color (hex format) |
| `inactive_tab_bg_color` | String | `"#01010A"` | Inactive tab background color |
| `inactive_tab_fg_color` | String | `"#999999"` | Inactive tab text color |
| `selection_fg_color` | String | `"#F0F0F0"` | Selected text color |
| `selection_bg_color` | String | `"#6565AA"` | Selection background color |
| `scrollbar_color` | String | `"#BBBBBB"` | Scrollbar color |
| `opacity` | Number | `0.8` | Window opacity (0.0-1.0) |
| `blur` | Boolean | `true` | Enable background blur |

!!! note "Color Format"
    Colors should be specified in hex format with a `#` prefix, e.g., `"#FF0000"` for red.

!!! note "Opacity and Blur"
    - `opacity` controls the transparency of the window
    - `blur` enables background blur on supported platforms (macOS, Windows, some Linux desktop environments)

### [window]

Controls window geometry and behavior.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `initial_width` | Number | `130` | Initial window width in columns |
| `initial_height` | Number | `34` | Initial window height in rows |
| `hide_tab_bar_if_only_one_tab` | Boolean | `true` | Hide tab bar when only one tab is open |

### [border]

Controls window borders (macOS and Windows only).

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `window_border_color` | String | `"#AAAAAA"` | Window border color (hex format) |

!!! note "Platform Support"
    Window borders are only supported on macOS and Windows. This setting has no effect on Linux.

### [terminal]

Controls terminal behavior.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `scrollback_lines` | Number | `300000` | Number of lines to keep in scrollback buffer |

### [behavior]

Controls application behavior.

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `window_close_confirmation` | String | `"never"` | When to show close confirmation: `"never"`, `"always"`, or `"auto"` |

## Hot Reload

CLIBuddy automatically watches your TOML configuration file for changes. When you save changes to the file, they will be applied immediately without needing to restart the application.

## File Location Reference

The configuration file is stored in different locations depending on your operating system:

### Linux
- **File Path:** `~/.config/clibuddy/config.toml`
- **Full Path:** `/home/username/.config/clibuddy/config.toml`
- **XDG Override:** If `$XDG_CONFIG_HOME` is set, uses `$XDG_CONFIG_HOME/clibuddy/config.toml`

### macOS
- **File Path:** `~/Library/Application Support/CLIBuddy/config.toml`
- **Full Path:** `/Users/username/Library/Application Support/CLIBuddy/config.toml`

### Windows
- **File Path:** `%APPDATA%\CLIBuddy\config.toml`
- **Full Path:** `C:\Users\username\AppData\Roaming\CLIBuddy\config.toml`

## Advanced Configuration

For advanced configuration needs beyond what the TOML file provides, CLIBuddy uses an internal Lua configuration system. However, this is managed automatically by the system and should not be modified by users.

If you need advanced features like:
- Custom key bindings
- Complex color schemes
- SSH domains
- Multiplexer configuration
- Custom events and scripts

You may want to consider using the standard WezTerm distribution which provides full Lua configuration access.

## Migration from WezTerm

If you're migrating from standard WezTerm, you can convert your existing Lua configuration by extracting the relevant settings into the TOML format. Common migration mappings:

| WezTerm Lua | CLIBuddy TOML |
|-------------|---------------|
| `config.font_size = 12` | `[font]`<br>`size = 12.0` |
| `config.color_scheme = "Dracula"` | `[appearance]`<br>`color_scheme = "Dracula"` |
| `config.window_background_opacity = 0.9` | `[appearance]`<br>`opacity = 0.9` |
| `config.initial_cols = 120` | `[window]`<br>`initial_width = 120` |
| `config.scrollback_lines = 10000` | `[terminal]`<br>`scrollback_lines = 10000` |

## Troubleshooting

### Configuration Not Loading
1. Check that the file is saved in the correct location for your OS
2. Verify the TOML syntax is valid (use a TOML validator)
3. Check the terminal output for any error messages

### Changes Not Applied
1. Ensure the file is saved
2. CLIBuddy should automatically reload - if not, try restarting
3. Check for TOML syntax errors that might prevent loading

### Invalid Colors
- Colors must be in hex format with `#` prefix
- Use 6-digit hex codes (e.g., `#FF0000`, not `#F00`)
- Color names are not supported in TOML config

## Supported Color Schemes

CLIBuddy includes all the built-in WezTerm color schemes. Some popular options include:

- `Cobalt2` (default)
- `Dracula`
- `GitHub Dark`
- `One Dark`
- `Solarized Dark`
- `Tokyo Night`

For a complete list, see the [Color Schemes documentation](../colorschemes/index.md).