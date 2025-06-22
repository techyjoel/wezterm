use super::{Sidebar, SidebarPosition};
use crate::termwindow::box_model::{Element, ElementColors, ElementContent};
use anyhow::Result;
use std::rc::Rc;
use termwiz::input::KeyCode;
use wezterm_font::LoadedFont;
use window::color::LinearRgba;
use window::MouseEvent;

pub struct SettingsSidebar {
    width: u16,
    visible: bool,
}

impl SettingsSidebar {
    pub fn new(width: u16) -> Self {
        Self {
            width,
            visible: false,
        }
    }
}

impl Sidebar for SettingsSidebar {
    fn render(&mut self, font: &Rc<LoadedFont>, _window_height: f32) -> Element {
        // Placeholder for settings sidebar
        Element::new(
            font,
            ElementContent::Text("Settings Sidebar (Coming Soon)".to_string()),
        )
        .colors(ElementColors {
            text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
            bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
            ..Default::default()
        })
    }

    fn get_width(&self) -> u16 {
        self.width
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    fn get_position(&self) -> SidebarPosition {
        SidebarPosition::Left
    }

    fn set_width(&mut self, width: u16) {
        self.width = width;
    }

    fn handle_mouse_event(&mut self, _event: &MouseEvent) -> Result<bool> {
        // No mouse handling for placeholder
        Ok(false)
    }

    fn handle_key_event(&mut self, _key: &KeyCode) -> Result<bool> {
        // No key handling for placeholder
        Ok(false)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
