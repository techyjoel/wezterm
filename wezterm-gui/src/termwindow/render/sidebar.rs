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
        let button_size = 40.0;
        let button_margin = 10.0;
        let border = self.get_os_border();

        // Right sidebar toggle button (AI icon)
        let mut quad = layers.allocate(2)?;

        let right_x = self.dimensions.pixel_width as f32 - button_size - button_margin;
        let y = border.top.get() as f32 + button_margin;

        quad.set_position(right_x, y, right_x + button_size, y + button_size);
        quad.set_fg_color(LinearRgba::with_components(0.2, 0.2, 0.25, 0.8));
        quad.set_is_background();

        // Add UI item for click detection
        self.ui_items.push(UIItem {
            x: right_x as usize,
            y: y as usize,
            width: button_size as usize,
            height: button_size as usize,
            item_type: UIItemType::SidebarButton(crate::sidebar::SidebarPosition::Right),
        });

        // TODO: Add actual icon rendering (e.g., AI assistant icon)

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
