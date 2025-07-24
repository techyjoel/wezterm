//! Sidebar framework for WezTerm
//!
//! This module provides the infrastructure for creating sidebars in WezTerm,
//! including:
//! - Base sidebar traits and configuration
//! - Font management for sidebar rendering
//! - Animation coordinator for smooth transitions
//! - Sidebar manager for orchestrating multiple sidebars
//! - Component library for building sidebar UIs
//!
//! Sidebars can be positioned on the left or right, support smooth animations,
//! and integrate with the terminal window's event and rendering systems.

use crate::termwindow::box_model::Element;
use anyhow::Result;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use termwiz::input::KeyCode;
use wezterm_font::LoadedFont;
use wezterm_term::KeyModifiers;
use window::MouseEvent;
// Widget traits will be implemented differently without termwiz widgets

/// Bundle of fonts for rendering sidebar content
#[derive(Clone)]
pub struct SidebarFonts {
    /// Font for headings (e.g., "CLiBuddy AI", "Current Goal")
    pub heading: Rc<LoadedFont>,
    /// Font for body text (status chips, filter chips, content)
    pub body: Rc<LoadedFont>,
    /// Bold variant of body font for markdown emphasis
    pub body_bold: Option<Rc<LoadedFont>>,
    /// Italic variant of body font for markdown emphasis
    pub body_italic: Option<Rc<LoadedFont>>,
    /// Bold-italic variant of body font for markdown emphasis
    pub body_bold_italic: Option<Rc<LoadedFont>>,
    /// Font for code blocks in markdown
    pub code: Rc<LoadedFont>,
    /// Bold variant of code font (kept for future use, not used in code blocks)
    pub code_bold: Option<Rc<LoadedFont>>,
    /// Italic variant of code font (kept for future use, not used in code blocks)
    pub code_italic: Option<Rc<LoadedFont>>,
    /// Bold-italic variant of code font (kept for future use, not used in code blocks)
    pub code_bold_italic: Option<Rc<LoadedFont>>,
    /// Line height multiplier for code blocks
    pub code_line_height: f64,
    /// Bottom margin between logical lines in code blocks (in pixels)
    pub code_line_margin: f64,
    /// Dimming factor for syntax highlighting colors (0.0-1.0)
    pub syntax_dimming_factor: f64,
    /// Width correction factor for text wrapping calculations
    pub width_correction_factor: f64,
}

impl SidebarFonts {
    /// Get the appropriate code font variant based on font style flags
    pub fn get_code_font_variant(
        &self,
        style_flags: Option<&crate::termwindow::box_model::FontStyleFlags>,
    ) -> &Rc<LoadedFont> {
        match style_flags {
            Some(flags) if flags.bold && flags.italic => {
                self.code_bold_italic.as_ref().unwrap_or(&self.code)
            }
            Some(flags) if flags.bold => self.code_bold.as_ref().unwrap_or(&self.code),
            Some(flags) if flags.italic => self.code_italic.as_ref().unwrap_or(&self.code),
            _ => &self.code,
        }
    }

    /// Get the appropriate body font variant for markdown emphasis
    pub fn get_body_font_for_emphasis(&self, bold: bool, italic: bool) -> &Rc<LoadedFont> {
        match (bold, italic) {
            (true, true) => self.body_bold_italic.as_ref().unwrap_or(&self.body),
            (true, false) => self.body_bold.as_ref().unwrap_or(&self.body),
            (false, true) => self.body_italic.as_ref().unwrap_or(&self.body),
            (false, false) => &self.body,
        }
    }
}

pub mod ai_sidebar;
pub mod animation;
pub mod components;
pub mod settings_sidebar;

pub use ai_sidebar::AiSidebar;
pub use animation::{SidebarAnimation, SidebarPositionAnimation};
pub use components::ScrollbarInfo;
pub use settings_sidebar::SettingsSidebar;

/// Information about scrollbars in a sidebar that need external rendering
#[derive(Default)]
pub struct SidebarScrollbars {
    pub activity_log: Option<ScrollbarInfo>,
    pub chat_input: Option<ScrollbarInfo>,
    // Future: Add more scrollbar info for other scrollable areas
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPosition {
    Left,
    Right,
}

#[derive(Clone)]
pub struct SidebarState {
    pub visible: bool,
    pub position: SidebarPosition,
    pub animation: SidebarPositionAnimation,
    pub animation_target_visible: bool,
    pub width: u16,
}

impl SidebarState {
    pub fn new(position: SidebarPosition, width: u16) -> Self {
        Self::new_with_visibility(position, width, false)
    }

