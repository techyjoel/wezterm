//! Element-based scrollbar component for self-contained rendering
//!
//! This module provides a scrollbar component that renders using the Element system,
//! suitable for components that already use Elements and need integrated scrollbars.

use std::rc::Rc;

use crate::termwindow::box_model::*;
use crate::termwindow::UIItemType;
use ::window::color::LinearRgba;
use config::Dimension;
use wezterm_font::LoadedFont;
use window::{PixelUnit, RectF};

use super::{ScrollMetrics, ScrollbarColors, ScrollbarConfig, ScrollbarState, ScrollbarStyle};

/// A scrollbar component that renders as Elements
#[derive(Debug)]
pub struct ScrollbarElement {
    state: ScrollbarState,
    config: ScrollbarConfig,
    style: ScrollbarStyle,
    metrics: ScrollMetrics,
}

impl ScrollbarElement {
    /// Create a new scrollbar element with default styling
    pub fn new() -> Self {
        Self {
            state: ScrollbarState::new(),
            config: ScrollbarConfig::default(),
            style: ScrollbarStyle::modal(), // Default to modal style
            metrics: ScrollMetrics::new(0.0, 0.0, 0.0),
        }
    }

    /// Create with custom style
    pub fn with_style(style: ScrollbarStyle) -> Self {
        Self {
            state: ScrollbarState::new(),
            config: ScrollbarConfig::default(),
            style,
            metrics: ScrollMetrics::new(0.0, 0.0, 0.0),
        }
    }

    /// Update the scrollbar dimensions
    pub fn set_dimensions(&mut self, content_height: f32, viewport_height: f32) {
        self.state.set_dimensions(content_height, viewport_height);
        self.metrics =
            ScrollMetrics::new(content_height, viewport_height, self.state.scroll_offset);
    }

    /// Get current scroll offset
    pub fn scroll_offset(&self) -> f32 {
        self.state.scroll_offset
    }

    /// Set scroll offset
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.state.set_scroll_offset(offset);
        self.metrics.set_scroll_offset(offset);
    }

    /// Handle mouse wheel event
    pub fn handle_wheel(&mut self, delta: f32, lines_per_notch: f32) -> bool {
        let changed = self.state.handle_wheel(delta, lines_per_notch);
        if changed {
            self.metrics.set_scroll_offset(self.state.scroll_offset);
        }
        changed
    }

    /// Check if point is over scrollbar
    pub fn hit_test(&self, point: euclid::Point2D<f32, PixelUnit>, bounds: RectF) -> bool {
        if !self.state.is_needed() {
            return false;
        }

        let scrollbar_bounds = self.calculate_bounds(bounds);
        scrollbar_bounds.contains(point)
    }

    /// Start drag at given position
    pub fn start_drag(&mut self, y_position: f32) {
        self.state.start_drag(y_position);
    }

    /// Update drag position
    pub fn update_drag(&mut self, y_position: f32, scrollbar_height: f32) {
        self.state.update_drag(y_position, scrollbar_height);
        self.metrics.set_scroll_offset(self.state.scroll_offset);
    }

    /// End drag
    pub fn end_drag(&mut self) {
        self.state.end_drag();
    }

    /// Set hover state
    pub fn set_hovering(&mut self, hovering: bool) {
        self.state.set_hovering(hovering);
    }

    /// Update animations
    pub fn update_animation(&mut self) -> bool {
        self.state.update_animation(&self.config)
    }

    /// Calculate scrollbar bounds within container
    fn calculate_bounds(&self, container: RectF) -> RectF {
        let width = self.style.dimensions.width;
        let padding = self.style.dimensions.edge_padding;

        euclid::rect(
            container.max_x() - width - padding,
            container.min_y(),
            width,
            container.size.height,
        )
    }

    /// Render the scrollbar as Elements
    pub fn render(&self, font: &Rc<LoadedFont>, bounds: RectF, z_index: i8) -> Vec<Element> {
        let mut elements = vec![];

        // Check if scrollbar should be visible
        if !self.state.is_needed() {
            return elements;
        }

        let opacity = self.state.get_opacity(&self.config);
        if opacity <= 0.0 && self.config.auto_hide {
            return elements;
        }

        // Calculate scrollbar position and dimensions
        let scrollbar_bounds = self.calculate_bounds(bounds);
        let scrollbar_x = scrollbar_bounds.min_x();
        let scrollbar_y = scrollbar_bounds.min_y();
        let scrollbar_width = scrollbar_bounds.size.width;
        let scrollbar_height = scrollbar_bounds.size.height;

        // Get thumb geometry
        let thumb_geometry = self.state.calculate_thumb_geometry(scrollbar_height);
        let thumb_y = scrollbar_y + thumb_geometry.y_offset;

        // Get colors based on state
        let thumb_color = self
            .style
            .colors
            .get_thumb_color(self.state.is_hovering, self.state.is_dragging);
        let track_opacity = self.style.colors.get_track_opacity(self.state.is_hovering);

        // Apply overall opacity for auto-hide
        let thumb_color = thumb_color.mul_alpha(opacity);
        let track_color = if let Some(bg) = self.style.colors.track_bg {
            bg.mul_alpha(opacity * track_opacity)
        } else {
            LinearRgba(0.2, 0.2, 0.2, opacity * track_opacity)
        };

        // Create track background
        let track_element = Element::new(font, ElementContent::Text(String::new()))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: track_color.into(),
                text: LinearRgba(1.0, 1.0, 1.0, 1.0).into(),
            })
            .min_width(Some(Dimension::Pixels(scrollbar_width)))
            .min_height(Some(Dimension::Pixels(scrollbar_height)))
            .margin(BoxDimension {
                left: Dimension::Pixels(scrollbar_x),
                top: Dimension::Pixels(scrollbar_y),
                right: Dimension::Pixels(0.0),
                bottom: Dimension::Pixels(0.0),
            })
            .display(DisplayType::Block)
            .zindex(z_index);

        elements.push(track_element);

        // Create thumb
        let thumb_element = Element::new(font, ElementContent::Text(String::new()))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: thumb_color.into(),
                text: LinearRgba(1.0, 1.0, 1.0, 1.0).into(),
            })
            .min_width(Some(Dimension::Pixels(scrollbar_width)))
            .min_height(Some(Dimension::Pixels(thumb_geometry.height)))
            .margin(BoxDimension {
                left: Dimension::Pixels(scrollbar_x),
                top: Dimension::Pixels(thumb_y),
                right: Dimension::Pixels(0.0),
                bottom: Dimension::Pixels(0.0),
            })
            .display(DisplayType::Block)
            .zindex(z_index);

        elements.push(thumb_element);

        elements
    }

    /// Render with UIItemType for automatic event handling
    pub fn render_with_item_type(
        &self,
        font: &Rc<LoadedFont>,
        bounds: RectF,
        z_index: i8,
        item_type: UIItemType,
    ) -> Vec<Element> {
        let mut elements = self.render(font, bounds, z_index);

        // Add UIItemType to all elements for hit testing
        for element in &mut elements {
            *element = element.clone().item_type(item_type.clone());
        }

        elements
    }
}

