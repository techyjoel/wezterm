//! Generic neon glow effect rendering system
//!
//! This module provides reusable neon-style rendering for UI elements like buttons,
//! borders, and dividers. The effect is achieved through layered gradients that
//! simulate the glow of neon lights.

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
    /// Number of glow layers (more = smoother but more expensive)
    pub glow_layers: u8,
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
            glow_intensity: 1.0, // Full intensity (will be multiplied by 0.08 for 8% brightness)
            glow_layers: 5,      // 5 layers worked well
            glow_radius: 20.0,   // 20 pixels extension worked well
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

    /// Render neon text/icon with glow
    fn render_neon_glyph(
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

/// Helper function to create a rect outline as a series of filled rectangles
fn render_rect_outline(
    term_window: &mut TermWindow,
    layers: &mut TripleLayerQuadAllocator,
    layer: usize,
    bounds: RectF,
    width: f32,
    color: LinearRgba,
) -> Result<()> {
    // Top border
    term_window.filled_rectangle(
        layers,
        layer,
        euclid::rect(bounds.min_x(), bounds.min_y(), bounds.width(), width),
        color,
    )?;

    // Bottom border
    term_window.filled_rectangle(
        layers,
        layer,
        euclid::rect(
            bounds.min_x(),
            bounds.max_y() - width,
            bounds.width(),
            width,
        ),
        color,
    )?;

    // Left border
    term_window.filled_rectangle(
        layers,
        layer,
        euclid::rect(bounds.min_x(), bounds.min_y(), width, bounds.height()),
        color,
    )?;

    // Right border
    term_window.filled_rectangle(
        layers,
        layer,
        euclid::rect(
            bounds.max_x() - width,
            bounds.min_y(),
            width,
            bounds.height(),
        ),
        color,
    )?;

    Ok(())
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
        let metrics = RenderMetrics::with_font_metrics(&font.metrics());

        // Use 40x40 button size like in the working version
        let button_size = 40.0;

        // Create icon bounds using the full button area
        let icon_bounds = RectF::new(
            euclid::point2(position.x, position.y),
            euclid::size2(button_size, button_size),
        );

        // Render glow effect first (behind the icon) when active
        if style.is_active && style.glow_intensity > 0.0 {
            // Check if we can use GPU blur via overlay system
            let can_use_gpu = self
                .render_state
                .as_ref()
                .map(|rs| matches!(&rs.context, RenderContext::WebGpu(_)))
                .unwrap_or(false);

            if can_use_gpu
                && self.effects_overlay.borrow().is_some()
                && self.blur_renderer.borrow().is_some()
            {
                // Use GPU-accelerated blur via effects overlay
                log::debug!(
                    "Using GPU blur overlay for '{}' with radius {}",
                    text,
                    style.glow_radius
                );

                // Create and blur the icon texture
                match self.create_icon_texture(
                    text,
                    font,
                    style.neon_color,
                    button_size as u32,
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
                            width: (button_size + style.glow_radius * 2.0) as u32,
                            height: (button_size + style.glow_radius * 2.0) as u32,
                        };

                        let render_context = self.render_state.as_ref().unwrap().context.clone();
                        if let Some(blur_renderer) = self.blur_renderer.borrow_mut().as_mut() {
                            match blur_renderer.apply_blur(
                                &*icon_texture,
                                style.glow_radius,
                                Some(cache_key),
                                &render_context,
                            ) {
                                Ok(blurred_texture) => {
                                    // Add glow effect to overlay with pre-blurred texture
                                    if let Some(ref mut overlay) =
                                        self.effects_overlay.borrow_mut().as_mut()
                                    {
                                        overlay.add_glow(crate::termwindow::render::effects_overlay::GlowEffect {
                                            texture: blurred_texture,
                                            position: euclid::point2(position.x as isize, position.y as isize),
                                            intensity: (style.glow_intensity * 0.08) as f32, // Match CPU's 8% brightness
                                        });
                                    }
                                    log::info!("✓ GPU blur successfully applied for '{}'", text);
                                }
                                Err(e) => {
                                    log::debug!("GPU blur failed, falling back to CPU: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        log::debug!("Failed to create icon texture: {}", e);
                    }
                }
            } else {
                // Use the baseline multi-pass approach
                // This is the slow 240-pass method

                let glow_radius = style.glow_radius;
                let base_alpha = 0.08 * style.glow_intensity;

                let rings = style.glow_layers.min(10) as usize;

                let mut samples_per_ring = vec![0];
                for i in 1..rings {
                    samples_per_ring.push((i * 6).min(24));
                }

                // Log multi-pass rendering details
                let total_passes: usize = samples_per_ring.iter().sum();
                log::debug!(
                    "CPU multi-pass glow for '{}': {} rings, {} total passes",
                    text,
                    rings,
                    total_passes
                );

                for ring in 1..=rings {
                    let ring_radius = (ring as f32 / rings as f32) * glow_radius;
                    let samples = samples_per_ring.get(ring - 1).copied().unwrap_or(0);

                    let falloff = 1.0 - (ring as f32 - 1.0) / rings as f32;
                    let ring_alpha = base_alpha * falloff as f64 * 0.7;

                    if samples == 0 {
                        continue;
                    }

                    for sample in 0..samples {
                        let angle = (sample as f32 / samples as f32) * std::f32::consts::PI * 2.0;
                        let x_offset = angle.cos() * ring_radius;
                        let y_offset = angle.sin() * ring_radius;

                        let glow_alpha = ring_alpha as f32;
                        let glow_color = with_alpha(style.neon_color, glow_alpha);

                        let glow_bounds = RectF::new(
                            euclid::point2(position.x + x_offset, position.y + y_offset),
                            euclid::size2(button_size, button_size),
                        );

                        let glow_element =
                            Element::new(font, ElementContent::Text(text.to_string()))
                                .vertical_align(VerticalAlign::Middle)
                                .colors(ElementColors {
                                    border: crate::termwindow::box_model::BorderColor::default(),
                                    bg: LinearRgba::TRANSPARENT.into(),
                                    text: glow_color.into(),
                                })
                                .padding(BoxDimension {
                                    left: Dimension::Pixels(button_size * 0.01),
                                    right: Dimension::Pixels(button_size * 0.01),
                                    top: Dimension::Pixels(button_size * 0.01),
                                    bottom: Dimension::Pixels(button_size * 0.01),
                                });

                        let glow_context = crate::termwindow::box_model::LayoutContext {
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
                            bounds: glow_bounds,
                            metrics: &metrics,
                            gl_state: self.render_state.as_ref().unwrap(),
                            zindex: 2,
                        };

                        let computed = self.compute_element(&glow_context, &glow_element)?;
                        self.render_element(&computed, self.render_state.as_ref().unwrap(), None)?;
                    }
                }
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
                left: Dimension::Pixels(button_size * 0.01),
                right: Dimension::Pixels(button_size * 0.01),
                top: Dimension::Pixels(button_size * 0.01),
                bottom: Dimension::Pixels(button_size * 0.01),
            });

        // Create layout context for main icon
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
            bounds: icon_bounds,
            metrics: &metrics,
            gl_state: self.render_state.as_ref().unwrap(),
            zindex: 3, // Render above glow
        };

        // Compute the element layout
        let computed = self.compute_element(&context, &icon_element)?;

        // Render the computed element
        self.render_element(&computed, self.render_state.as_ref().unwrap(), None)?;

        Ok(())
    }
}

/// Helper to create NeonStyle from configuration
impl NeonStyle {
    pub fn from_config(
        neon_color: LinearRgba,
        base_color: LinearRgba,
        glow_intensity: Option<f64>,
        glow_layers: Option<u8>,
        glow_radius: Option<f32>,
        border_width: Option<f32>,
        is_active: bool,
    ) -> Self {
        Self {
            neon_color,
            base_color,
            glow_intensity: glow_intensity.unwrap_or(1.0), // Default to full intensity
            glow_layers: glow_layers.unwrap_or(5),         // 5 layers for smooth glow
            glow_radius: glow_radius.unwrap_or(20.0),      // 20 pixel extension
            border_width: border_width.unwrap_or(2.0),
            is_active,
        }
    }

    /// Update the active state with optional transition
    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }

    /// Create a dimmed version for hover states
    pub fn with_hover(&self, hover_intensity: f64) -> Self {
        Self {
            glow_intensity: self.glow_intensity * hover_intensity,
            ..self.clone()
        }
    }
}