    pub fn new_with_visibility(
        position: SidebarPosition,
        width: u16,
        show_on_startup: bool,
    ) -> Self {
        // Sidebar slides in from off-screen
        let (start_pos, end_pos) = match position {
            SidebarPosition::Left => (-(width as f32), 0.0),
            SidebarPosition::Right => (width as f32, 0.0),
        };

        let mut state = Self {
            visible: show_on_startup,
            position,
            animation: SidebarPositionAnimation::new(200, start_pos, end_pos),
            animation_target_visible: show_on_startup,
            width,
        };

        // If showing on startup, immediately set to the visible position
        if show_on_startup {
            // Create animation that's already at the end position
            state.animation = SidebarPositionAnimation::new(200, end_pos, end_pos);
            // Start animation in the forward direction (showing)
            state.animation.start(true);
            state.visible = true;
            // Animation will immediately return end_pos since start == end
        }

        state
    }

    pub fn toggle_visibility(&mut self) {
        // Toggle target based on the final state, not the current visible state
        let was_target = self.animation_target_visible;
        self.animation_target_visible = !self.animation_target_visible;
        log::info!(
            "SidebarState::toggle_visibility: was_target={}, new_target={}, visible={}",
            was_target,
            self.animation_target_visible,
            self.visible
        );
        // No animation - just update the visible state immediately
        self.visible = self.animation_target_visible;
    }

    pub fn is_animating(&self) -> bool {
        false // No animation
    }

    pub fn finish_animation(&mut self) {
        log::info!(
            "SidebarState::finish_animation: visible {} -> {}",
            self.visible,
            self.animation_target_visible
        );
        self.visible = self.animation_target_visible;
    }

    pub fn get_animation_progress(&mut self, _duration_ms: u64) -> Option<f32> {
        if self.animation.is_animating() {
            self.animation.get_progress()
        } else {
            None
        }
    }

    /// Get the current position offset for rendering
    pub fn get_position_offset(&mut self) -> f32 {
        // No animation - sidebar is either fully visible or fully hidden
        if self.visible {
            0.0
        } else {
            self.width as f32
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SidebarMode {
    /// Sidebar overlays on top of terminal content
    Overlay,
    /// Sidebar expands the window, terminal content shifts
    Expand,
}

#[derive(Debug, Clone)]
pub struct SidebarConfig {
    pub width: u16,
    pub position: SidebarPosition,
    pub mode: SidebarMode,
    pub show_on_startup: bool,
    pub animation_duration_ms: u64,
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            width: 300,
            position: SidebarPosition::Right,
            mode: SidebarMode::Expand,
            show_on_startup: false,
            animation_duration_ms: 200,
        }
    }
}

/// Trait defining the interface for sidebar implementations
///
/// Sidebars must be thread-safe (Send + Sync) and provide methods for:
/// - Rendering content as Elements
/// - Managing visibility and dimensions
/// - Handling input events
/// - Providing scrollbar information
pub trait Sidebar: Send + Sync {
    /// Return the rendered content for this sidebar
    ///
    /// The fonts parameter provides pre-loaded fonts for consistent typography.
    /// The window_height is used for calculating scrollable regions.
    fn render(&mut self, fonts: &SidebarFonts, window_height: f32) -> Element;

    // DEPRECATED: This method is no longer used since fonts are passed in render()
    fn set_font_config(&mut self, _fonts: &wezterm_font::FontConfiguration) {
        // Default implementation does nothing - DEPRECATED
    }

    // Get scrollbar information for external rendering
    fn get_scrollbars(&self) -> SidebarScrollbars {
        SidebarScrollbars::default()
    }

    fn get_width(&self) -> u16;

    fn is_visible(&self) -> bool;

    fn toggle_visibility(&mut self);

    fn get_position(&self) -> SidebarPosition;

    fn set_width(&mut self, width: u16);

    fn handle_mouse_event(&mut self, _event: &MouseEvent) -> Result<bool> {
        Ok(false)
    }

    fn handle_key_event(&mut self, _key: &KeyCode, _modifiers: KeyModifiers) -> Result<bool> {
        Ok(false)
    }

    /// Returns true if this sidebar should capture keyboard input
    /// (either has an active modal or focused input field)
    fn has_keyboard_focus(&self) -> bool {
        false
    }

    /// Clear any focus state in the sidebar (e.g., chat input focus)
    fn clear_focus(&mut self) {
        // Default implementation does nothing
    }

    // Allow downcasting for specialized rendering
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

pub struct SidebarManager {
    left_sidebar: Option<Arc<Mutex<dyn Sidebar>>>,
    right_sidebar: Option<Arc<Mutex<dyn Sidebar>>>,
    left_state: SidebarState,
    right_state: SidebarState,
    pub config: SidebarConfig,
}

impl SidebarManager {
    pub fn new(config: SidebarConfig) -> Self {
        // Create specific configs for left and right sidebars
        let mut left_config = config.clone();
        left_config.position = SidebarPosition::Left;
        left_config.mode = SidebarMode::Overlay;
        left_config.width = 350; // Slightly wider for settings
        left_config.show_on_startup = false; // Left sidebar always starts collapsed

        let mut right_config = config;
        right_config.position = SidebarPosition::Right;
        right_config.mode = SidebarMode::Expand;

        let left_state = SidebarState::new_with_visibility(
            SidebarPosition::Left,
            left_config.width,
            left_config.show_on_startup,
        );
        let right_state = SidebarState::new_with_visibility(
            SidebarPosition::Right,
            right_config.width,
            right_config.show_on_startup,
        );

        Self {
            left_sidebar: None,
            right_sidebar: None,
            left_state,
            right_state,
            config: right_config, // Keep the original/default as base
        }
    }

