use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::termwindow::{UIItem, UIItemType};
use anyhow::Result;
use euclid;
use window::color::LinearRgba;
use window::WindowOps;

// Minimum width to keep visible when sidebar is "collapsed"
const MIN_SIDEBAR_WIDTH: f32 = 25.0;

impl crate::TermWindow {
    pub fn paint_sidebars(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        log::info!("paint_sidebars called");

        // Update sidebar animations and check if we need to redraw
        let needs_redraw = self.sidebar_manager.borrow_mut().update_animations();
        if needs_redraw {
            self.window.as_ref().unwrap().invalidate();
        }

        // Check visibility first, then borrow and paint
        let left_visible = self.sidebar_manager.borrow().is_left_visible();
        let right_visible = self.sidebar_manager.borrow().is_right_visible();

        log::info!(
            "Sidebar visibility check: left={}, right={}",
            left_visible,
            right_visible
        );

        // Paint left sidebar if visible
        if left_visible {
            self.paint_left_sidebar(layers)?;
        }

        // Paint right sidebar if visible
        if right_visible {
            self.paint_right_sidebar(layers)?;
        }

        // Always paint toggle buttons AFTER sidebars to ensure they're on top
        self.paint_sidebar_toggle_buttons(layers)?;

        Ok(())
    }

    pub fn paint_sidebar_toggle_buttons(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
    ) -> Result<()> {
        log::info!("paint_sidebar_toggle_buttons called");

        let _config = &self.config;

        // Get configuration for right sidebar button
        let button_size = 40.0; // TODO: Read from config.clibuddy.right_sidebar.button.size
        let button_margin = 10.0;
        let border = self.get_os_border();

        log::info!(
            "Window dimensions: {}x{}, show_tab_bar: {}",
            self.dimensions.pixel_width,
            self.dimensions.pixel_height,
            self.show_tab_bar
        );
        log::info!("Border: {:?}", border);

        // Get sidebar dimensions for proper positioning
        let sidebar_manager = self.sidebar_manager.borrow();
        let is_right_visible = sidebar_manager.is_right_visible();
        let expansion = sidebar_manager.get_window_expansion() as f32;
        drop(sidebar_manager);

        // Calculate button position - align with left edge of scrollbar
        let padding = self.effective_right_padding(&self.config) as f32;
        let _terminal_scrollbar_padding = self.terminal_scrollbar_padding();

        // Button should align with the left edge of the scrollbar
        // The scrollbar is positioned at: window_width - padding - border + terminal_scrollbar_padding
        // We want the button at the same x position
        let button_x = if expansion > 0.0 {
            // Sidebar is visible/expanding - position relative to terminal content
            self.dimensions.pixel_width as f32 - expansion - padding - border.right.get() as f32
        } else {
            // No sidebar - position at the start of the padding area (where scrollbar begins)
            self.dimensions.pixel_width as f32 - padding - border.right.get() as f32
        };

        let button_y = if self.show_tab_bar {
            // Tab bar is visible - center button vertically in tab bar
            let tab_bar_height = self.tab_bar_pixel_height().unwrap_or(0.0);
            border.top.get() as f32 + (tab_bar_height - button_size) / 2.0
        } else {
            // No tab bar - position button at the top with margin
            border.top.get() as f32 + button_margin
        };

        log::info!(
            "Button position: x={}, y={}, size={}, visible={}",
            button_x,
            button_y,
            button_size,
            is_right_visible
        );

        // Use the filled_rectangle helper which handles all the coordinate conversion
        let button_rect = euclid::rect(button_x, button_y, button_size, button_size);
        let button_color = LinearRgba::with_components(0.2, 0.4, 1.0, 1.0); // Bright blue

        // Render button on layer 2 (highest available) to ensure it's above sidebar
        self.filled_rectangle(layers, 2, button_rect, button_color)?;

        // Add UI item for click detection - use consistent positioning
        self.ui_items.push(UIItem {
            x: button_x as usize,
            y: button_y as usize,
            width: button_size as usize,
            height: button_size as usize,
            item_type: UIItemType::SidebarButton(crate::sidebar::SidebarPosition::Right),
        });

        log::info!(
            "Added UI item at ({}, {}) with size {}x{}",
            button_x,
            button_y,
            button_size,
            button_size
        );

        Ok(())
    }

