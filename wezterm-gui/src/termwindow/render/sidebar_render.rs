use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::termwindow::box_model::{Element, ElementColors, ElementContent, LayoutContext};
use crate::termwindow::render::neon::{NeonRenderer, NeonStyle};
use crate::termwindow::{UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use anyhow::Result;
use config::{Dimension, DimensionContext};
use euclid;
use std::rc::Rc;
use std::sync::Arc;
use wezterm_font::LoadedFont;
use window::bitmaps::TextureRect;
use window::color::LinearRgba;
use window::{PointF, RectF, WindowOps};

// Minimum width to keep visible when sidebar is "collapsed"
const MIN_SIDEBAR_WIDTH: f32 = 25.0;

impl crate::TermWindow {
    pub fn paint_sidebars(&mut self, _layers: &mut TripleLayerQuadAllocator) -> Result<()> {
        log::trace!("paint_sidebars called");

        // Update sidebar animations and check if we need to redraw
        let needs_redraw = self.sidebar_manager.borrow_mut().update_animations();
        if needs_redraw {
            self.window.as_ref().unwrap().invalidate();
        }

        // Check if left sidebar exists (not just visible)
        let sidebar_manager = self.sidebar_manager.borrow();
        let has_left_sidebar = sidebar_manager.get_left_sidebar().is_some();
        let left_visible = sidebar_manager.is_left_visible();
        let _right_visible = sidebar_manager.is_right_visible();
        drop(sidebar_manager);

        // Paint left button bar background if left sidebar exists
        if has_left_sidebar {
            // Use z-index 32 for left sidebar background (per CLAUDE.md)
            let gl_state = self.render_state.as_ref().unwrap();
            let layer = gl_state.layer_for_zindex(32)?;
            let mut layers = layer.quad_allocator();
            self.paint_left_button_bar_background(&mut layers)?;
        }

        // Paint left sidebar if visible
        if left_visible {
            // Use z-index 32 for left sidebar background (per CLAUDE.md)
            let gl_state = self.render_state.as_ref().unwrap();
            let layer = gl_state.layer_for_zindex(32)?;
            let mut layers = layer.quad_allocator();
            self.paint_left_sidebar(&mut layers)?;
        }

        // Paint right sidebar if it exists (even when collapsed)
        let sidebar_manager = self.sidebar_manager.borrow();
        let has_right_sidebar = sidebar_manager.get_right_sidebar().is_some();
        drop(sidebar_manager);

        if has_right_sidebar {
            // Paint right sidebar at multiple z-indices for proper layering
            self.paint_right_sidebar()?;
        }

        // Paint toggle buttons at their respective z-indices
        self.paint_sidebar_toggle_buttons()?;

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

        // Now using dedicated z-index 4, so use sub-layer 0 for background
        self.filled_rectangle(layers, 0, bar_rect, bar_bg_color)?;

        Ok(())
    }

    pub fn paint_sidebar_toggle_buttons(&mut self) -> Result<()> {
        log::trace!("paint_sidebar_toggle_buttons called");
        let config = self.config.clone();

        // Common button configuration
        let button_size = 40.0;
        let button_margin = 10.0;
        let border = self.get_os_border();
        let icon_padding_left = 4.0; // Padding for icon position

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
        let is_right_visible = sidebar_manager.is_right_visible();
        let expansion = sidebar_manager.get_window_expansion() as f32;
        drop(sidebar_manager);

        if has_left_sidebar {
            // Get layer for left button at z-index 36
            let gl_state = self.render_state.as_ref().unwrap();
            let layer = gl_state.layer_for_zindex(36)?;
            let mut layers = layer.quad_allocator();

            // Left button is always at x=0 (left edge)
            let left_button_x = border.left.get() as f32;
            let left_button_rect = euclid::rect(left_button_x, button_y, button_size, button_size);

            // Create neon style for left button
            let left_neon_style =
                if let Some(left_style) = &config.clibuddy.sidebar_button.left_style {
                    if let Some(neon) = &left_style.neon {
                        NeonStyle::from_config(
                            neon.color.to_linear(),
                            neon.base_color.to_linear(),
                            Some(neon.glow_intensity),
                            Some(neon.glow_radius),
                            Some(config.clibuddy.sidebar_button.border_width),
                            is_left_visible,
                        )
                    } else {
                        // Fall back to default neon config
                        self.get_default_left_neon_style(is_left_visible, &config)
                    }
                } else {
                    // Use default style
                    self.get_default_left_neon_style(is_left_visible, &config)
                };

            log::debug!(
                "Left button style: is_active={}, glow_intensity={}, glow_radius={}",
                left_neon_style.is_active,
                left_neon_style.glow_intensity,
                left_neon_style.glow_radius
            );

            // Render button with neon effect
            self.render_neon_rect(
                &mut layers,
                left_button_rect,
                &left_neon_style,
                Some(config.clibuddy.sidebar_button.corner_radius),
            )?;

            // Render gear icon with neon effect
            let icon_font = self.fonts.sidebar_icon_font()?;

            // Render icon at z-index 37 (base 36 + 1 to avoid layer conflicts)
            let icon_bounds = euclid::rect(left_button_x + icon_padding_left, button_y, 40.0, 40.0);
            self.render_neon_glyph_with_bounds_and_zindex(
                &mut layers,
                "\u{f013}", // fa_gear
                icon_bounds,
                &icon_font,
                &left_neon_style,
                36, // Base z-index, will render at 37
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
        // Get layer for right button at z-index 16
        let gl_state = self.render_state.as_ref().unwrap();
        let layer = gl_state.layer_for_zindex(16)?;
        let mut layers = layer.quad_allocator();

        let padding = self.effective_right_padding(&self.config) as f32;

        // Calculate right button position - align with left edge of main scrollbar
        let right_button_x = if expansion > 0.0 {
            // Sidebar is visible/expanding - position relative to terminal content
            self.dimensions.pixel_width as f32 - expansion - padding - border.right.get() as f32
        } else {
            // No sidebar - position at the start of the padding area (where scrollbar begins)
            self.dimensions.pixel_width as f32 - padding - border.right.get() as f32
        };

        let right_button_rect = euclid::rect(right_button_x, button_y, button_size, button_size);

        // Create neon style for right button
        let right_neon_style =
            if let Some(right_style) = &config.clibuddy.sidebar_button.right_style {
                if let Some(neon) = &right_style.neon {
                    NeonStyle::from_config(
                        neon.color.to_linear(),
                        neon.base_color.to_linear(),
                        Some(neon.glow_intensity),
                        Some(neon.glow_radius),
                        Some(config.clibuddy.sidebar_button.border_width),
                        is_right_visible,
                    )
                } else {
                    // Fall back to default neon config
                    self.get_default_right_neon_style(is_right_visible, &config)
                }
            } else {
                // Use default style
                self.get_default_right_neon_style(is_right_visible, &config)
            };

        // Render button with neon effect
        self.render_neon_rect(
            &mut layers,
            right_button_rect,
            &right_neon_style,
            Some(config.clibuddy.sidebar_button.corner_radius),
        )?;

        // Render AI assistant icon with neon effect
        let icon_font = self.fonts.sidebar_icon_font()?;

        // Render icon at z-index 17 (base 16 + 1 to avoid layer conflicts)
        let icon_bounds = euclid::rect(right_button_x + icon_padding_left, button_y, 40.0, 40.0);
        self.render_neon_glyph_with_bounds_and_zindex(
            &mut layers,
            "\u{f0064}", // md_assistant
            icon_bounds,
            &icon_font,
            &right_neon_style,
            16, // Base z-index, will render at 17
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
        // Now using dedicated z-index 4, so use sub-layer 0 for background
        let sidebar_rect = euclid::rect(sidebar_x, 0.0, width, self.dimensions.pixel_height as f32);
        self.filled_rectangle(layers, 0, sidebar_rect, sidebar_bg_color)?;

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

    fn paint_right_sidebar(&mut self) -> Result<()> {
        let mut sidebar_manager = self.sidebar_manager.borrow_mut();
        let full_width = sidebar_manager.get_right_sidebar_actual_width() as f32;
        let _x_offset = sidebar_manager.get_right_position_offset();
        let expansion = sidebar_manager.get_window_expansion() as f32;
        let is_visible = sidebar_manager.is_right_visible();
        let sidebar_mode = sidebar_manager.config.mode;

        // Calculate sidebar position
        let (visible_width, sidebar_x) = if is_visible {
            if sidebar_mode == crate::sidebar::SidebarMode::Expand {
                (full_width, self.dimensions.pixel_width as f32 - full_width)
            } else {
                (full_width, self.dimensions.pixel_width as f32 - full_width)
            }
        } else {
            (
                MIN_SIDEBAR_WIDTH,
                self.dimensions.pixel_width as f32 - MIN_SIDEBAR_WIDTH,
            )
        };

        let sidebar_bg_color = LinearRgba::with_components(0.02, 0.02, 0.024, 1.0);

        // Get the actual activity log bounds from the sidebar
        let (activity_log_top, activity_log_bottom, activity_log_left, activity_log_right) = {
            let sidebar = sidebar_manager.get_right_sidebar();
            if let Some(sidebar) = sidebar {
                let sidebar_locked = sidebar.lock().unwrap();
                if let Some(ai_sidebar) = sidebar_locked
                    .as_any()
                    .downcast_ref::<crate::sidebar::ai_sidebar::AiSidebar>()
                {
                    if let Some(bounds) =
                        ai_sidebar.get_activity_log_bounds(self.dimensions.pixel_height as f32)
                    {
                        (
                            bounds.origin.y,
                            bounds.origin.y + bounds.size.height,
                            bounds.origin.x,
                            bounds.origin.x + bounds.size.width,
                        )
                    } else {
                        (
                            200.0,
                            self.dimensions.pixel_height as f32 - 120.0,
                            16.0,
                            visible_width - 16.0,
                        )
                    }
                } else {
                    (
                        200.0,
                        self.dimensions.pixel_height as f32 - 120.0,
                        16.0,
                        visible_width - 16.0,
                    )
                }
            } else {
                (
                    200.0,
                    self.dimensions.pixel_height as f32 - 120.0,
                    16.0,
                    visible_width - 16.0,
                )
            }
        };
        let activity_log_height = activity_log_bottom - activity_log_top;

        log::debug!("Cut-a-hole rendering: sidebar_x={}, visible_width={}, activity_log bounds: top={}, bottom={}, height={}", 
            sidebar_x, visible_width, activity_log_top, activity_log_bottom, activity_log_height);

        // Paint sidebar background at z-index 12 with a "hole" for the activity log
        let gl_state = self.render_state.as_ref().unwrap();

        // Top section (above activity log)
        if activity_log_top > 0.0 {
            let layer = gl_state.layer_for_zindex(12)?;
            let mut layers = layer.quad_allocator();
            let top_rect = euclid::rect(sidebar_x, 0.0, visible_width, activity_log_top);
            self.filled_rectangle(&mut layers, 0, top_rect, sidebar_bg_color)?;
        }

        // Bottom section (below activity log)
        if activity_log_bottom < self.dimensions.pixel_height as f32 {
            let layer = gl_state.layer_for_zindex(12)?;
            let mut layers = layer.quad_allocator();
            let bottom_rect = euclid::rect(
                sidebar_x,
                activity_log_bottom,
                visible_width,
                self.dimensions.pixel_height as f32 - activity_log_bottom,
            );
            self.filled_rectangle(&mut layers, 0, bottom_rect, sidebar_bg_color)?;
        }

        // Left edge of activity log area (if needed for borders)
        if activity_log_left > 0.0 {
            let layer = gl_state.layer_for_zindex(12)?;
            let mut layers = layer.quad_allocator();
            let left_rect = euclid::rect(
                sidebar_x,
                activity_log_top,
                activity_log_left,
                activity_log_height,
            );
            self.filled_rectangle(&mut layers, 0, left_rect, sidebar_bg_color)?;
        }

        // Right edge of activity log area (for scrollbar background)
        let right_edge_width = visible_width - activity_log_right;
        if right_edge_width > 0.0 {
            let layer = gl_state.layer_for_zindex(12)?;
            let mut layers = layer.quad_allocator();
            let right_rect = euclid::rect(
                sidebar_x + activity_log_right,
                activity_log_top,
                right_edge_width,
                activity_log_height,
            );
            self.filled_rectangle(&mut layers, 0, right_rect, sidebar_bg_color)?;
        }

        // DEBUG: Add a red border around the hole to make it visible
        let debug_color = LinearRgba::with_components(1.0, 0.0, 0.0, 0.5);
        let border_width = 2.0;

        // Get layer for debug borders
        let layer = gl_state.layer_for_zindex(12)?;
        let mut layers = layer.quad_allocator();

        // Top border
        let top_border = euclid::rect(
            sidebar_x + activity_log_left,
            activity_log_top - border_width,
            activity_log_right - activity_log_left,
            border_width,
        );
        self.filled_rectangle(&mut layers, 1, top_border, debug_color)?;

        // Bottom border
        let bottom_border = euclid::rect(
            sidebar_x + activity_log_left,
            activity_log_bottom,
            activity_log_right - activity_log_left,
            border_width,
        );
        self.filled_rectangle(&mut layers, 1, bottom_border, debug_color)?;

        // Add UI item for the sidebar area to capture mouse events
        // Exclude bottom-right corner for window resize handle
        let resize_exclusion = 20;
        if visible_width > resize_exclusion as f32 {
            // Main sidebar area (excluding bottom portion)
            self.ui_items.push(UIItem {
                x: sidebar_x as usize,
                y: 0,
                width: visible_width as usize,
                height: self
                    .dimensions
                    .pixel_height
                    .saturating_sub(resize_exclusion),
                item_type: UIItemType::Sidebar(crate::sidebar::SidebarPosition::Right),
            });

            // Left portion of bottom area (excluding resize corner)
            if visible_width > (resize_exclusion * 2) as f32 {
                self.ui_items.push(UIItem {
                    x: sidebar_x as usize,
                    y: self
                        .dimensions
                        .pixel_height
                        .saturating_sub(resize_exclusion),
                    width: (visible_width as usize).saturating_sub(resize_exclusion),
                    height: resize_exclusion,
                    item_type: UIItemType::Sidebar(crate::sidebar::SidebarPosition::Right),
                });
            }
        }

        // We need to clone and drop the manager before using the sidebar
        let sidebar = sidebar_manager.get_right_sidebar();
        drop(sidebar_manager);

        // Render the actual AI sidebar content
        if let Some(sidebar) = sidebar {
            let mut sidebar_locked = sidebar.lock().unwrap();

            let font = self.fonts.title_font()?;

            // First render the activity log content at z-index 10 (lower layer, will show through the hole)
            log::debug!("Rendering activity log at z-index 10");
            if let Some(ai_sidebar) = sidebar_locked
                .as_any_mut()
                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
            {
                // Get the activity log element
                let activity_log_element = ai_sidebar
                    .render_activity_log_content(&font, self.dimensions.pixel_height as f32);

                // Get the activity log bounds to position it correctly
                let activity_bounds = ai_sidebar
                    .get_activity_log_bounds(self.dimensions.pixel_height as f32)
                    .unwrap_or_else(|| {
                        euclid::rect(
                            16.0,
                            200.0,
                            visible_width - 32.0,
                            self.dimensions.pixel_height as f32 - 320.0,
                        )
                    });

                // Compute it at z-index 10 with bounds matching the hole
                let mut activity_log_computed = self.compute_element(
                    &LayoutContext {
                        width: DimensionContext {
                            dpi: self.dimensions.dpi as f32,
                            pixel_cell: self.render_metrics.cell_size.width as f32,
                            pixel_max: activity_bounds.size.width,
                        },
                        height: DimensionContext {
                            dpi: self.dimensions.dpi as f32,
                            pixel_cell: self.render_metrics.cell_size.height as f32,
                            pixel_max: activity_bounds.size.height,
                        },
                        bounds: euclid::rect(
                            0.0,
                            0.0,
                            activity_bounds.size.width,
                            activity_bounds.size.height,
                        ),
                        metrics: &self.render_metrics,
                        gl_state: self.render_state.as_ref().unwrap(),
                        zindex: 10, // Activity log content at z-index 10
                    },
                    &activity_log_element,
                )?;

                // Translate to the correct position within the sidebar
                // Note: activity_bounds.origin.x already includes the left padding (16px),
                // so we shouldn't add it to sidebar_x again
                activity_log_computed.translate(euclid::vec2(
                    sidebar_x + activity_bounds.origin.x,
                    activity_bounds.origin.y,
                ));

                log::debug!(
                    "Activity log computed bounds before translation: {:?}, after translation to y={}",
                    activity_log_computed.bounds,
                    activity_bounds.origin.y
                );

                // Render the activity log
                let gl_state = self.render_state.as_ref().unwrap();
                self.render_element(&activity_log_computed, gl_state, None)?;
                log::debug!("Activity log rendered at z-index 10");
            }

            // Now get the main sidebar element
            let element = sidebar_locked.render(&font, self.dimensions.pixel_height as f32);
            drop(sidebar_locked);

            // Render main sidebar content at z-index 14
            log::debug!("Rendering main sidebar content at z-index 14");
            let mut computed = self.compute_element(
                &LayoutContext {
                    width: DimensionContext {
                        dpi: self.dimensions.dpi as f32,
                        pixel_cell: self.render_metrics.cell_size.width as f32,
                        pixel_max: visible_width,
                    },
                    height: DimensionContext {
                        dpi: self.dimensions.dpi as f32,
                        pixel_cell: self.render_metrics.cell_size.height as f32,
                        pixel_max: self.dimensions.pixel_height as f32,
                    },
                    bounds: euclid::rect(
                        0.0,
                        0.0,
                        visible_width,
                        self.dimensions.pixel_height as f32,
                    ),
                    metrics: &self.render_metrics,
                    gl_state: self.render_state.as_ref().unwrap(),
                    zindex: 14,
                },
                &element,
            )?;

            // Translate the computed element to the sidebar position
            computed.translate(euclid::vec2(sidebar_x, 0.0));

            // Render the computed element to quads
            let gl_state = self.render_state.as_ref().unwrap();
            self.render_element(&computed, gl_state, None)?;

            // Extract UI items for mouse handling
            self.ui_items.extend(computed.ui_items());

            // Render scrollbars at z-index 16 after main content
            self.render_sidebar_scrollbars(&sidebar, sidebar_x, visible_width)?;

            // Update filter chip bounds with sidebar position
            let mut sidebar_locked = sidebar.lock().unwrap();
            if let Some(ai_sidebar) = sidebar_locked
                .as_any_mut()
                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
            {
                ai_sidebar.update_filter_chip_bounds(sidebar_x);
            }
            drop(sidebar_locked);
        }

        Ok(())
    }

    /// Helper function for filled rectangle rendering that doesn't require self
    fn render_filled_rect(
        layers: &mut TripleLayerQuadAllocator,
        layer_num: usize,
        rect: RectF,
        color: LinearRgba,
        pixel_width: f32,
        pixel_height: f32,
        filled_box_coords: TextureRect,
    ) -> Result<()> {
        let mut quad = layers.allocate(layer_num)?;
        let left_offset = pixel_width / 2.;
        let top_offset = pixel_height / 2.;

        quad.set_position(
            rect.min_x() as f32 - left_offset,
            rect.min_y() as f32 - top_offset,
            rect.max_x() as f32 - left_offset,
            rect.max_y() as f32 - top_offset,
        );
        quad.set_texture(filled_box_coords);
        quad.set_is_background();
        quad.set_fg_color(color);
        quad.set_hsv(None);
        Ok(())
    }

    /// Render scrollbars for the sidebar using direct rendering at z-index 12
    fn render_sidebar_scrollbars(
        &mut self,
        sidebar: &Arc<std::sync::Mutex<dyn crate::sidebar::Sidebar>>,
        sidebar_x: f32,
        sidebar_width: f32,
    ) -> Result<()> {
        use crate::termwindow::render::scrollbar_renderer::ScrollbarRenderer;

        // Get scrollbar info and keep lock to update bounds later
        let scrollbars = {
            let sidebar_locked = sidebar.lock().unwrap();
            sidebar_locked.get_scrollbars()
        };

        // Render activity log scrollbar if present
        if let Some(scrollbar_info) = scrollbars.activity_log {
            if scrollbar_info.should_show {
                // Get actual activity log bounds from sidebar
                let (scrollbar_top, scrollbar_height) = {
                    let sidebar_locked = sidebar.lock().unwrap();
                    if let Some(ai_sidebar) = sidebar_locked
                        .as_any()
                        .downcast_ref::<crate::sidebar::ai_sidebar::AiSidebar>(
                    ) {
                        if let Some(bounds) =
                            ai_sidebar.get_activity_log_bounds(self.dimensions.pixel_height as f32)
                        {
                            (bounds.origin.y, bounds.size.height)
                        } else {
                            (200.0, self.dimensions.pixel_height as f32 - 320.0)
                        }
                    } else {
                        (200.0, self.dimensions.pixel_height as f32 - 320.0)
                    }
                };

                let scrollbar_width = 8.0;
                let scrollbar_bounds = euclid::rect(
                    sidebar_x + sidebar_width - scrollbar_width - 4.0,
                    scrollbar_top,
                    scrollbar_width,
                    scrollbar_height,
                );

                // Create scrollbar renderer using pixel-based values
                let mut scrollbar = ScrollbarRenderer::new_vertical(
                    scrollbar_info.content_height,
                    scrollbar_info.viewport_height,
                    scrollbar_info.scroll_offset,
                    20.0, // min thumb size
                );

                // Get palette first (requires mutable borrow)
                let palette = self.palette().clone();

                // Now get other values
                let gl_state = self.render_state.as_ref().unwrap();
                let config = &self.config;
                let pixel_width = self.dimensions.pixel_width as f32;
                let pixel_height = self.dimensions.pixel_height as f32;
                let filled_box_coords = gl_state.util_sprites.filled_box.texture_coords();

                // Render at z-index 16 for right sidebar scrollbars
                let _ui_items = scrollbar.render_direct(
                    gl_state,
                    scrollbar_bounds,
                    16,
                    &palette,
                    config,
                    |layers, sub_layer, rect, color| {
                        Self::render_filled_rect(
                            layers,
                            sub_layer,
                            rect,
                            color,
                            pixel_width,
                            pixel_height,
                            filled_box_coords,
                        )
                    },
                )?;

                // Don't add scrollbar UI items - they conflict with terminal scrollbar
                // The sidebar will handle scrollbar events via hit testing
                log::debug!(
                    "Scrollbar rendered at bounds: ({}, {}, {}, {})",
                    scrollbar_bounds.origin.x,
                    scrollbar_bounds.origin.y,
                    scrollbar_bounds.size.width,
                    scrollbar_bounds.size.height
                );

                // Update sidebar with scrollbar bounds
                let mut sidebar_locked = sidebar.lock().unwrap();
                if let Some(ai_sidebar) = sidebar_locked
                    .as_any_mut()
                    .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
                {
                    ai_sidebar.set_scrollbar_bounds(scrollbar_bounds);
                }
                drop(sidebar_locked);
            }
        }

        Ok(())
    }

    /// Get default neon style for left button
    fn get_default_left_neon_style(
        &self,
        is_active: bool,
        config: &config::ConfigHandle,
    ) -> NeonStyle {
        // Check if there's a default neon config at the sidebar_button level
        if let Some(neon) = &config.clibuddy.sidebar_button.neon {
            NeonStyle::from_config(
                neon.color.to_linear(),
                neon.base_color.to_linear(),
                Some(neon.glow_intensity),
                Some(neon.glow_radius),
                Some(config.clibuddy.sidebar_button.border_width),
                is_active,
            )
        } else {
            // Hardcoded default cyan neon
            NeonStyle {
                neon_color: LinearRgba::with_components(0.0, 1.0, 1.0, 1.0), // Cyan
                base_color: LinearRgba::with_components(0.05, 0.05, 0.06, 1.0), // Dark gray
                glow_intensity: 0.7,
                glow_radius: 8.0, // 8px subtle glow
                border_width: 2.0,
                is_active,
            }
        }
    }

    /// Get default neon style for right button
    fn get_default_right_neon_style(
        &self,
        is_active: bool,
        config: &config::ConfigHandle,
    ) -> NeonStyle {
        // Check if there's a default neon config at the sidebar_button level
        if let Some(neon) = &config.clibuddy.sidebar_button.neon {
            NeonStyle::from_config(
                neon.color.to_linear(),
                neon.base_color.to_linear(),
                Some(neon.glow_intensity),
                Some(neon.glow_radius),
                Some(config.clibuddy.sidebar_button.border_width),
                is_active,
            )
        } else {
            // Hardcoded default pink/magenta neon
            NeonStyle {
                neon_color: LinearRgba::with_components(1.0, 0.08, 0.58, 1.0), // Deep pink
                base_color: LinearRgba::with_components(0.06, 0.04, 0.06, 1.0), // Dark purple-black
                glow_intensity: 0.8,
                glow_radius: 8.0, // 8px subtle glow
                border_width: 2.0,
                is_active,
            }
        }
    }
}
