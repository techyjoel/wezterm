//! Shared scrollbar styling for consistent appearance across components
//!
//! This module provides a unified styling system for scrollbars used in different
//! parts of the application (modals, sidebars, etc). It ensures visual consistency
//! while allowing customization for specific use cases.

use ::window::color::LinearRgba;
use wezterm_term::color::ColorPalette;

/// Scrollbar color configuration for different states and components
#[derive(Clone, Debug)]
pub struct ScrollbarColors {
    /// Background color of the scrollbar track (None = use parent background)
    pub track_bg: Option<LinearRgba>,
    /// Thumb color in normal state
    pub thumb_normal: LinearRgba,
    /// Thumb color when hovering
    pub thumb_hover: LinearRgba,
    /// Thumb color when actively dragging
    pub thumb_active: LinearRgba,
}

impl ScrollbarColors {
    /// Default style - track inherits from parent background
    pub fn default_style(palette: &ColorPalette) -> Self {
        let thumb_color = palette.scrollbar_thumb.to_linear();
        Self {
            track_bg: None, // Will use parent background
            thumb_normal: thumb_color,
            thumb_hover: thumb_color.mul_alpha(0.8),
            thumb_active: thumb_color.mul_alpha(0.9),
        }
    }

    /// Modal style - semi-transparent overlays
    pub fn modal_style() -> Self {
        Self {
            track_bg: Some(LinearRgba(0.2, 0.2, 0.2, 0.2)),
            thumb_normal: LinearRgba(0.4, 0.4, 0.4, 0.6),
            thumb_hover: LinearRgba(0.4, 0.4, 0.4, 0.8),
            thumb_active: LinearRgba(0.4, 0.4, 0.4, 0.9),
        }
    }

    /// Activity log style - optimized for dark background
    pub fn activity_log_style() -> Self {
        Self {
            track_bg: Some(LinearRgba::with_components(0.03, 0.03, 0.035, 1.0)),
            thumb_normal: LinearRgba(0.3, 0.3, 0.3, 0.6),
            thumb_hover: LinearRgba(0.4, 0.4, 0.4, 0.8),
            thumb_active: LinearRgba(0.5, 0.5, 0.5, 0.9),
        }
    }

    /// Builder method to set a specific track background color
    pub fn with_track_bg(mut self, bg: LinearRgba) -> Self {
        self.track_bg = Some(bg);
        self
    }

    /// Builder method to customize thumb colors
    pub fn with_thumb_colors(
        mut self,
        normal: LinearRgba,
        hover: LinearRgba,
        active: LinearRgba,
    ) -> Self {
        self.thumb_normal = normal;
        self.thumb_hover = hover;
        self.thumb_active = active;
        self
    }

    /// Get the appropriate thumb color based on state
    pub fn get_thumb_color(&self, is_hovering: bool, is_dragging: bool) -> LinearRgba {
        if is_dragging {
            self.thumb_active
        } else if is_hovering {
            self.thumb_hover
        } else {
            self.thumb_normal
        }
    }

    /// Get the track opacity based on hover state
    pub fn get_track_opacity(&self, is_hovering: bool) -> f32 {
        if let Some(track_color) = self.track_bg {
            // If we have a specific track color, use its alpha
            track_color.3
        } else {
            // Default track opacity
            if is_hovering {
                0.4
            } else {
                0.2
            }
        }
    }
}

/// Visual dimensions and spacing for scrollbars
#[derive(Clone, Debug)]
pub struct ScrollbarDimensions {
    /// Width of the scrollbar
    pub width: f32,
    /// Minimum height of the thumb
    pub min_thumb_height: f32,
    /// Padding from the edge of the container
    pub edge_padding: f32,
}

impl Default for ScrollbarDimensions {
    fn default() -> Self {
        Self {
            width: 10.0,
            min_thumb_height: 20.0,
            edge_padding: 4.0,
        }
    }
}

impl ScrollbarDimensions {
    /// Compact scrollbar for space-constrained areas
    pub fn compact() -> Self {
        Self {
            width: 6.0,
            min_thumb_height: 16.0,
            edge_padding: 2.0,
        }
    }

    /// Wide scrollbar for better touch interaction
    pub fn wide() -> Self {
        Self {
            width: 14.0,
            min_thumb_height: 30.0,
            edge_padding: 6.0,
        }
    }
}

/// Combined styling configuration for scrollbars
#[derive(Clone, Debug)]
pub struct ScrollbarStyle {
    pub colors: ScrollbarColors,
    pub dimensions: ScrollbarDimensions,
}

impl ScrollbarStyle {
    /// Create default scrollbar style from palette
    pub fn default(palette: &ColorPalette) -> Self {
        Self {
            colors: ScrollbarColors::default_style(palette),
            dimensions: ScrollbarDimensions::default(),
        }
    }

    /// Modal-specific styling
    pub fn modal() -> Self {
        Self {
            colors: ScrollbarColors::modal_style(),
            dimensions: ScrollbarDimensions::default(),
        }
    }

    /// Activity log specific styling
    pub fn activity_log() -> Self {
        Self {
            colors: ScrollbarColors::activity_log_style(),
            dimensions: ScrollbarDimensions::default(),
        }
    }

    /// Compact style for space-constrained areas
    pub fn compact(palette: &ColorPalette) -> Self {
        Self {
            colors: ScrollbarColors::default_style(palette),
            dimensions: ScrollbarDimensions::compact(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumb_color_states() {
        let colors = ScrollbarColors::modal_style();

        // Normal state
        let normal = colors.get_thumb_color(false, false);
        assert_eq!(normal.3, 0.6); // Check alpha

        // Hover state
        let hover = colors.get_thumb_color(true, false);
        assert_eq!(hover.3, 0.8);

        // Drag state (takes precedence over hover)
        let active = colors.get_thumb_color(true, true);
        assert_eq!(active.3, 0.9);
    }

    #[test]
    fn test_track_opacity() {
        let colors = ScrollbarColors::modal_style();

        // Modal style has fixed track opacity
        assert_eq!(colors.get_track_opacity(false), 0.2);
        assert_eq!(colors.get_track_opacity(true), 0.2);

        // Default style varies by hover state
        let default_colors = ScrollbarColors {
            track_bg: None,
            thumb_normal: LinearRgba(1.0, 1.0, 1.0, 1.0),
            thumb_hover: LinearRgba(1.0, 1.0, 1.0, 1.0),
            thumb_active: LinearRgba(1.0, 1.0, 1.0, 1.0),
        };
        assert_eq!(default_colors.get_track_opacity(false), 0.2);
        assert_eq!(default_colors.get_track_opacity(true), 0.4);
    }
}
