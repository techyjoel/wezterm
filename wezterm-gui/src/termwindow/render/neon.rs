//! Generic neon glow effect rendering system
//!
//! This module provides reusable neon-style rendering for UI elements like buttons,
//! borders, and dividers. The effect is achieved through GPU-accelerated blur
//! shaders for high performance. Requires WebGPU support for glow effects.

use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::renderstate::RenderContext;
use crate::termwindow::box_model::{
    BoxDimension, Element, ElementColors, ElementContent, VerticalAlign,
};
use crate::termwindow::render::blur::{BlurCacheKey, BlurRenderer};
use crate::termwindow::TermWindow;
use crate::utilsprites::RenderMetrics;
use anyhow::Result;
use config::Dimension;
use euclid::{Point2D, Size2D, Vector2D};
use std::rc::Rc;
use wezterm_font::LoadedFont;
use window::bitmaps::{TextureCoord, TextureRect};
use window::color::LinearRgba;
use window::{Point, PointF, Rect, RectF};

/// Configuration for neon glow effects
#[derive(Debug, Clone)]
pub struct NeonStyle {
    /// Primary neon color when "lit"
    pub neon_color: LinearRgba,
    /// Base color when "unlit" (dark glass-like appearance)
    pub base_color: LinearRgba,
    /// Glow intensity (0.0 = no glow, 1.0 = full glow)
    pub glow_intensity: f64,
    /// Maximum glow radius in pixels
    pub glow_radius: f32,
    /// Border width for the element
    pub border_width: f32,
    /// Whether element is currently "lit"
    pub is_active: bool,
}

impl Default for NeonStyle {
    fn default() -> Self {
        Self {
            neon_color: LinearRgba::with_components(0.0, 1.0, 1.0, 1.0), // Cyan
            base_color: LinearRgba::with_components(0.05, 0.05, 0.06, 1.0), // Dark gray
            glow_intensity: 1.0,                                         // Full intensity
            glow_radius: 8.0, // 8 pixels extension for subtle glow
            border_width: 2.0,
            is_active: false,
        }
    }
}

/// Trait for rendering neon effects
pub trait NeonRenderer {
    /// Render a neon rectangle (button, panel, etc.)
    fn render_neon_rect(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        bounds: RectF,
        style: &NeonStyle,
        corner_radius: Option<f32>,
    ) -> Result<()>;

    /// Render a neon line (border, divider)
    fn render_neon_line(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        start: PointF,
        end: PointF,
        style: &NeonStyle,
    ) -> Result<()>;

    /// Render neon text/icon with glow (legacy version for buttons)
    fn render_neon_glyph(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        position: PointF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
    ) -> Result<()>;

    /// Render neon glyph with explicit bounds and z-index
    fn render_neon_glyph_with_bounds_and_zindex(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        content_bounds: RectF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
        base_zindex: i8,
    ) -> Result<()>;

    /// Render neon text/icon with explicit bounds
    fn render_neon_glyph_with_bounds(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        content_bounds: RectF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
    ) -> Result<()>;

    /// Render debug visualization rectangles
    fn render_debug_rect(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        bounds: RectF,
        color: LinearRgba,
        label: &str,
    ) -> Result<()>;

    /// Render arbitrary neon text at a position (auto-sized)
    fn render_neon_text(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        position: PointF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
    ) -> Result<()>;
}

/// Helper function to blend two colors
fn blend_colors(a: LinearRgba, b: LinearRgba, factor: f32) -> LinearRgba {
    LinearRgba::with_components(
        a.0 * (1.0 - factor) + b.0 * factor,
        a.1 * (1.0 - factor) + b.1 * factor,
        a.2 * (1.0 - factor) + b.2 * factor,
        a.3 * (1.0 - factor) + b.3 * factor,
    )
}

/// Helper to create a color with modified alpha
fn with_alpha(color: LinearRgba, alpha: f32) -> LinearRgba {
    LinearRgba::with_components(color.0, color.1, color.2, alpha)
}

