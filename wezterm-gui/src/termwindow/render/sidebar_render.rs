//! Sidebar rendering integration for the terminal window
//!
//! This module handles the rendering of sidebars within the main terminal window,
//! including:
//! - Left and right sidebar painting
//! - Button bar rendering with neon effects
//! - Sidebar animation updates
//! - Modal overlay rendering
//! - Font loading and management for sidebars
//!
//! Sidebars are rendered at specific z-index layers as defined in CLAUDE.md:
//! - Left sidebar: z-indices 30-40
//! - Right sidebar: z-indices 10-16
//! - Modal overlays: z-indices 20-23

use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::sidebar::{AiSidebar, SidebarScrollbars};
use crate::termwindow::box_model::{
    set_width_correction_factor, ComputedElement, ComputedElementContent, Element, ElementCell,
    ElementColors, ElementContent, LayoutContext, RenderSource,
};
use crate::termwindow::render::neon::{NeonRenderer, NeonStyle};
use crate::termwindow::render::scrollbar_renderer::ScrollbarRenderer;
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
    /// Main entry point for sidebar rendering
    ///
    /// Updates animations and renders visible sidebars to appropriate z-index layers.
    /// This function is called during the main render loop and handles:
    /// - Animation state updates
    /// - Left sidebar rendering (if visible)
    /// - Right sidebar rendering (if visible)
    /// - Scrollbar rendering for both sidebars
    /// - Modal overlay rendering
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

        log::debug!("Scissor rect rendering: sidebar_x={}, visible_width={}, activity_log bounds: top={}, bottom={}, height={}", 
            sidebar_x, visible_width, activity_log_top, activity_log_bottom, activity_log_height);

        // Paint full sidebar background at z-index 10
        let gl_state = self.render_state.as_ref().unwrap();
        let layer = gl_state.layer_for_zindex(10)?;
        let mut layers = layer.quad_allocator();

        // Render full sidebar background
        let sidebar_rect = euclid::rect(
            sidebar_x,
            0.0,
            visible_width,
            self.dimensions.pixel_height as f32,
        );
        self.filled_rectangle(&mut layers, 0, sidebar_rect, sidebar_bg_color)?;

        // Render activity log background color in its area (creates visual frame)
        // This also stays at z-index 10 to create the frame effect
        let activity_log_bg_rect = euclid::rect(
            sidebar_x + activity_log_left,
            activity_log_top,
            activity_log_right - activity_log_left,
            activity_log_height,
        );
        // Activity log background color
        let activity_log_bg_color = LinearRgba::with_components(0.03, 0.03, 0.035, 1.0);
        self.filled_rectangle(&mut layers, 0, activity_log_bg_rect, activity_log_bg_color)?;

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

            // Get all fonts for the sidebar
            let heading_font = self.fonts.sidebar_heading_font()?;
            let body_font = self.fonts.sidebar_body_font()?;
            let code_font = self.fonts.sidebar_code_font()?;

            // Load synthetic font variants
            let (
                body_bold,
                body_italic,
                body_bold_italic,
                code_bold,
                code_italic,
                code_bold_italic,
            ) = self.load_sidebar_font_variants(&body_font, &code_font);

            let fonts = crate::sidebar::SidebarFonts {
                heading: heading_font.clone(),
                body: body_font,
                body_bold,
                body_italic,
                body_bold_italic,
                code: code_font,
                code_bold,
                code_italic,
                code_bold_italic,
                code_line_height: self.config.clibuddy.right_sidebar.fonts.code_line_height,
                code_line_margin: self.config.clibuddy.right_sidebar.fonts.code_line_margin,
                syntax_dimming_factor: self
                    .config
                    .clibuddy
                    .right_sidebar
                    .fonts
                    .syntax_dimming_factor,
                width_correction_factor: self
                    .config
                    .clibuddy
                    .right_sidebar
                    .fonts
                    .width_correction_factor,
            };

            // Get the color palette for syntax highlighting
            let palette = self.palette().clone();

            // First render the activity log content at z-index 10 (lower layer, will show through the hole)
            log::debug!("Rendering activity log at z-index 10");
            if let Some(ai_sidebar) = sidebar_locked
                .as_any_mut()
                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
            {
                // Code block opacity animation has been removed with horizontal scrolling
                // If needs_animation is false, has_animation remains None and animations stop

                // Get the activity log element
                // Set the width correction factor before rendering markdown
                set_width_correction_factor(fonts.width_correction_factor as f32);

                let activity_log_element = ai_sidebar.render_activity_log_content(
                    &fonts,
                    self.dimensions.pixel_height as f32,
                    &palette,
                );

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

                // Compute it at z-index 12 with bounds matching the viewport
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
                        zindex: 12, // Activity log content at z-index 12
                        source: RenderSource::Sidebar,
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

                // Apply scissor rect to z-index 12 before rendering
                let gl_state = self.render_state.as_ref().unwrap();
                let layer = gl_state.layer_for_zindex(12)?;

                // Convert bounds to window coordinates for scissor rect
                let scissor_rect = euclid::rect(
                    sidebar_x + activity_bounds.origin.x,
                    activity_bounds.origin.y,
                    activity_bounds.size.width,
                    activity_bounds.size.height,
                );
                layer.update_scissor_rect(scissor_rect);

                // Render the activity log (now clipped by scissor rect)
                self.render_element(&activity_log_computed, gl_state, None)?;

                // CRITICAL: Extract UI items from activity log for mouse handling
                self.ui_items.extend(activity_log_computed.ui_items());
                log::debug!(
                    "Activity log rendered at z-index 12 with scissor rect and {} UI items",
                    activity_log_computed.ui_items().len()
                );

                // Update height cache with actual rendered heights (for virtual scrolling)
                // We need to call this on the AiSidebar to update its height cache
                if let Some(ai_sidebar) = sidebar_locked.as_any_mut().downcast_mut::<AiSidebar>() {
                    // Get the viewport height from the activity bounds
                    let viewport_height = activity_bounds.size.height;
                    ai_sidebar
                        .update_activity_log_height_cache(&activity_log_computed, viewport_height);

                    // CRITICAL: Update activity item bounds for selection rendering
                    // Extract bounds from the computed UI items
                    for ui_item in activity_log_computed.ui_items() {
                        if let UIItemType::ActivityItemText { index, .. } = &ui_item.item_type {
                            let item_bounds = euclid::rect(
                                ui_item.x as f32,
                                ui_item.y as f32,
                                ui_item.width as f32,
                                ui_item.height as f32,
                            );
                            ai_sidebar.set_activity_item_bounds(*index, item_bounds);
                            log::debug!(
                                "Set activity item {} bounds: x={}, y={}, w={}, h={}",
                                index,
                                item_bounds.origin.x,
                                item_bounds.origin.y,
                                item_bounds.size.width,
                                item_bounds.size.height
                            );
                        }
                    }
                }
            }

            // Note: We'll render chat input text AFTER capturing bounds from the current frame

            // Now get the main sidebar element
            let element = sidebar_locked.render(&fonts, self.dimensions.pixel_height as f32);
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
                    source: RenderSource::Sidebar,
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

            // Capture bounds for suggestion and goal cards for selection rendering
            if let Ok(mut sidebar_guard) = sidebar.lock() {
                if let Some(ai_sidebar) = sidebar_guard.as_any_mut().downcast_mut::<AiSidebar>() {
                    for ui_item in computed.ui_items() {
                        match &ui_item.item_type {
                            crate::termwindow::UIItemType::SuggestionText { .. } => {
                                let bounds = euclid::rect(
                                    ui_item.x as f32,
                                    ui_item.y as f32,
                                    ui_item.width as f32,
                                    ui_item.height as f32,
                                );
                                ai_sidebar.set_suggestion_bounds(bounds);
                                log::debug!(
                                    "Set suggestion bounds: x={}, y={}, w={}, h={}",
                                    bounds.origin.x,
                                    bounds.origin.y,
                                    bounds.size.width,
                                    bounds.size.height
                                );
                            }
                            crate::termwindow::UIItemType::GoalText { .. } => {
                                let bounds = euclid::rect(
                                    ui_item.x as f32,
                                    ui_item.y as f32,
                                    ui_item.width as f32,
                                    ui_item.height as f32,
                                );
                                ai_sidebar.set_goal_bounds(bounds);
                                log::debug!(
                                    "GOAL BOUNDS DEBUG: Set goal bounds: x={}, y={}, w={}, h={} from UIItem at ({},{},{},{})",
                                    bounds.origin.x,
                                    bounds.origin.y,
                                    bounds.size.width,
                                    bounds.size.height,
                                    ui_item.x,
                                    ui_item.y,
                                    ui_item.width,
                                    ui_item.height
                                );

                                // Extract goal text positions from the computed element
                                if let Some(positions) =
                                    self.extract_goal_text_positions(&computed, &ui_item.item_type)
                                {
                                    log::debug!(
                                        "GOAL POSITIONS DEBUG: Extracted {} character positions for goal text",
                                        positions.len()
                                    );
                                    ai_sidebar.store_goal_positions(positions);
                                } else {
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Render chat input background and content using activity log pattern
            if let Ok(mut sidebar_guard) = sidebar.lock() {
                if let Some(ai_sidebar) = sidebar_guard.as_any_mut().downcast_mut::<AiSidebar>() {
                    // Find the ChatInput UI item to get its bounds
                    let chat_input_bounds = computed
                        .ui_items()
                        .iter()
                        .find(|item| {
                            matches!(
                                item.item_type,
                                crate::termwindow::UIItemType::ChatInput { .. }
                            )
                        })
                        .map(|item| {
                            euclid::rect::<f32, euclid::UnknownUnit>(
                                item.x as f32,
                                item.y as f32,
                                item.width as f32,
                                item.height as f32,
                            )
                        });

                    if let Some(bounds) = chat_input_bounds {
                        // Render background at z-index 13 (below main content)
                        let bg_color = ai_sidebar.get_chat_input_bg_color();
                        self.filled_rectangle(
                            &mut layers,
                            2, // sub-layer 2 for UI elements
                            euclid::rect(
                                bounds.origin.x,
                                bounds.origin.y,
                                bounds.size.width,
                                bounds.size.height,
                            ),
                            bg_color,
                        )?;

                        // Render border
                        let border_color = ai_sidebar.get_chat_input_border_color();
                        let border_thickness = 1.0;

                        // Top border
                        self.filled_rectangle(
                            &mut layers,
                            2,
                            euclid::rect(
                                bounds.origin.x,
                                bounds.origin.y,
                                bounds.size.width,
                                border_thickness,
                            ),
                            border_color,
                        )?;
                        // Bottom border
                        self.filled_rectangle(
                            &mut layers,
                            2,
                            euclid::rect(
                                bounds.origin.x,
                                bounds.origin.y + bounds.size.height - border_thickness,
                                bounds.size.width,
                                border_thickness,
                            ),
                            border_color,
                        )?;
                        // Left border
                        self.filled_rectangle(
                            &mut layers,
                            2,
                            euclid::rect(
                                bounds.origin.x,
                                bounds.origin.y,
                                border_thickness,
                                bounds.size.height,
                            ),
                            border_color,
                        )?;
                        // Right border
                        self.filled_rectangle(
                            &mut layers,
                            2,
                            euclid::rect(
                                bounds.origin.x + bounds.size.width - border_thickness,
                                bounds.origin.y,
                                border_thickness,
                                bounds.size.height,
                            ),
                            border_color,
                        )?;

                        // Calculate text viewport (inside padding and border)
                        let text_padding = 8.0;
                        let vertical_padding = 6.0;
                        let text_bounds = euclid::rect(
                            bounds.origin.x + border_thickness + text_padding,
                            bounds.origin.y + border_thickness + vertical_padding,
                            bounds.size.width - (border_thickness * 2.0) - (text_padding * 2.0),
                            // Subtract vertical padding from height to maintain symmetry
                            bounds.size.height
                                - (border_thickness * 2.0)
                                - (vertical_padding * 2.0),
                        );

                        // Store the chat input bounds for scrollbar positioning
                        ai_sidebar.set_chat_input_bounds(euclid::rect(
                            bounds.origin.x,
                            bounds.origin.y,
                            bounds.size.width,
                            bounds.size.height,
                        ));

                        // Get the chat input text content element
                        let chat_input_element =
                            ai_sidebar.render_chat_input_content(&fonts, text_bounds.size.width);

                        // Compute the chat input text at z-index 15
                        let mut chat_input_computed = self.compute_element(
                            &LayoutContext {
                                width: DimensionContext {
                                    dpi: self.dimensions.dpi as f32,
                                    pixel_cell: self.render_metrics.cell_size.width as f32,
                                    pixel_max: text_bounds.size.width,
                                },
                                height: DimensionContext {
                                    dpi: self.dimensions.dpi as f32,
                                    pixel_cell: self.render_metrics.cell_size.height as f32,
                                    pixel_max: text_bounds.size.height,
                                },
                                bounds: euclid::rect(
                                    0.0,
                                    0.0,
                                    text_bounds.size.width,
                                    text_bounds.size.height,
                                ),
                                metrics: &self.render_metrics,
                                gl_state: self.render_state.as_ref().unwrap(),
                                zindex: 15, // Chat input text at z-index 15
                                source: RenderSource::Sidebar,
                            },
                            &chat_input_element,
                        )?;

                        // Translate to absolute position
                        chat_input_computed
                            .translate(euclid::vec2(text_bounds.origin.x, text_bounds.origin.y));

                        // Apply scissor rect to z-index 15
                        let gl_state = self.render_state.as_ref().unwrap();
                        if let Ok(layer) = gl_state.layer_for_zindex(15) {
                            layer.update_scissor_rect(text_bounds);
                        } else {
                            log::error!(
                                "Failed to get layer for z-index 15 for chat input scissor rect"
                            );
                        }

                        // Debug: Log exact glyph positions from the computed element
                        if let Some(UIItemType::ChatInput { line_positions }) =
                            &chat_input_computed.item_type
                        {
                            log::debug!(
                                "Chat input has {} lines of exact glyph positions",
                                line_positions.len()
                            );
                            // Debug: log the first few positions
                            for (line_idx, line_pos) in line_positions.iter().enumerate().take(2) {
                                log::debug!("  Line {} has {} positions", line_idx, line_pos.len());
                                for (i, &(x_start, x_end, byte_offset)) in
                                    line_pos.iter().enumerate().take(5)
                                {
                                    log::debug!(
                                        "    Pos[{}]: x=({:.1}, {:.1}), byte_offset={}",
                                        i,
                                        x_start,
                                        x_end,
                                        byte_offset
                                    );
                                }
                            }
                        } else {
                            log::debug!("No UIItemType::ChatInput found in computed element");
                        }

                        // Extract positions from the chat input before rendering
                        if let Some(positions) = self.extract_chat_input_positions(&chat_input_computed) {
                            log::debug!(
                                "Extracted {} lines of positions for chat input",
                                positions.len()
                            );
                            ai_sidebar.set_chat_input_glyph_positions(positions);
                        }
                        
                        // Render the chat input text (now clipped by scissor rect)
                        self.render_element(&chat_input_computed, gl_state, None)?;

                        // Render cursor as overlay if focused
                        if let Some((cursor_x, cursor_y)) =
                            ai_sidebar.get_cursor_position(&fonts.body)
                        {
                            // Only render cursor if it's within the visible viewport
                            if cursor_y >= 0.0 && cursor_y < text_bounds.size.height {
                                let cursor_height = fonts.body.metrics().cell_height.get() as f32;
                                let cursor_width = 2.0; // 2px wide cursor

                                let cursor_rect = euclid::rect(
                                    text_bounds.origin.x + cursor_x,
                                    text_bounds.origin.y + cursor_y,
                                    cursor_width,
                                    cursor_height,
                                );

                                // Draw cursor as a filled rectangle at z-index 16
                                let gl_state = self.render_state.as_ref().unwrap();
                                let layer = gl_state.layer_for_zindex(16)?;
                                let mut layers = layer.quad_allocator();

                                self.filled_rectangle(
                                    &mut layers,
                                    0, // sub_layer
                                    cursor_rect,
                                    LinearRgba::with_components(0.9, 0.9, 0.9, 1.0), // Light gray cursor
                                )?;
                            }
                        }
                    }
                }
            }

            // Render selection overlays at z-index 13 (above text but below UI elements)
            self.render_sidebar_selection_overlays(&sidebar, sidebar_x)?;

            // Render sidebar scrollbars at z-index 16
            let sidebar_scrollbars = sidebar.lock().unwrap().get_scrollbars();

            log::debug!(
                "Sidebar scrollbars: activity_log={:?}, chat_input={:?}",
                sidebar_scrollbars.activity_log.is_some(),
                sidebar_scrollbars.chat_input.is_some()
            );
            // Render scrollbars if either activity log or chat input needs them
            let should_render_scrollbars = sidebar_scrollbars
                .activity_log
                .as_ref()
                .map_or(false, |s| s.should_show)
                || sidebar_scrollbars
                    .chat_input
                    .as_ref()
                    .map_or(false, |s| s.should_show);

            if should_render_scrollbars {
                self.render_sidebar_scrollbars(
                    sidebar_x,
                    visible_width,
                    &sidebar_scrollbars,
                    &sidebar,
                )?;
            }

            // Update filter chip bounds with sidebar position
            let mut sidebar_locked = sidebar.lock().unwrap();
            if let Some(ai_sidebar) = sidebar_locked
                .as_any_mut()
                .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
            {
                ai_sidebar.update_sidebar_position(sidebar_x);
            }
            drop(sidebar_locked);

            // Render modals at z-index 20-24
            self.render_sidebar_modals(&sidebar, sidebar_x, visible_width)?;
        }

        Ok(())
    }

    /// Render selection overlays for all selectable text in the sidebar
    fn render_sidebar_selection_overlays(
        &mut self,
        sidebar: &Arc<std::sync::Mutex<dyn crate::sidebar::Sidebar>>,
        sidebar_x: f32,
    ) -> Result<()> {
        let mut sidebar_locked = sidebar.lock().unwrap();
        if let Some(ai_sidebar) = sidebar_locked
            .as_any_mut()
            .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
        {
            let gl_state = self.render_state.as_ref().unwrap();
            // Use z-index 14 (same as content) with sub-layer 0 for selection rectangles
            // Sub-layer 0 renders behind text (which uses sub-layer 1), following terminal selection pattern
            let layer = gl_state.layer_for_zindex(14)?;
            let mut layers = layer.quad_allocator();

            // Get selection state
            if let Some(selection) = ai_sidebar.get_active_selection() {
                // Get selection rectangles from the sidebar
                let selection_rects = ai_sidebar.calculate_selection_rectangles(selection);

                // Bright blue selection color - same as before
                let selection_color = LinearRgba::with_components(0.0, 0.5, 1.0, 1.0);

                // Render each selection rectangle
                for (i, rect) in selection_rects.iter().enumerate() {
                    log::debug!(
                        "SELECTION DEBUG: Rectangle {}: x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                        i,
                        rect.origin.x,
                        rect.origin.y,
                        rect.size.width,
                        rect.size.height
                    );

                    // Sanity check rectangle dimensions
                    if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
                        continue;
                    }

                    // Check if rectangle is within reasonable bounds
                    if rect.origin.x < 0.0
                        || rect.origin.x > 2000.0
                        || rect.origin.y < 0.0
                        || rect.origin.y > 2000.0
                    {}

                    self.filled_rectangle(
                        &mut layers,
                        0, // sub_layer 0 for backgrounds
                        *rect,
                        selection_color,
                    )?;
                }

                // If no rectangles were returned, log why
                if selection_rects.is_empty() {}
            } else {
            }
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

    /// Render scrollbars for the sidebar using direct rendering at z-index 16
    fn render_sidebar_scrollbars(
        &mut self,
        sidebar_x: f32,
        sidebar_width: f32,
        scrollbars: &SidebarScrollbars,
        sidebar: &Arc<std::sync::Mutex<dyn crate::sidebar::Sidebar>>,
    ) -> Result<()> {
        use crate::termwindow::render::scrollbar_renderer::ScrollbarOrientation;

        if let Some(ref scrollbar_info) = scrollbars.activity_log {
            if scrollbar_info.should_show {
                // Get activity log bounds for positioning
                let activity_bounds = {
                    let locked = sidebar.lock().unwrap();
                    if let Some(ai_sidebar) = locked
                        .as_any()
                        .downcast_ref::<crate::sidebar::ai_sidebar::AiSidebar>()
                    {
                        ai_sidebar.get_activity_log_bounds(self.dimensions.pixel_height as f32)
                    } else {
                        None
                    }
                };

                if let Some(bounds) = activity_bounds {
                    let scrollbar_width = 10.0;
                    let scrollbar_x = sidebar_x + sidebar_width - scrollbar_width - 4.0;
                    let scrollbar_y = bounds.min_y();
                    let scrollbar_height = bounds.size.height;

                    let scrollbar_bounds =
                        euclid::rect(scrollbar_x, scrollbar_y, scrollbar_width, scrollbar_height);

                    // Create scrollbar renderer using pixel-based values
                    let mut scrollbar = ScrollbarRenderer::new_vertical(
                        scrollbar_info.content_height,
                        scrollbar_info.viewport_height,
                        scrollbar_info.scroll_offset,
                        crate::sidebar::components::scrollbar_helpers::MIN_THUMB_SIZE,
                    );

                    // Get palette first (requires mutable borrow)
                    let palette = self.palette().clone();

                    // Now get other values
                    let gl_state = self.render_state.as_ref().unwrap();
                    let config = &self.config;
                    let pixel_width = self.dimensions.pixel_width as f32;
                    let pixel_height = self.dimensions.pixel_height as f32;
                    let filled_box_coords = gl_state.util_sprites.filled_box.texture_coords();

                    // Activity log background color - slightly lighter than sidebar
                    let activity_log_bg = LinearRgba::with_components(0.03, 0.03, 0.035, 1.0);

                    // Render at z-index 16 for right sidebar scrollbars
                    let _ui_items = scrollbar.render_direct(
                        gl_state,
                        scrollbar_bounds,
                        16,
                        &palette,
                        config,
                        |layers, sub_layer, rect, color| {
                            // Intercept the background color for the scrollbar track
                            // Note: ScrollbarRenderer applies window_background_opacity to the background,
                            // so we need to check if this is the track background
                            let is_track_bg = sub_layer == 0; // Track is rendered on sub-layer 0
                            let final_color = if is_track_bg {
                                // Use activity log background with full opacity
                                activity_log_bg
                            } else {
                                // Keep original color (thumb, etc.)
                                color
                            };

                            Self::render_filled_rect(
                                layers,
                                sub_layer,
                                rect,
                                final_color,
                                pixel_width,
                                pixel_height,
                                filled_box_coords,
                            )
                        },
                    )?;

                    // Update the scrollbar bounds in the sidebar for hit testing
                    let mut locked = sidebar.lock().unwrap();
                    if let Some(ai_sidebar) = locked
                        .as_any_mut()
                        .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>(
                    ) {
                        ai_sidebar.set_scrollbar_bounds(scrollbar_bounds);
                    }

                    log::debug!(
                        "Scrollbar rendered at bounds: ({}, {}, {}, {})",
                        scrollbar_bounds.origin.x,
                        scrollbar_bounds.origin.y,
                        scrollbar_bounds.size.width,
                        scrollbar_bounds.size.height
                    );
                }
            }
        }

        // Render chat input scrollbar if needed
        if let Some(ref scrollbar_info) = scrollbars.chat_input {
            log::debug!(
                "Chat input scrollbar render check: should_show={}, content_height={}, viewport_height={}",
                scrollbar_info.should_show,
                scrollbar_info.content_height,
                scrollbar_info.viewport_height
            );
            if scrollbar_info.should_show {
                // Get chat input bounds for positioning
                let chat_input_bounds = {
                    let locked = sidebar.lock().unwrap();
                    if let Some(ai_sidebar) = locked
                        .as_any()
                        .downcast_ref::<crate::sidebar::ai_sidebar::AiSidebar>()
                    {
                        ai_sidebar.get_chat_input_bounds()
                    } else {
                        None
                    }
                };

                if let Some(bounds) = chat_input_bounds {
                    log::debug!(
                        "Chat input bounds found: ({}, {}, {}, {})",
                        bounds.origin.x,
                        bounds.origin.y,
                        bounds.size.width,
                        bounds.size.height
                    );

                    let scrollbar_width = 10.0;
                    let scrollbar_x = sidebar_x + sidebar_width - scrollbar_width - 4.0;
                    let scrollbar_y = bounds.min_y();
                    let scrollbar_height = bounds.size.height;

                    let scrollbar_bounds =
                        euclid::rect(scrollbar_x, scrollbar_y, scrollbar_width, scrollbar_height);

                    log::debug!(
                        "Chat input scrollbar will render at: ({}, {}, {}, {})",
                        scrollbar_bounds.origin.x,
                        scrollbar_bounds.origin.y,
                        scrollbar_bounds.size.width,
                        scrollbar_bounds.size.height
                    );

                    // Create scrollbar renderer using pixel-based values
                    let mut scrollbar = ScrollbarRenderer::new_vertical(
                        scrollbar_info.content_height,
                        scrollbar_info.viewport_height,
                        scrollbar_info.scroll_offset,
                        crate::sidebar::components::scrollbar_helpers::MIN_THUMB_SIZE,
                    );

                    // Get palette first (requires mutable borrow)
                    let palette = self.palette().clone();

                    // Now get other values
                    let gl_state = self.render_state.as_ref().unwrap();
                    let config = &self.config;
                    let pixel_width = self.dimensions.pixel_width as f32;
                    let pixel_height = self.dimensions.pixel_height as f32;
                    let filled_box_coords = gl_state.util_sprites.filled_box.texture_coords();

                    // Chat input background color
                    let chat_input_bg = LinearRgba::with_components(0.08, 0.08, 0.08, 1.0);

                    // Render at z-index 16 for scrollbars
                    let _ui_items = scrollbar.render_direct(
                        gl_state,
                        scrollbar_bounds,
                        16,
                        &palette,
                        config,
                        |layers, sub_layer, rect, color| {
                            // Intercept the background color for the scrollbar track
                            let is_track_bg = sub_layer == 0;
                            let final_color = if is_track_bg {
                                // Use chat input background with full opacity
                                chat_input_bg
                            } else {
                                // Keep original color (thumb, etc.)
                                color
                            };

                            Self::render_filled_rect(
                                layers,
                                sub_layer,
                                rect,
                                final_color,
                                pixel_width,
                                pixel_height,
                                filled_box_coords,
                            )
                        },
                    )?;
                }
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

    /// Render modals for the sidebar at z-index 20-24
    fn render_sidebar_modals(
        &mut self,
        sidebar: &Arc<std::sync::Mutex<dyn crate::sidebar::Sidebar>>,
        sidebar_x: f32,
        sidebar_width: f32,
    ) -> Result<()> {
        let mut sidebar_locked = sidebar.lock().unwrap();
        if let Some(ai_sidebar) = sidebar_locked
            .as_any_mut()
            .downcast_mut::<crate::sidebar::ai_sidebar::AiSidebar>()
        {
            // Get fonts
            let heading_font = self.fonts.sidebar_heading_font()?;
            let body_font = self.fonts.sidebar_body_font()?;
            let code_font = self.fonts.sidebar_code_font()?;

            // Load synthetic font variants
            let (
                body_bold,
                body_italic,
                body_bold_italic,
                code_bold,
                code_italic,
                code_bold_italic,
            ) = self.load_sidebar_font_variants(&body_font, &code_font);

            let fonts = crate::sidebar::SidebarFonts {
                heading: heading_font.clone(),
                body: body_font,
                body_bold,
                body_italic,
                body_bold_italic,
                code: code_font,
                code_bold,
                code_italic,
                code_bold_italic,
                code_line_height: self.config.clibuddy.right_sidebar.fonts.code_line_height,
                code_line_margin: self.config.clibuddy.right_sidebar.fonts.code_line_margin,
                syntax_dimming_factor: self
                    .config
                    .clibuddy
                    .right_sidebar
                    .fonts
                    .syntax_dimming_factor,
                width_correction_factor: self
                    .config
                    .clibuddy
                    .right_sidebar
                    .fonts
                    .width_correction_factor,
            };

            // Set the width correction factor before rendering modals
            set_width_correction_factor(fonts.width_correction_factor as f32);

            // Update modal animations and check if we need to redraw
            let needs_modal_redraw = ai_sidebar.modal_manager_mut().update_animation();
            if needs_modal_redraw {
                self.window.as_ref().unwrap().invalidate();
            }

            // Get modal elements
            let modal_elements =
                ai_sidebar.render_modals(&fonts, self.dimensions.pixel_height as f32);

            // Render each modal element
            for element in modal_elements {
                // Compute the element with proper context
                let mut computed = self.compute_element(
                    &LayoutContext {
                        width: DimensionContext {
                            dpi: self.dimensions.dpi as f32,
                            pixel_cell: self.render_metrics.cell_size.width as f32,
                            pixel_max: self.dimensions.pixel_width as f32,
                        },
                        height: DimensionContext {
                            dpi: self.dimensions.dpi as f32,
                            pixel_cell: self.render_metrics.cell_size.height as f32,
                            pixel_max: self.dimensions.pixel_height as f32,
                        },
                        bounds: euclid::rect(
                            0.0,
                            0.0,
                            self.dimensions.pixel_width as f32,
                            self.dimensions.pixel_height as f32,
                        ),
                        metrics: &self.render_metrics,
                        gl_state: self.render_state.as_ref().unwrap(),
                        zindex: 20, // Modal elements render at z-index 20+
                        source: RenderSource::Sidebar,
                    },
                    &element,
                )?;

                // No need to translate - modal positions are already absolute

                // Render the element
                let gl_state = self.render_state.as_ref().unwrap();
                self.render_element(&computed, gl_state, None)?;

                // Extract UI items for mouse handling
                self.ui_items.extend(computed.ui_items());
            }
        }

        Ok(())
    }

    /// Load synthetic font variants for sidebar markdown rendering
    fn load_sidebar_font_variants(
        &self,
        body_font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
    ) -> (
        Option<Rc<LoadedFont>>, // body_bold
        Option<Rc<LoadedFont>>, // body_italic
        Option<Rc<LoadedFont>>, // body_bold_italic
        Option<Rc<LoadedFont>>, // code_bold
        Option<Rc<LoadedFont>>, // code_italic
        Option<Rc<LoadedFont>>, // code_bold_italic
    ) {
        let body_style = body_font.style();
        let code_style = code_font.style();

        let body_bold = self.fonts.resolve_font(&body_style.make_bold()).ok();
        let body_italic = self.fonts.resolve_font(&body_style.make_italic()).ok();
        let body_bold_italic = self
            .fonts
            .resolve_font(&body_style.make_bold().make_italic())
            .ok();

        log::debug!(
            "Loaded sidebar font variants: bold={}, italic={}, bold_italic={}",
            body_bold.is_some(),
            body_italic.is_some(),
            body_bold_italic.is_some()
        );

        (
            body_bold,
            body_italic,
            body_bold_italic,
            self.fonts.resolve_font(&code_style.make_bold()).ok(),
            self.fonts.resolve_font(&code_style.make_italic()).ok(),
            self.fonts
                .resolve_font(&code_style.make_bold().make_italic())
                .ok(),
        )
    }

    /// Extract goal text positions from a computed element tree
    fn extract_goal_text_positions(
        &self,
        computed: &ComputedElement,
        ui_item_type: &UIItemType,
    ) -> Option<Vec<(f32, f32, usize)>> {
        // Check if this element has the matching UIItemType
        if let Some(item_type) = &computed.item_type {
            if matches!(item_type, UIItemType::GoalText { .. }) {
                // Extract positions from the content
                if let ComputedElementContent::MultilineText { lines, .. } = &computed.content {
                    let mut all_positions = Vec::new();
                    let mut last_cluster_seen = 0u32;
                    let mut line_byte_offset = 0usize;

                    // Process each line of text
                    log::debug!(
                        "GOAL POSITIONS DEBUG: Processing {} lines of text",
                        lines.len()
                    );
                    for (line_idx, cells) in lines.iter().enumerate() {
                        let mut x_pos = 0.0;
                        let mut line_has_clusters = false;

                        // First pass: check if this line has any clusters to detect line boundaries
                        for cell in cells {
                            if let ElementCell::GlyphWithCluster { cluster, .. } = cell {
                                line_has_clusters = true;
                                // If cluster resets to a lower value, we've started a new line
                                if *cluster < last_cluster_seen {
                                    // Add the previous line's max cluster + 1 to account for newline
                                    line_byte_offset += (last_cluster_seen + 1) as usize;
                                }
                                last_cluster_seen = *cluster;
                            }
                        }

                        if line_idx == 0 && line_has_clusters {
                            log::debug!(
                                "GOAL POSITIONS DEBUG: Line {} has {} cells, last_cluster_seen={}",
                                line_idx,
                                cells.len(),
                                last_cluster_seen
                            );
                        }

                        // Second pass: extract positions with proper byte offsets
                        x_pos = 0.0;
                        for cell in cells {
                            match cell {
                                ElementCell::Glyph(glyph) => {
                                    // Regular glyph without cluster info - just advance position
                                    if line_idx == 0 {
                                        log::warn!("GOAL POSITIONS DEBUG: Found regular Glyph (no cluster) at x_pos={}", x_pos);
                                    }
                                    x_pos += glyph.x_advance.get() as f32;
                                }
                                ElementCell::GlyphWithCluster { glyph, cluster } => {
                                    let x_start = x_pos;
                                    let x_end = x_pos + glyph.x_advance.get() as f32;

                                    // Calculate the absolute byte offset by adding line offset
                                    let byte_offset = line_byte_offset + *cluster as usize;
                                    all_positions.push((x_start, x_end, byte_offset));

                                    // Debug log for last few glyphs
                                    if line_idx == 0 && all_positions.len() >= 33 {
                                        log::debug!("GOAL POSITIONS DEBUG: Glyph {}: cluster={}, byte_offset={}, x_start={}, x_end={}", 
                                            all_positions.len() - 1, cluster, byte_offset, x_start, x_end);
                                    }

                                    x_pos = x_end;
                                }
                                _ => {} // Other cell types don't have position info
                            }
                        }
                    }

                    if !all_positions.is_empty() {
                        log::debug!(
                            "GOAL POSITIONS DEBUG: Extracted {} positions with cluster data",
                            all_positions.len()
                        );
                        // Log first few and last positions for debugging
                        if let Some(first) = all_positions.first() {}
                        if let Some(last) = all_positions.last() {
                            log::debug!("GOAL POSITIONS DEBUG: Last position: x_start={}, x_end={}, byte={}", 
                                last.0, last.1, last.2);
                        }
                        // Log last few positions to debug the missing last 2 chars
                        let len = all_positions.len();
                        if len >= 3 {
                            log::debug!("GOAL POSITIONS DEBUG: Last 3 positions:");
                            for i in (len - 3)..len {
                                let pos = &all_positions[i];
                                log::debug!(
                                    "  Position {}: x_start={}, x_end={}, byte={}",
                                    i,
                                    pos.0,
                                    pos.1,
                                    pos.2
                                );
                            }
                        }

                        // Log total cells to understand if we're missing some
                        let glyph_count = lines
                            .iter()
                            .flat_map(|line| line.iter())
                            .filter(|c| matches!(c, ElementCell::GlyphWithCluster { .. }))
                            .count();
                        log::debug!(
                            "GOAL POSITIONS DEBUG: Total GlyphWithCluster cells: {}",
                            glyph_count
                        );
                        return Some(all_positions);
                    } else {
                    }
                }
            }
        }

        // Recursively search children
        if let ComputedElementContent::Children(children) = &computed.content {
            for child in children {
                if let Some(positions) = self.extract_goal_text_positions(child, ui_item_type) {
                    return Some(positions);
                }
            }
        }

        None
    }
    
    /// Extract chat input positions from a computed element tree
    fn extract_chat_input_positions(
        &self,
        computed: &ComputedElement,
    ) -> Option<Vec<Vec<(f32, f32, usize)>>> {
        // Check if this element is the chat input
        if let Some(UIItemType::ChatInput { .. }) = &computed.item_type {
            // Extract positions from the content
            if let ComputedElementContent::MultilineText { lines, .. } = &computed.content {
                let mut line_positions = Vec::new();
                
                // Process each line of text
                for (line_idx, cells) in lines.iter().enumerate() {
                    let mut x_pos = 0.0;
                    let mut positions = Vec::new();
                    
                    for cell in cells {
                        match cell {
                            ElementCell::GlyphWithCluster { glyph, cluster } => {
                                let x_start = x_pos;
                                let x_end = x_pos + glyph.x_advance.get() as f32;
                                positions.push((x_start, x_end, *cluster as usize));
                                x_pos = x_end;
                            }
                            ElementCell::Glyph(glyph) => {
                                // Skip glyphs without cluster info
                                x_pos += glyph.x_advance.get() as f32;
                            }
                            ElementCell::Sprite(_sprite) => {
                                // Sprites are block drawing characters, use cell width
                                // For chat input, we might not have sprites, but handle just in case
                                x_pos += 8.0; // Approximate cell width, ideally get from context
                            }
                        }
                    }
                    
                    line_positions.push(positions);
                }
                
                log::debug!(
                    "Chat input position extraction: {} lines, first line has {} positions",
                    line_positions.len(),
                    line_positions.first().map(|l| l.len()).unwrap_or(0)
                );
                
                return Some(line_positions);
            }
        }
        
        // Recursively search children
        if let ComputedElementContent::Children(children) = &computed.content {
            for child in children {
                if let Some(positions) = self.extract_chat_input_positions(child) {
                    return Some(positions);
                }
            }
        }
        
        None
    }
}
