use anyhow::Result;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use termwiz::input::{KeyCode, MouseEvent};
// Widget traits will be implemented differently without termwiz widgets

pub mod components;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPosition {
    Left,
    Right,
}

pub struct SidebarState {
    pub visible: bool,
    pub position: SidebarPosition,
    pub animation_start: Option<Instant>,
    pub animation_target_visible: bool,
    pub width: u16,
}

impl SidebarState {
    pub fn new(position: SidebarPosition, width: u16) -> Self {
        Self {
            visible: false,
            position,
            animation_start: None,
            animation_target_visible: false,
            width,
        }
    }

    pub fn toggle_visibility(&mut self) {
        self.animation_start = Some(Instant::now());
        self.animation_target_visible = !self.visible;
    }

    pub fn is_animating(&self) -> bool {
        self.animation_start.is_some() && self.visible != self.animation_target_visible
    }

    pub fn finish_animation(&mut self) {
        self.visible = self.animation_target_visible;
        self.animation_start = None;
    }

    pub fn get_animation_progress(&self, duration_ms: u64) -> Option<f32> {
        if let Some(start) = self.animation_start {
            let elapsed = start.elapsed().as_millis() as f32;
            let duration = duration_ms as f32;
            if elapsed >= duration {
                None
            } else {
                Some(elapsed / duration)
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct SidebarConfig {
    pub width: u16,
    pub position: SidebarPosition,
    pub show_on_startup: bool,
    pub animation_duration_ms: u64,
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            width: 300,
            position: SidebarPosition::Right,
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
        let left_state = SidebarState::new(SidebarPosition::Left, config.width);
        let right_state = SidebarState::new(SidebarPosition::Right, config.width);

        Self {
            left_sidebar: None,
            right_sidebar: None,
            left_state,
            right_state,
            config,
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
        self.right_state.toggle_visibility();
        if let Some(sidebar) = &self.right_sidebar {
            sidebar.lock().unwrap().toggle_visibility();
        }
    }

    pub fn is_left_visible(&self) -> bool {
        self.left_state.visible || self.left_state.is_animating()
    }

    pub fn is_right_visible(&self) -> bool {
        self.right_state.visible || self.right_state.is_animating()
    }

    pub fn get_left_width(&self) -> u16 {
        if self.is_left_visible() {
            self.left_state.width
        } else {
            0
        }
    }

    pub fn get_right_width(&self) -> u16 {
        if self.is_right_visible() {
            self.right_state.width
        } else {
            0
        }
    }

    pub fn update_animations(&mut self) -> bool {
        let mut needs_redraw = false;

        if let Some(progress) = self
            .left_state
            .get_animation_progress(self.config.animation_duration_ms)
        {
            needs_redraw = true;
            if progress >= 1.0 {
                self.left_state.finish_animation();
            }
        }

        if let Some(progress) = self
            .right_state
            .get_animation_progress(self.config.animation_duration_ms)
        {
            needs_redraw = true;
            if progress >= 1.0 {
                self.right_state.finish_animation();
            }
        }

        needs_redraw
    }

    pub fn get_left_animation_progress(&self) -> f32 {
        self.left_state
            .get_animation_progress(self.config.animation_duration_ms)
            .unwrap_or(1.0)
    }

    pub fn get_right_animation_progress(&self) -> f32 {
        self.right_state
            .get_animation_progress(self.config.animation_duration_ms)
            .unwrap_or(1.0)
    }
}
