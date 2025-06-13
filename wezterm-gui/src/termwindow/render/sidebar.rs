use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::termwindow::{UIItem, UIItemType};
use anyhow::Result;
use window::color::LinearRgba;

impl crate::TermWindow {
    pub fn paint_sidebars(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        // Always paint toggle buttons
        self.paint_sidebar_toggle_buttons(layers)?;

        // Check visibility first, then borrow and paint
        let left_visible = self.sidebar_manager.borrow().is_left_visible();
        let right_visible = self.sidebar_manager.borrow().is_right_visible();

        // Paint left sidebar if visible
        if left_visible {
            self.paint_left_sidebar(layers)?;
        }

        // Paint right sidebar if visible
        if right_visible {
            self.paint_right_sidebar(layers)?;
        }

        Ok(())
    }

    fn paint_sidebar_toggle_buttons(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
    ) -> Result<()> {
        let config = &self.config;
        
        // Get configuration for right sidebar button
        let button_size = 40.0; // TODO: Read from config.clibuddy.right_sidebar.button.size
        let button_margin = 10.0;
        let border = self.get_os_border();

        // Right sidebar toggle button (AI icon)
        // Allocate background quad
        let mut bg_quad = layers.allocate(2)?;
        
        let right_x = self.dimensions.pixel_width as f32 - button_size - button_margin;
        let y = border.top.get() as f32 + button_margin;

        // Set button background with a more visible color
        bg_quad.set_position(right_x, y, right_x + button_size, y + button_size);
        bg_quad.set_fg_color(LinearRgba::with_components(0.125, 0.125, 0.156, 0.8)); // rgba(32, 32, 40, 0.8)
        bg_quad.set_is_background();

        // Add a simple AI icon using text for now
        // We'll use a robot emoji or text as placeholder
        let icon_text = "AI";
        let icon_size = button_size * 0.5;
        
        // Create a text quad for the icon
        let mut text_quad = layers.allocate(2)?;
        let icon_x = right_x + (button_size - icon_size) / 2.0;
        let icon_y = y + (button_size - icon_size) / 2.0;
        
        text_quad.set_position(icon_x, icon_y, icon_x + icon_size, icon_y + icon_size);
        text_quad.set_fg_color(LinearRgba::with_components(0.125, 0.5, 1.0, 1.0)); // Blue color for AI
        text_quad.set_has_color(true);

        // Add UI item for click detection
        self.ui_items.push(UIItem {
            x: right_x as usize,
            y: y as usize,
            width: button_size as usize,
            height: button_size as usize,
            item_type: UIItemType::SidebarButton(crate::sidebar::SidebarPosition::Right),
        });

        Ok(())
    }

    fn paint_left_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        let mut sidebar_manager = self.sidebar_manager.borrow_mut();
        let width = sidebar_manager.get_left_width() as f32;
        let x_offset = sidebar_manager.get_left_position_offset();

        // Left sidebar overlays, so we render it on top
        // The animation system returns the offset directly

        // Background
        let mut quad = layers.allocate(2)?;
        quad.set_position(
            x_offset,
            0.0,
            x_offset + width,
            self.dimensions.pixel_height as f32,
        );
        quad.set_fg_color(LinearRgba::with_components(0.05, 0.05, 0.06, 0.95));
        quad.set_is_background();

        // TODO: Render actual sidebar content
        // For now, just render the background

        Ok(())
    }

    fn paint_right_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        let mut sidebar_manager = self.sidebar_manager.borrow_mut();
        let width = sidebar_manager.get_right_width() as f32;
        let x_offset = sidebar_manager.get_right_position_offset();

        // Right sidebar expands the window, so it's part of the main layer
        // The animation returns position offset, with positive values sliding in from right
        let window_width = self.dimensions.pixel_width as f32;
        let x_start = window_width;

        // Background
        let mut quad = layers.allocate(1)?;
        quad.set_position(
            x_start - x_offset,
            0.0,
            x_start + width - x_offset,
            self.dimensions.pixel_height as f32,
        );
        quad.set_fg_color(LinearRgba::with_components(0.05, 0.05, 0.06, 1.0));
        quad.set_is_background();

        // TODO: Render actual sidebar content (AI sidebar)
        // For now, just render the background

        Ok(())
    }
}