impl NeonRenderer for TermWindow {
    fn render_neon_rect(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        bounds: RectF,
        style: &NeonStyle,
        _corner_radius: Option<f32>,
    ) -> Result<()> {
        // Layer 1 for glow effects (behind), layer 2 for button background/border
        let _glow_layer = 1;
        let button_layer = 2;

        // Render base button background - always keep it dark
        let base_color = style.base_color;
        self.filled_rectangle(layers, button_layer, bounds, base_color)?;

        // NO border rendering - we want clean buttons without glowing borders

        Ok(())
    }

    fn render_neon_line(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        start: PointF,
        end: PointF,
        style: &NeonStyle,
    ) -> Result<()> {
        // Calculate line dimensions
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt();

        // For now, we'll render horizontal/vertical lines as rectangles
        // TODO: Add support for diagonal lines using rotated quads

        if dx.abs() > dy.abs() {
            // Mostly horizontal line
            let y_center = (start.y + end.y) / 2.0;
            let line_bounds = euclid::rect(
                start.x.min(end.x),
                y_center - style.border_width / 2.0,
                length,
                style.border_width,
            );

            // Apply same glow effect as rectangles
            let line_style = NeonStyle {
                border_width: 0.0, // No additional border on the line itself
                ..style.clone()
            };

            self.render_neon_rect(layers, line_bounds, &line_style, None)?;
        } else {
            // Mostly vertical line
            let x_center = (start.x + end.x) / 2.0;
            let line_bounds = euclid::rect(
                x_center - style.border_width / 2.0,
                start.y.min(end.y),
                style.border_width,
                length,
            );

            let line_style = NeonStyle {
                border_width: 0.0,
                ..style.clone()
            };

            self.render_neon_rect(layers, line_bounds, &line_style, None)?;
        }

        Ok(())
    }

    fn render_neon_glyph(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        position: PointF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
    ) -> Result<()> {
        // This function currently assumes a 40x40 button size for backward compatibility
        // TODO: Make this more generic by passing in content bounds
        let content_bounds = euclid::rect(position.x, position.y, 40.0, 40.0);
        // Use z-index 2 as default (assumes button is at z-index 1)
        self.render_neon_glyph_with_bounds_and_zindex(layers, text, content_bounds, font, style, 2)
    }

    /// Generic version that accepts explicit content bounds
    fn render_neon_glyph_with_bounds(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        content_bounds: RectF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
    ) -> Result<()> {
        // Use z-index 2 as default (assumes button is at z-index 1)
        self.render_neon_glyph_with_bounds_and_zindex(layers, text, content_bounds, font, style, 2)
    }

