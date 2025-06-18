//! Generic neon glow effect rendering system
//!
//! This module provides reusable neon-style rendering for UI elements like buttons,
//! borders, and dividers. The effect is achieved through layered gradients that
//! simulate the glow of neon lights.

use crate::quad::{QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::termwindow::box_model::{
    BoxDimension, Element, ElementColors, ElementContent, VerticalAlign,
};
use crate::termwindow::TermWindow;
use crate::utilsprites::RenderMetrics;
use anyhow::Result;
use config::Dimension;
use euclid::{Point2D, Size2D, Vector2D};
use std::rc::Rc;
use wezterm_font::LoadedFont;
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
            glow_intensity: 0.8,
            glow_layers: 5,
            glow_radius: 12.0,
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
        // Use layer 1 for glow effects, layer 2 for the button itself
        let glow_layer = 1;
        let button_layer = 2;

        if style.is_active && style.glow_intensity > 0.0 {
            // Render glow layers from outside in
            for i in 0..style.glow_layers {
                let layer_idx = (style.glow_layers - 1 - i) as f32;
                let glow_expansion =
                    (layer_idx + 1.0) * style.glow_radius / style.glow_layers as f32;
                let glow_alpha = (style.glow_intensity * (0.3 / (layer_idx + 1.0) as f64)) as f32;

                // Expanded bounds for this glow layer
                let glow_bounds = euclid::rect(
                    bounds.min_x() - glow_expansion,
                    bounds.min_y() - glow_expansion,
                    bounds.width() + glow_expansion * 2.0,
                    bounds.height() + glow_expansion * 2.0,
                );

                // Render glow with fading alpha
                let glow_color = with_alpha(style.neon_color, glow_alpha);
                self.filled_rectangle(layers, glow_layer, glow_bounds, glow_color)?;
            }
        }

        // Render base element
        let base_color = if style.is_active {
            // Lit: blend neon color with base for vibrant core
            blend_colors(style.base_color, style.neon_color, 0.3)
        } else {
            // Unlit: dark glass-like appearance
            style.base_color
        };

        self.filled_rectangle(layers, button_layer, bounds, base_color)?;

        // Render border with inner glow
        if style.border_width > 0.0 {
            let border_color = if style.is_active {
                style.neon_color
            } else {
                // Dim hint of color when unlit
                with_alpha(style.neon_color, 0.2)
            };

            // Render border outline - use layer 2 (max layer)
            render_rect_outline(
                self,
                layers,
                2,
                bounds,
                style.border_width,
                border_color,
            )?;

            // Inner glow on border when active
            if style.is_active && style.glow_intensity > 0.0 {
                let inner_inset = style.border_width + 1.0;
                let inner_bounds = euclid::rect(
                    bounds.min_x() + inner_inset,
                    bounds.min_y() + inner_inset,
                    bounds.width() - inner_inset * 2.0,
                    bounds.height() - inner_inset * 2.0,
                );
                let inner_color = with_alpha(style.neon_color, (0.2 * style.glow_intensity) as f32);
                self.filled_rectangle(layers, 2, inner_bounds, inner_color)?;
            }
        }

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

        // Calculate text bounds for centering
        let text_width = metrics.cell_size.width as f32 * text.chars().count() as f32;
        let text_height = metrics.cell_size.height as f32;

        // Render glow layers if active
        if style.is_active && style.glow_intensity > 0.0 {
            for i in 0..style.glow_layers {
                let layer_idx = (style.glow_layers - 1 - i) as f32;
                let glow_alpha = (style.glow_intensity * (0.5 / (layer_idx + 1.0) as f64)) as f32;
                let glow_color = with_alpha(style.neon_color, glow_alpha);

                // Create text element with glow color
                let glow_element = Element::new(font, ElementContent::Text(text.to_string()))
                    .colors(ElementColors {
                        border: crate::termwindow::box_model::BorderColor::default(),
                        bg: LinearRgba::TRANSPARENT.into(),
                        text: glow_color.into(),
                    });

                // Slight offset for each glow layer to create blur effect
                let offset = layer_idx * 0.5;
                let glow_bounds = RectF::new(
                    euclid::point2(position.x - offset, position.y - offset),
                    euclid::size2(text_width + offset * 2.0, text_height + offset * 2.0),
                );

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
                    bounds: glow_bounds,
                    metrics: &metrics,
                    gl_state: self.render_state.as_ref().unwrap(),
                    zindex: 3,
                };

                let computed = self.compute_element(&context, &glow_element)?;
                self.render_element(&computed, self.render_state.as_ref().unwrap(), None)?;
            }
        }

        // Render the main text
        let text_color = if style.is_active {
            // Bright white/neon color when active
            LinearRgba::with_components(1.0, 1.0, 1.0, 1.0)
        } else {
            // Dim gray when inactive
            with_alpha(style.neon_color, 0.3)
        };

        let text_element =
            Element::new(font, ElementContent::Text(text.to_string())).colors(ElementColors {
                border: crate::termwindow::box_model::BorderColor::default(),
                bg: LinearRgba::TRANSPARENT.into(),
                text: text_color.into(),
            });

        let text_bounds = RectF::new(
            euclid::point2(position.x, position.y),
            euclid::size2(text_width, text_height),
        );

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
            bounds: text_bounds,
            metrics: &metrics,
            gl_state: self.render_state.as_ref().unwrap(),
            zindex: 4, // Above glow layers
        };

        let computed = self.compute_element(&context, &text_element)?;
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
            glow_intensity: glow_intensity.unwrap_or(0.8),
            glow_layers: glow_layers.unwrap_or(5),
            glow_radius: glow_radius.unwrap_or(12.0),
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
