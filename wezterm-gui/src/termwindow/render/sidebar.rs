use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::termwindow::box_model::{
    BoxDimension, Element, ElementColors, ElementContent, VerticalAlign,
};
use crate::termwindow::{UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use anyhow::Result;
use config::{Dimension, DimensionContext};
use euclid;
use std::rc::Rc;
use wezterm_font::LoadedFont;
use window::color::LinearRgba;
use window::{RectF, WindowOps};

// Minimum width to keep visible when sidebar is "collapsed"
const MIN_SIDEBAR_WIDTH: f32 = 25.0;

impl crate::TermWindow {
    pub fn paint_sidebars(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        // Update sidebar animations and check if we need to redraw
        let needs_redraw = self.sidebar_manager.borrow_mut().update_animations();
        if needs_redraw {
            self.window.as_ref().unwrap().invalidate();
        }

        // Check if left sidebar exists (not just visible)
        let sidebar_manager = self.sidebar_manager.borrow();
        let has_left_sidebar = sidebar_manager.get_left_sidebar().is_some();
        let left_visible = sidebar_manager.is_left_visible();
        let right_visible = sidebar_manager.is_right_visible();
        drop(sidebar_manager);

        // Paint left button bar background if left sidebar exists
        if has_left_sidebar {
            self.paint_left_button_bar_background(layers)?;
        }

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

    fn paint_left_button_bar_background(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
    ) -> Result<()> {
        // Paint the entire left button bar area with a dark gray background
        let button_bar_width = 40.0;
        let border = self.get_os_border();

        // Dark gray background for the entire button bar column
        let bar_bg_color = LinearRgba::with_components(0.15, 0.15, 0.15, 1.0); // Darker than button

        let bar_rect = euclid::rect(
            border.left.get() as f32,
            0.0,
            button_bar_width,
            self.dimensions.pixel_height as f32,
        );

        // Render on layer 1 so it's behind the button but above terminal background
        self.filled_rectangle(layers, 1, bar_rect, bar_bg_color)?;

        Ok(())
    }

    pub fn paint_sidebar_toggle_buttons(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
    ) -> Result<()> {
        let _config = &self.config;

        // Common button configuration
        let button_size = 40.0;
        let button_margin = 10.0;
        let border = self.get_os_border();

        // Common Y position calculation
        let button_y = if self.show_tab_bar {
            // Tab bar is visible - center button vertically in tab bar
            let tab_bar_height = self.tab_bar_pixel_height().unwrap_or(0.0);
            border.top.get() as f32 + (tab_bar_height - button_size) / 2.0
        } else {
            // No tab bar - position button at the top with margin
            border.top.get() as f32 + button_margin
        };

        // Paint left sidebar button if left sidebar is configured
        let sidebar_manager = self.sidebar_manager.borrow();
        let has_left_sidebar = sidebar_manager.get_left_sidebar().is_some();
        let is_left_visible = sidebar_manager.is_left_visible();
        let _is_right_visible = sidebar_manager.is_right_visible();
        let expansion = sidebar_manager.get_window_expansion() as f32;
        drop(sidebar_manager);

        if has_left_sidebar {
            // Left button is always at x=0 (left edge)
            let left_button_x = border.left.get() as f32;

            // Left button background (gear icon)
            let left_button_rect = euclid::rect(left_button_x, button_y, button_size, button_size);
            let left_button_color = if is_left_visible {
                LinearRgba::with_components(0.3, 0.3, 0.35, 1.0) // Darker when active
            } else {
                LinearRgba::with_components(0.2, 0.2, 0.25, 1.0) // Dark gray
            };

            // Draw background rectangle
            self.filled_rectangle(layers, 2, left_button_rect, left_button_color)?;

            // Render gear icon
            self.render_sidebar_icon(
                layers,
                '\u{f013}', // fa_gear
                left_button_x,
                button_y,
                button_size,
                LinearRgba::with_components(0.7, 0.7, 0.7, 1.0), // Medium gray icon for better contrast
            )?;

            // Add UI item for left button click detection
            self.ui_items.push(UIItem {
                x: left_button_x as usize,
                y: button_y as usize,
                width: button_size as usize,
                height: button_size as usize,
                item_type: UIItemType::SidebarButton(crate::sidebar::SidebarPosition::Left),
            });
        }

        // Paint right sidebar button
        let padding = self.effective_right_padding(&self.config) as f32;

        // Calculate right button position - align with left edge of scrollbar
        let right_button_x = if expansion > 0.0 {
            // Sidebar is visible/expanding - position relative to terminal content
            self.dimensions.pixel_width as f32 - expansion - padding - border.right.get() as f32
        } else {
            // No sidebar - position at the start of the padding area (where scrollbar begins)
            self.dimensions.pixel_width as f32 - padding - border.right.get() as f32
        };

        // Right button background
        let right_button_rect = euclid::rect(right_button_x, button_y, button_size, button_size);
        let right_button_color = LinearRgba::with_components(0.2, 0.4, 1.0, 1.0); // Bright blue

        // Draw background rectangle
        self.filled_rectangle(layers, 2, right_button_rect, right_button_color)?;

        // Render AI assistant icon
        self.render_sidebar_icon(
            layers,
            '\u{f0064}', // md_assistant
            right_button_x,
            button_y,
            button_size,
            LinearRgba::with_components(1.0, 1.0, 1.0, 1.0), // White icon
        )?;

        // Add UI item for right button click detection
        self.ui_items.push(UIItem {
            x: right_button_x as usize,
            y: button_y as usize,
            width: button_size as usize,
            height: button_size as usize,
            item_type: UIItemType::SidebarButton(crate::sidebar::SidebarPosition::Right),
        });

        Ok(())
    }

    fn paint_left_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        let sidebar_manager = self.sidebar_manager.borrow();
        let width = sidebar_manager.get_left_width() as f32;
        let is_visible = sidebar_manager.is_left_visible();
        drop(sidebar_manager);

        if !is_visible || width == 0.0 {
            return Ok(());
        }

        // Use hardcoded dark gray background for now
        // TODO: Read from configuration when available
        let sidebar_bg_color = LinearRgba::with_components(0.16, 0.16, 0.16, 1.0);

        // Left sidebar starts after the button bar (40px)
        let button_bar_width = 40.0;
        let sidebar_x = button_bar_width;

        // Background using filled_rectangle for proper coordinate transformation
        let sidebar_rect = euclid::rect(sidebar_x, 0.0, width, self.dimensions.pixel_height as f32);
        self.filled_rectangle(layers, 2, sidebar_rect, sidebar_bg_color)?;

        // Add UI item for the sidebar area to capture mouse events
        self.ui_items.push(UIItem {
            x: sidebar_x as usize,
            y: 0,
            width: width as usize,
            height: self.dimensions.pixel_height,
            item_type: UIItemType::Sidebar(crate::sidebar::SidebarPosition::Left),
        });

        // TODO: Render actual sidebar content using the Element system
        // For now, the sidebar just shows a background color as a placeholder

        Ok(())
    }

    fn paint_right_sidebar(&mut self, layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        let mut sidebar_manager = self.sidebar_manager.borrow_mut();
        // Use the actual sidebar width, not the dynamically calculated width
        // which changes during animation and breaks position calculations
        let full_width = sidebar_manager.get_right_sidebar_actual_width() as f32;
        let _x_offset = sidebar_manager.get_right_position_offset();
        let expansion = sidebar_manager.get_window_expansion() as f32;

        // No animation - sidebar is either fully visible or shows minimum width
        let (visible_width, sidebar_x) = if expansion == MIN_SIDEBAR_WIDTH as f32 {
            // Collapsed state - show only MIN_SIDEBAR_WIDTH
            (
                MIN_SIDEBAR_WIDTH,
                self.dimensions.pixel_width as f32 - MIN_SIDEBAR_WIDTH,
            )
        } else {
            // Expanded state
            (full_width, self.dimensions.pixel_width as f32 - expansion)
        };

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

    /// Helper method to render a nerdfont icon at the specified position
    fn render_sidebar_icon(
        &mut self,
        _layers: &mut TripleLayerQuadAllocator,
        icon_char: char,
        x: f32,
        y: f32,
        button_size: f32,
        icon_color: LinearRgba,
    ) -> Result<()> {
        let font = &self.fonts.title_font()?;
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());

        // Create text element with the icon character
        let icon_text = icon_char.to_string();

        // Use the full button bounds for positioning
        let icon_bounds = RectF::new(
            euclid::point2(x, y),
            euclid::size2(button_size, button_size),
        );

        let icon_element = Element::new(font, ElementContent::Text(icon_text))
            .vertical_align(VerticalAlign::Middle)
            .colors(ElementColors {
                border: crate::termwindow::box_model::BorderColor::default(),
                bg: LinearRgba::TRANSPARENT.into(),
                text: icon_color.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Pixels(button_size * 0.01),
                right: Dimension::Pixels(button_size * 0.01),
                top: Dimension::Pixels(button_size * 0.01),
                bottom: Dimension::Pixels(button_size * 0.01),
            });

        // Create layout context
        let context = crate::termwindow::box_model::LayoutContext {
            width: DimensionContext {
                dpi: self.dimensions.dpi as f32,
                pixel_max: self.dimensions.pixel_width as f32,
                pixel_cell: metrics.cell_size.width as f32,
            },
            height: DimensionContext {
                dpi: self.dimensions.dpi as f32,
                pixel_max: self.dimensions.pixel_height as f32,
                pixel_cell: metrics.cell_size.height as f32,
            },
            bounds: icon_bounds,
            metrics: &metrics,
            gl_state: self.render_state.as_ref().unwrap(),
            zindex: 3, // Render above background
        };

        // Compute the element layout
        let computed = self.compute_element(&context, &icon_element)?;

        // Render the computed element
        self.render_element(&computed, self.render_state.as_ref().unwrap(), None)?;

        Ok(())
    }
}