    /// Full version that accepts bounds and z-index offset
    fn render_neon_glyph_with_bounds_and_zindex(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        content_bounds: RectF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
        base_zindex: i8,
    ) -> Result<()> {
        // Debug logging commented out for performance
        // log::debug!(
        //     "render_neon_glyph_with_bounds called for '{}' at {:?}, is_active={}, glow_intensity={}",
        //     text,
        //     content_bounds,
        //     style.is_active,
        //     style.glow_intensity
        // );

        // Enable debug visualization with environment variable
        let debug_viz = std::env::var("WEZTERM_DEBUG_GLOW_POS").is_ok();

        if debug_viz {
            // Draw the content bounds in red
            self.render_debug_rect(
                layers,
                content_bounds,
                LinearRgba::with_components(1.0, 0.0, 0.0, 1.0),
                "content_bounds",
            )?;
        }

        // Shape the text to get glyph information including bearing values
        let infos = font.shape(
            text,
            || {},
            |_| {},
            None,
            wezterm_font::shaper::Direction::LeftToRight,
            None,
            None,
        )?;

        // Get bearing values from the first glyph
        let (_bearing_x, _bearing_y) = if let Some(info) = infos.first() {
            match font.rasterize_glyph(info.glyph_pos, info.font_idx) {
                Ok(glyph) => {
                    log::trace!(
                        "Glyph '{}' bearing values: x={:?} ({}), y={:?} ({})",
                        text,
                        glyph.bearing_x,
                        glyph.bearing_x.get(),
                        glyph.bearing_y,
                        glyph.bearing_y.get()
                    );
                    (glyph.bearing_x.get() as f32, glyph.bearing_y.get() as f32)
                }
                Err(e) => {
                    log::debug!("Failed to get bearing values for glyph '{}': {}", text, e);
                    (0.0, 0.0)
                }
            }
        } else {
            log::debug!("No glyph info for '{}', using default bearings", text);
            (0.0, 0.0)
        };

        let metrics = RenderMetrics::with_font_metrics(&font.metrics());

        // Store debug visualization info outside the borrow scopes
        let mut debug_glow_bounds = None;
        let mut debug_center_bounds = None;

        // Render glow effect first (behind the icon) when active
        if style.is_active && style.glow_intensity > 0.0 {
            // Check if we can use GPU blur via overlay system
            // Both WebGPU and OpenGL backends support GPU blur
            let can_use_gpu = self
                .render_state
                .as_ref()
                .map(|rs| {
                    matches!(
                        &rs.context,
                        RenderContext::WebGpu(_) | RenderContext::Glium(_)
                    )
                })
                .unwrap_or(false);

            // Debug logging commented out for performance
            // log::debug!("can_use_gpu={}, effects_overlay={}, blur_renderer={}",
            //     can_use_gpu,
            //     self.effects_overlay.borrow().is_some(),
            //     self.blur_renderer.borrow().is_some()
            // );

            if can_use_gpu
                && self.effects_overlay.borrow().is_some()
                && self.blur_renderer.borrow().is_some()
            {
                log::debug!("Using GPU blur for neon glow effect");
                // Use GPU-accelerated blur via effects overlay
                match self.create_icon_texture(
                    text,
                    font,
                    style.neon_color,
                    content_bounds.width().max(content_bounds.height()) as u32,
                    style.glow_radius as u32,
                ) {
                    Ok(icon_texture) => {
                        // Apply GPU blur
                        let cache_key = BlurCacheKey {
                            content_hash:
                                crate::termwindow::render::blur::BlurRenderer::compute_content_hash(
                                    text.as_bytes(),
                                ),
                            radius: style.glow_radius as u32,
                            // Use the actual texture dimensions for the cache key
                            width: icon_texture.width() as u32,
                            height: icon_texture.height() as u32,
                        };

                        let render_context = self.render_state.as_ref().unwrap().context.clone();

                        if let Some(blur_renderer) = self.blur_renderer.borrow_mut().as_mut() {
                            match blur_renderer.apply_blur(
                                &*icon_texture,
                                style.glow_radius,
                                Some(cache_key.clone()),
                                &render_context,
                            ) {
                                Ok(blurred_texture) => {
                                    // Debug logging commented out for performance
                                    // log::debug!("Blur succeeded, got texture {}x{}",
                                    //     blurred_texture.width(), blurred_texture.height());

                                    // Add glow effect to overlay with pre-blurred texture
                                    if let Some(ref mut overlay) =
                                        self.effects_overlay.borrow_mut().as_mut()
                                    {
                                        // Calculate where to position the glow texture in window coordinates
                                        // The blurred texture is larger than the original icon by blur radius on each side
                                        // We need to offset by half the difference to center it on the icon
                                        let texture_width = blurred_texture.width() as isize;
                                        let texture_height = blurred_texture.height() as isize;

                                        // For proper positioning, we need to center the glow on the actual content
                                        // The Element system computes smaller bounds (32x38) than what we pass in (40x40)
                                        // This causes the icon to be rendered at a different position than expected
                                        // Based on debug output, the computed bounds are typically 32x38 for a 40x40 button
                                        // This means the icon is 4 pixels narrower and 2 pixels shorter
                                        // Since the icon is left-aligned, we need to adjust the center calculation
                                        let element_width_reduction = 8.0; // 40 - 32 = 8
                                        let element_height_reduction = -2.0; // fix vertical offset

                                        // Adjust the content center to account for the actual rendered position
                                        let content_center_x = content_bounds.min_x()
                                            + (content_bounds.width() - element_width_reduction)
                                                / 2.0;
                                        let content_center_y = content_bounds.min_y()
                                            + (content_bounds.height() - element_height_reduction)
                                                / 2.0;

                                        // Position the glow texture so its center aligns with the content center
                                        let offset_correction = 0;
                                        let glow_window_x = content_center_x as isize
                                            - texture_width / 2
                                            + offset_correction;
                                        let glow_window_y = content_center_y as isize
                                            - texture_height / 2
                                            + offset_correction;

                                        log::trace!(
                                            "Glow position: content_bounds={:?}, center ({:.1}, {:.1}), texture {}x{}, glow at ({}, {})",
                                            content_bounds,
                                            content_center_x, content_center_y,
                                            texture_width, texture_height,
                                            glow_window_x, glow_window_y
                                        );

                                        if debug_viz {
                                            // Store debug bounds to render later
                                            debug_glow_bounds = Some(euclid::rect(
                                                glow_window_x as f32,
                                                glow_window_y as f32,
                                                texture_width as f32,
                                                texture_height as f32,
                                            ));

                                            let center_marker_size = 6.0;
                                            debug_center_bounds = Some(euclid::rect(
                                                content_center_x - center_marker_size / 2.0,
                                                content_center_y - center_marker_size / 2.0,
                                                center_marker_size,
                                                center_marker_size,
                                            ));
                                        }

                                        overlay.add_glow(crate::termwindow::render::effects_overlay::GlowEffect {
                                            texture: blurred_texture,
                                            window_position: euclid::point2(glow_window_x, glow_window_y),
                                            intensity: (style.glow_intensity * 0.8) as f32, // Use 80% intensity for strongest visible glow
                                        });
                                    }
                                }
                                Err(e) => {
                                    log::debug!("GPU blur failed, skipping glow: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::debug!("Failed to create icon texture: {}", e);
                    }
                }

                // Render debug visualization after borrowing is done
                if let Some(glow_bounds) = debug_glow_bounds {
                    self.render_debug_rect(
                        layers,
                        glow_bounds,
                        LinearRgba::with_components(0.0, 1.0, 0.0, 1.0),
                        "glow_bounds",
                    )?;
                }
                if let Some(center_bounds) = debug_center_bounds {
                    self.render_debug_rect(
                        layers,
                        center_bounds,
                        LinearRgba::with_components(1.0, 1.0, 0.0, 1.0),
                        "content_center",
                    )?;
                }
            } else {
                // No GPU support - skip glow effects entirely
            }
        }

        // Determine main icon color based on active state
        let icon_color = if style.is_active {
            // Use the neon color when active
            style.neon_color
        } else {
            // Use a visible gray when inactive
            LinearRgba::with_components(0.7, 0.7, 0.7, 1.0)
        };

        // Create the main icon element with proper vertical alignment
        let icon_element = Element::new(font, ElementContent::Text(text.to_string()))
            .vertical_align(VerticalAlign::Middle)
            .colors(ElementColors {
                border: crate::termwindow::box_model::BorderColor::default(),
                bg: LinearRgba::TRANSPARENT.into(),
                text: icon_color.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Pixels(content_bounds.width() * 0.01),
                right: Dimension::Pixels(content_bounds.width() * 0.01),
                top: Dimension::Pixels(content_bounds.height() * 0.01),
                bottom: Dimension::Pixels(content_bounds.height() * 0.01),
            });

        // Create layout context for main icon
        // The zindex should be 0 since we're already rendering within the correct z-layer
        let context = crate::termwindow::box_model::LayoutContext {
            width: config::DimensionContext {
                dpi: self.dimensions.dpi as f32,
                pixel_max: self.dimensions.pixel_width as f32,
                pixel_cell: metrics.cell_size.width as f32,
            },
            height: config::DimensionContext {
                dpi: self.dimensions.dpi as f32,
                pixel_max: self.dimensions.pixel_height as f32,
                pixel_cell: metrics.cell_size.height as f32,
            },
            bounds: content_bounds,
            metrics: &metrics,
            gl_state: self.render_state.as_ref().unwrap(),
            zindex: base_zindex + 1, // Render icon 1 level above the base to avoid conflicts
        };

        // Compute the element layout
        let computed = self.compute_element(&context, &icon_element)?;

        // Log the computed bounds for debugging
        log::trace!(
            "Element bounds: content_bounds={:?}, computed.bounds={:?}",
            content_bounds,
            computed.bounds
        );

        if debug_viz {
            // Draw the computed element bounds in blue to see where it actually renders
            let elem_bounds = computed.bounds;
            self.render_debug_rect(
                layers,
                elem_bounds,
                LinearRgba::with_components(0.0, 0.0, 1.0, 1.0),
                "computed_element_bounds",
            )?;
        }

        // Render the computed element
        self.render_element(&computed, self.render_state.as_ref().unwrap(), None)?;

        Ok(())
    }

    fn render_neon_text(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        text: &str,
        position: PointF,
        font: &Rc<LoadedFont>,
        style: &NeonStyle,
    ) -> Result<()> {
        // For arbitrary text, we need to measure it first
        let _metrics = RenderMetrics::with_font_metrics(&font.metrics());

        // Shape the text to get its dimensions
        let infos = font.shape(
            text,
            || {},
            |_| {},
            None,
            wezterm_font::shaper::Direction::LeftToRight,
            None,
            None,
        )?;

        // Calculate text bounds
        let mut width = 0.0;
        let mut max_height: f32 = 0.0;
        for info in &infos {
            width += info.x_advance.get() as f32;
            if let Ok(glyph) = font.rasterize_glyph(info.glyph_pos, info.font_idx) {
                let height = glyph.height as f32 + glyph.bearing_y.get() as f32;
                max_height = max_height.max(height);
            }
        }

        // Create bounds for the text with some padding for the glow
        let padding = 4.0; // Small padding
        let text_bounds = euclid::rect(
            position.x - padding,
            position.y - padding,
            width + padding * 2.0,
            max_height + padding * 2.0,
        );

        // Use the generic bounds version
        self.render_neon_glyph_with_bounds(layers, text, text_bounds, font, style)
    }

    fn render_debug_rect(
        &mut self,
        layers: &mut TripleLayerQuadAllocator,
        bounds: RectF,
        color: LinearRgba,
        label: &str,
    ) -> Result<()> {
        log::trace!(
            "Debug rect: {} at {:?} with color {:?}",
            label,
            bounds,
            color
        );

        // Draw a semi-transparent filled rectangle for visualization
        let debug_color = LinearRgba::with_components(color.0, color.1, color.2, 0.3);
        self.filled_rectangle(layers, 2, bounds, debug_color)?;

        // Draw a solid border
        let border_width = 1.0;
        // Top border
        self.filled_rectangle(
            layers,
            2,
            euclid::rect(bounds.min_x(), bounds.min_y(), bounds.width(), border_width),
            color,
        )?;
        // Bottom border
        self.filled_rectangle(
            layers,
            2,
            euclid::rect(
                bounds.min_x(),
                bounds.max_y() - border_width,
                bounds.width(),
                border_width,
            ),
            color,
        )?;
        // Left border
        self.filled_rectangle(
            layers,
            2,
            euclid::rect(
                bounds.min_x(),
                bounds.min_y(),
                border_width,
                bounds.height(),
            ),
            color,
        )?;
        // Right border
        self.filled_rectangle(
            layers,
            2,
            euclid::rect(
                bounds.max_x() - border_width,
                bounds.min_y(),
                border_width,
                bounds.height(),
            ),
            color,
        )?;

        Ok(())
    }
}

/// Helper to create NeonStyle from configuration
impl NeonStyle {
    pub fn from_config(
        neon_color: LinearRgba,
        base_color: LinearRgba,
        glow_intensity: Option<f64>,
        glow_radius: Option<f32>,
        border_width: Option<f32>,
        is_active: bool,
    ) -> Self {
        Self {
            neon_color,
            base_color,
            glow_intensity: glow_intensity.unwrap_or(1.0), // Default to full intensity
            glow_radius: glow_radius.unwrap_or(20.0),      // 20 pixel extension
            border_width: border_width.unwrap_or(2.0),
            is_active,
        }
    }
}