    pub fn set_left_sidebar(&mut self, sidebar: Arc<Mutex<dyn Sidebar>>) {
        self.left_sidebar = Some(sidebar);
    }

    pub fn set_right_sidebar(&mut self, sidebar: Arc<Mutex<dyn Sidebar>>) {
        self.right_sidebar = Some(sidebar);
    }

    pub fn get_left_sidebar(&self) -> Option<Arc<Mutex<dyn Sidebar>>> {
        self.left_sidebar.clone()
    }

    pub fn get_right_sidebar(&self) -> Option<Arc<Mutex<dyn Sidebar>>> {
        self.right_sidebar.clone()
    }

    pub fn toggle_left_sidebar(&mut self) {
        self.left_state.toggle_visibility();
        if let Some(sidebar) = &self.left_sidebar {
            sidebar.lock().unwrap().toggle_visibility();
        }
    }

    pub fn toggle_right_sidebar(&mut self) {
        log::info!(
            "toggle_right_sidebar: before - visible={}, animation_target_visible={}",
            self.right_state.visible,
            self.right_state.animation_target_visible
        );
        self.right_state.toggle_visibility();
        log::info!(
            "toggle_right_sidebar: after - visible={}, animation_target_visible={}",
            self.right_state.visible,
            self.right_state.animation_target_visible
        );
        // Don't synchronize the AI sidebar visibility here - it should follow
        // the animation state, not toggle independently
    }

    pub fn is_left_visible(&self) -> bool {
        self.left_state.visible
    }

    pub fn is_right_visible(&self) -> bool {
        // For Expand mode, check if the sidebar is actually expanded beyond the minimum width
        if self.config.mode == SidebarMode::Expand {
            // The sidebar is considered visible if it's expanded (not collapsed to minimum)
            self.right_state.visible && self.right_state.animation_target_visible
        } else {
            self.right_state.visible
        }
    }

    pub fn set_right_visible(&mut self, visible: bool) {
        self.right_state.visible = visible;
        self.right_state.animation_target_visible = visible;
        // Don't toggle the sidebar visibility here - it should be managed
        // through toggle_right_sidebar to avoid state synchronization issues
    }

    pub fn set_right_width(&mut self, width: u16) {
        self.right_state.width = width;
    }

    pub fn get_left_width(&self) -> u16 {
        if self.is_left_visible() {
            self.left_state.width
        } else {
            0
        }
    }

    pub fn get_right_width(&self) -> u16 {
        const MIN_SIDEBAR_WIDTH: u16 = 25;

        if self.is_right_visible() {
            // In Expand mode, return at least MIN_SIDEBAR_WIDTH
            if self.config.mode == SidebarMode::Expand && !self.right_state.animation_target_visible
            {
                MIN_SIDEBAR_WIDTH
            } else {
                self.right_state.width
            }
        } else {
            0
        }
    }

    /// Get the actual configured width of the right sidebar (not affected by animation state)
    pub fn get_right_sidebar_actual_width(&self) -> u16 {
        self.right_state.width
    }

    pub fn update_animations(&mut self) -> bool {
        // No animations anymore
        false
    }

    pub fn get_left_animation_progress(&mut self) -> f32 {
        self.left_state
            .get_animation_progress(self.config.animation_duration_ms)
            .unwrap_or(1.0)
    }

    pub fn get_right_animation_progress(&mut self) -> f32 {
        self.right_state
            .get_animation_progress(self.config.animation_duration_ms)
            .unwrap_or(1.0)
    }

    /// Get the current position offset for the left sidebar
    pub fn get_left_position_offset(&mut self) -> f32 {
        self.left_state.get_position_offset()
    }

    /// Get the current position offset for the right sidebar
    pub fn get_right_position_offset(&mut self) -> f32 {
        self.right_state.get_position_offset()
    }

    /// Returns the extra window width needed for Expand-mode sidebars
    pub fn get_window_expansion(&self) -> u16 {
        // Only the right sidebar expands the window in our current design
        const MIN_SIDEBAR_WIDTH: u16 = 25; // Just enough to show a hint of sidebar past button

        let should_expand = self.config.mode == SidebarMode::Expand;
        let result = if should_expand {
            if self.right_state.visible {
                self.right_state.width
            } else {
                MIN_SIDEBAR_WIDTH
            }
        } else {
            0
        };
        log::trace!(
            "get_window_expansion: mode={:?}, visible={}, result={}",
            self.config.mode,
            self.right_state.visible,
            result
        );
        result
    }

    /// Returns the left offset for terminal content when sidebars affect positioning
    pub fn get_terminal_left_offset(&self) -> u16 {
        // Terminal content doesn't shift for overlay sidebars
        0
    }
}
