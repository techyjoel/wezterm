//! CLiBuddy-specific configuration

use crate::{RgbaColor, TextStyle};
use wezterm_config_derive::ConfigMeta;
use wezterm_dynamic::{FromDynamic, ToDynamic};

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct ClibuddyConfig {
    #[dynamic(default)]
    pub left_sidebar: LeftSidebarConfig,

    #[dynamic(default)]
    pub right_sidebar: RightSidebarConfig,

    /// Shared button configuration for both sidebars
    #[dynamic(default)]
    pub sidebar_button: SidebarButtonConfig,
}

impl Default for ClibuddyConfig {
    fn default() -> Self {
        Self {
            left_sidebar: LeftSidebarConfig::default(),
            right_sidebar: RightSidebarConfig::default(),
            sidebar_button: SidebarButtonConfig::default(),
        }
    }
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct LeftSidebarConfig {
    // Placeholder for future left sidebar configuration
}

impl Default for LeftSidebarConfig {
    fn default() -> Self {
        Self {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromDynamic, ToDynamic)]
pub enum SidebarMode {
    /// Sidebar overlays on top of terminal content
    Overlay,
    /// Sidebar expands the window, terminal content shifts
    Expand,
}

impl Default for SidebarMode {
    fn default() -> Self {
        SidebarMode::Expand
    }
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct RightSidebarConfig {
    #[dynamic(default = "default_right_sidebar_bg_color")]
    pub background_color: RgbaColor,

    /// Width of the sidebar in pixels
    #[dynamic(default = "default_sidebar_width")]
    pub width: u16,

    /// Whether the sidebar is shown on startup
    #[dynamic(default = "default_show_on_startup")]
    pub show_on_startup: bool,

    /// Sidebar mode (Overlay or Expand)
    #[dynamic(default)]
    pub mode: SidebarMode,

    /// Font configuration for sidebar content
    #[dynamic(default)]
    pub fonts: SidebarFontConfig,
}

impl Default for RightSidebarConfig {
    fn default() -> Self {
        Self {
            background_color: default_right_sidebar_bg_color(),
            width: default_sidebar_width(),
            show_on_startup: default_show_on_startup(),
            mode: SidebarMode::default(),
            fonts: SidebarFontConfig::default(),
        }
    }
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct SidebarFontConfig {
    /// Font weight for body text ("Regular" or "Light")
    #[dynamic(default = "default_body_font_weight")]
    pub body_weight: String,

    /// Font size reduction for body text compared to headings (in points)
    #[dynamic(default = "default_font_size_reduction")]
    pub font_size_reduction: f64,

    /// Override font family (defaults to Roboto if None)
    pub font_family: Option<String>,
}

impl Default for SidebarFontConfig {
    fn default() -> Self {
        Self {
            body_weight: default_body_font_weight(),
            font_size_reduction: default_font_size_reduction(),
            font_family: None,
        }
    }
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct SidebarButtonConfig {
    /// Font size for sidebar button icons in points.
    /// If None, uses the window frame font size.
    pub icon_font_size: Option<f64>,

    /// Neon effect configuration for buttons
    #[dynamic(default)]
    pub neon: Option<NeonConfig>,

    /// Border width in pixels
    #[dynamic(default = "default_border_width")]
    pub border_width: f32,

    /// Corner radius for rounded buttons (not yet implemented)
    #[dynamic(default = "default_corner_radius")]
    pub corner_radius: f32,

    /// Separate configs for left and right buttons
    #[dynamic(default)]
    pub left_style: Option<ButtonStyleOverride>,

    #[dynamic(default)]
    pub right_style: Option<ButtonStyleOverride>,
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct NeonConfig {
    /// Primary neon color (e.g., "#00FFFF" for cyan)
    pub color: RgbaColor,

    /// Base color when unlit (e.g., "#0A0A0F" for dark blue-black)
    pub base_color: RgbaColor,

    /// Glow intensity from 0.0 to 1.0
    #[dynamic(default = "default_glow_intensity")]
    pub glow_intensity: f64,

    /// Number of glow layers (3-8 recommended)
    #[dynamic(default = "default_glow_layers")]
    pub glow_layers: u8,

    /// Glow radius in pixels
    #[dynamic(default = "default_glow_radius")]
    pub glow_radius: f32,
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct ButtonStyleOverride {
    /// Override neon configuration for this button
    pub neon: Option<NeonConfig>,
}

impl Default for SidebarButtonConfig {
    fn default() -> Self {
        Self {
            icon_font_size: None,
            neon: None,
            border_width: default_border_width(),
            corner_radius: default_corner_radius(),
            left_style: None,
            right_style: None,
        }
    }
}

impl Default for NeonConfig {
    fn default() -> Self {
        Self {
            color: RgbaColor::from((0u8, 255u8, 255u8)),     // Cyan
            base_color: RgbaColor::from((10u8, 10u8, 15u8)), // Dark blue-black
            glow_intensity: default_glow_intensity(),
            glow_layers: default_glow_layers(),
            glow_radius: default_glow_radius(),
        }
    }
}

impl Default for ButtonStyleOverride {
    fn default() -> Self {
        Self { neon: None }
    }
}

fn default_right_sidebar_bg_color() -> RgbaColor {
    // rgba(5, 5, 6, 1.0)
    RgbaColor::from((5u8, 5u8, 6u8))
}

fn default_sidebar_width() -> u16 {
    400
}

fn default_show_on_startup() -> bool {
    true // Right sidebar should be expanded at startup
}

fn default_border_width() -> f32 {
    2.0
}

fn default_corner_radius() -> f32 {
    8.0
}

fn default_glow_intensity() -> f64 {
    0.8
}

fn default_glow_layers() -> u8 {
    6
}

fn default_glow_radius() -> f32 {
    20.0
}

fn default_body_font_weight() -> String {
    "Regular".to_string()
}

fn default_font_size_reduction() -> f64 {
    1.0
}