    fn paint_left_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        log::info!("paint_left_sidebar called");
        let mut sidebar_manager = self.sidebar_manager.borrow_mut();
        let width = sidebar_manager.get_left_width() as f32;
        let x_offset = sidebar_manager.get_left_position_offset();

        log::info!("Left sidebar: width={}, x_offset={}", width, x_offset);

        // Left sidebar overlays, so we render it on top
        // The animation system returns the offset directly

        // Background using filled_rectangle for proper coordinate transformation
        let sidebar_rect = euclid::rect(x_offset, 0.0, width, self.dimensions.pixel_height as f32);
        let sidebar_bg_color = LinearRgba::with_components(0.05, 0.05, 0.06, 0.95);
        self.filled_rectangle(layers, 2, sidebar_rect, sidebar_bg_color)?;

        // TODO: Render actual sidebar content
        // For now, just render the background

        Ok(())
    }

    fn paint_right_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        log::info!("paint_right_sidebar called");

        let mut sidebar_manager = self.sidebar_manager.borrow_mut();
        let full_width = sidebar_manager.get_right_width() as f32;
        let x_offset = sidebar_manager.get_right_position_offset();
        let expansion = sidebar_manager.get_window_expansion() as f32;

        log::info!(
            "Right sidebar: full_width={}, x_offset={}, expansion={}",
            full_width,
            x_offset,
            expansion
        );

        // Calculate visible width and position based on animation state
        let (visible_width, sidebar_x) = if expansion == MIN_SIDEBAR_WIDTH as f32 {
            // Fully collapsed state - show only MIN_SIDEBAR_WIDTH
            (
                MIN_SIDEBAR_WIDTH,
                self.dimensions.pixel_width as f32 - MIN_SIDEBAR_WIDTH,
            )
        } else if x_offset > 0.0 {
            // Collapsing animation in progress
            let progress = x_offset / (full_width - MIN_SIDEBAR_WIDTH);
            let visible = full_width - (full_width - MIN_SIDEBAR_WIDTH) * progress;
            let x_pos = self.dimensions.pixel_width as f32 - expansion + x_offset;
            (visible, x_pos)
        } else {
            // Fully expanded or expanding
            (full_width, self.dimensions.pixel_width as f32 - expansion)
        };

        log::info!(
            "Right sidebar rendering at x={}, visible_width={}, window_width={}, expansion={}",
            sidebar_x,
            visible_width,
            self.dimensions.pixel_width,
            expansion
        );

        // Draw the sidebar background
        // Use layer 2 to render on top of terminal content and overlay
        let sidebar_rect = euclid::rect(
            sidebar_x,
            0.0,
            visible_width,
            self.dimensions.pixel_height as f32,
        );
        // Use configured color from clibuddy config
        // TODO: Read from config.clibuddy.right_sidebar.background_color
        let sidebar_bg_color = LinearRgba::with_components(0.02, 0.02, 0.024, 1.0); // rgba(5, 5, 6, 1.0)

        self.filled_rectangle(layers, 2, sidebar_rect, sidebar_bg_color)?;

        // Add UI item for the sidebar area to capture mouse events
        self.ui_items.push(UIItem {
            x: sidebar_x as usize,
            y: 0,
            width: visible_width as usize,
            height: self.dimensions.pixel_height,
            item_type: UIItemType::Sidebar(crate::sidebar::SidebarPosition::Right),
        });

        // We need to clone and drop the manager before using the sidebar
        let sidebar = sidebar_manager.get_right_sidebar();
        drop(sidebar_manager);

        // Render the actual AI sidebar content
        if let Some(sidebar) = sidebar {
            let mut sidebar_locked = sidebar.lock().unwrap();
            sidebar_locked.render();
            // TODO: Convert the rendered content to quads
        }

        Ok(())
    }
}
