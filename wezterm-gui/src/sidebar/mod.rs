use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use termwiz::input::{KeyCode, MouseEvent};
// Widget traits will be implemented differently without termwiz widgets

pub mod ai_sidebar;
pub mod animation;
pub mod components;
pub mod settings_sidebar;

pub use ai_sidebar::AiSidebar;
pub use animation::{SidebarAnimation, SidebarPositionAnimation};
pub use settings_sidebar::SettingsSidebar;

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

pub trait Sidebar: Send + Sync {
    // Return the rendered content for this sidebar
    // This should return whatever content type the sidebar wants to render
    fn render(&mut self);

    fn get_width(&self) -> u16;

    fn is_visible(&self) -> bool;

    fn toggle_visibility(&mut self);

    fn get_position(&self) -> SidebarPosition;

    fn set_width(&mut self, width: u16);

    fn handle_mouse_event(&mut self, _event: &MouseEvent) -> Result<bool> {
        Ok(false)
    }

    fn handle_key_event(&mut self, _key: &KeyCode) -> Result<bool> {
        Ok(false)
    }
}

pub struct SidebarManager {
    left_sidebar: Option<Arc<Mutex<dyn Sidebar>>>,
    right_sidebar: Option<Arc<Mutex<dyn Sidebar>>>,
    left_state: SidebarState,
    right_state: SidebarState,
    config: SidebarConfig,
}

impl SidebarManager {
    pub fn new(config: SidebarConfig) -> Self {
        // Create specific configs for left and right sidebars
        let mut left_config = config.clone();
        left_config.position = SidebarPosition::Left;
        left_config.mode = SidebarMode::Overlay;
        left_config.width = 350; // Slightly wider for settings

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
