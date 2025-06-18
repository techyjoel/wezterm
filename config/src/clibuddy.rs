//! CLiBuddy-specific configuration

use crate::RgbaColor;
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

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct RightSidebarConfig {
    #[dynamic(default = "default_right_sidebar_bg_color")]
    pub background_color: RgbaColor,
}

impl Default for RightSidebarConfig {
    fn default() -> Self {
        Self {
            background_color: default_right_sidebar_bg_color(),
        }
    }
}

#[derive(Debug, Clone, FromDynamic, ToDynamic, ConfigMeta)]
pub struct SidebarButtonConfig {
    /// Font size for sidebar button icons in points.
    /// If None, uses the window frame font size.
    pub icon_font_size: Option<f64>,
}

impl Default for SidebarButtonConfig {
    fn default() -> Self {
        Self {
            icon_font_size: None,
        }
    }
}

fn default_right_sidebar_bg_color() -> RgbaColor {
    // rgba(5, 5, 6, 1.0)
    RgbaColor::from((5u8, 5u8, 6u8))
}