/// Builder pattern for ScrollbarElement
pub struct ScrollbarElementBuilder {
    style: Option<ScrollbarStyle>,
    config: Option<ScrollbarConfig>,
    content_height: f32,
    viewport_height: f32,
    initial_offset: f32,
}

impl ScrollbarElementBuilder {
    pub fn new(content_height: f32, viewport_height: f32) -> Self {
        Self {
            style: None,
            config: None,
            content_height,
            viewport_height,
            initial_offset: 0.0,
        }
    }

    pub fn with_style(mut self, style: ScrollbarStyle) -> Self {
        self.style = Some(style);
        self
    }

    pub fn with_config(mut self, config: ScrollbarConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn with_initial_offset(mut self, offset: f32) -> Self {
        self.initial_offset = offset;
        self
    }

    pub fn build(self) -> ScrollbarElement {
        let mut scrollbar = if let Some(style) = self.style {
            ScrollbarElement::with_style(style)
        } else {
            ScrollbarElement::new()
        };

        if let Some(config) = self.config {
            scrollbar.config = config;
        }

        scrollbar.set_dimensions(self.content_height, self.viewport_height);
        scrollbar.set_scroll_offset(self.initial_offset);

        scrollbar
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scrollbar_visibility() {
        let mut scrollbar = ScrollbarElement::new();

        // Not visible when content fits
        scrollbar.set_dimensions(100.0, 200.0);
        assert!(!scrollbar.state.is_needed());

        // Visible when content exceeds viewport
        scrollbar.set_dimensions(300.0, 200.0);
        assert!(scrollbar.state.is_needed());
    }

    #[test]
    fn test_builder_pattern() {
        let scrollbar = ScrollbarElementBuilder::new(1000.0, 200.0)
            .with_style(ScrollbarStyle::modal())
            .with_initial_offset(100.0)
            .build();

        assert_eq!(scrollbar.scroll_offset(), 100.0);
        assert!(scrollbar.state.is_needed());
    }
}
