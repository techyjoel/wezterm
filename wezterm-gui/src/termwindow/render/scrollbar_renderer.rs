use crate::quad::{QuadTrait, TripleLayerQuadAllocator};
use crate::renderstate::RenderState;
use crate::termwindow::{UIItem, UIItemType};
use anyhow::Result;
use config::ConfigHandle;
use euclid::{Point2D, Rect, Size2D};
use std::ops::Range;
use wezterm_term::color::ColorPalette;
use wezterm_term::StableRowIndex;
use window::color::LinearRgba;
use window::{PixelUnit, RectF};

/// Orientation of the scrollbar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarOrientation {
    Vertical,
    Horizontal,
}

/// Parts of the scrollbar for hit testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarHitTarget {
    None,
    AboveThumb,
    Thumb,
    BelowThumb,
}

/// State for tracking scrollbar interactions
#[derive(Debug, Clone)]
pub struct ScrollbarState {
    pub is_dragging: bool,
    pub drag_start_y: f32,
    pub drag_start_offset: f32,
    pub hover_target: ScrollbarHitTarget,
}

impl Default for ScrollbarState {
    fn default() -> Self {
        Self {
            is_dragging: false,
            drag_start_y: 0.0,
            drag_start_offset: 0.0,
            hover_target: ScrollbarHitTarget::None,
        }
    }
}

/// A reusable scrollbar renderer that can be used by any scrollable component
pub struct ScrollbarRenderer {
    orientation: ScrollbarOrientation,
    total_size: f32,
    viewport_size: f32,
    scroll_offset: f32,
    min_thumb_size: f32,
    state: ScrollbarState,
}

impl ScrollbarRenderer {
    /// Create a new vertical scrollbar renderer
    pub fn new_vertical(
        total_size: f32,
        viewport_size: f32,
        scroll_offset: f32,
        min_thumb_size: f32,
    ) -> Self {
        Self {
            orientation: ScrollbarOrientation::Vertical,
            total_size,
            viewport_size,
            scroll_offset,
            min_thumb_size,
            state: ScrollbarState::default(),
        }
    }

    /// Create a new horizontal scrollbar renderer
    pub fn new_horizontal(
        total_size: f32,
        viewport_size: f32,
        scroll_offset: f32,
        min_thumb_size: f32,
    ) -> Self {
        Self {
            orientation: ScrollbarOrientation::Horizontal,
            total_size,
            viewport_size,
            scroll_offset,
            min_thumb_size,
            state: ScrollbarState::default(),
        }
    }

    /// Update scrollbar parameters
    pub fn update(&mut self, total_size: f32, viewport_size: f32, scroll_offset: f32) {
        self.total_size = total_size;
        self.viewport_size = viewport_size;
        self.scroll_offset = scroll_offset;
    }

    /// Calculate thumb position and size
    fn calculate_thumb_geometry(&self, track_length: f32) -> (f32, f32) {
        if self.total_size <= self.viewport_size {
            // No scrolling needed, thumb fills the track
            return (0.0, track_length);
        }

        // Calculate thumb size as a proportion of viewport to total content
        let thumb_ratio = self.viewport_size / self.total_size;
        let thumb_size = (track_length * thumb_ratio).max(self.min_thumb_size);

        // Calculate thumb position
        let scroll_ratio = self.scroll_offset / (self.total_size - self.viewport_size);
        let max_thumb_offset = track_length - thumb_size;
        let thumb_offset = max_thumb_offset * scroll_ratio;

        (thumb_offset, thumb_size)
    }

    /// Render the scrollbar at the specified z-index
    pub fn render_direct(
        &self,
        gl_state: &RenderState,
        bounds: RectF,
        z_index: i8,
        palette: &ColorPalette,
        config: &ConfigHandle,
        filled_rectangle: impl Fn(&mut TripleLayerQuadAllocator, usize, RectF, LinearRgba) -> Result<()>,
    ) -> Result<Vec<UIItem>> {
        let mut ui_items = Vec::new();

        // Get the appropriate layer for this z-index
        let layer = gl_state.layer_for_zindex(z_index)?;
        let mut layers = layer.quad_allocator();

        // Get colors from config
        let bg_color = palette
            .background
            .to_linear()
            .mul_alpha(config.window_background_opacity);
        let thumb_color = palette.scrollbar_thumb.to_linear();
        let hover_color = thumb_color.mul_alpha(0.8);

        // Draw scrollbar track background on sub-layer 0
        filled_rectangle(&mut layers, 0, bounds, bg_color)?;

        // Calculate thumb geometry
        let track_length = match self.orientation {
            ScrollbarOrientation::Vertical => bounds.size.height,
            ScrollbarOrientation::Horizontal => bounds.size.width,
        };
        let (thumb_offset, thumb_size) = self.calculate_thumb_geometry(track_length);

        // Create thumb rectangle
        let thumb_rect = match self.orientation {
            ScrollbarOrientation::Vertical => RectF::new(
                euclid::point2(bounds.origin.x, bounds.origin.y + thumb_offset),
                euclid::size2(bounds.size.width, thumb_size),
            ),
            ScrollbarOrientation::Horizontal => RectF::new(
                euclid::point2(bounds.origin.x + thumb_offset, bounds.origin.y),
                euclid::size2(thumb_size, bounds.size.height),
            ),
        };

        // Draw thumb on sub-layer 2, with hover effect if applicable
        let thumb_final_color = if self.state.hover_target == ScrollbarHitTarget::Thumb {
            hover_color
        } else {
            thumb_color
        };
        filled_rectangle(&mut layers, 2, thumb_rect, thumb_final_color)?;

        // Create UI items for mouse interaction
        match self.orientation {
            ScrollbarOrientation::Vertical => {
                // Above thumb
                if thumb_offset > 0.0 {
                    ui_items.push(UIItem {
                        x: bounds.origin.x as usize,
                        y: bounds.origin.y as usize,
                        width: bounds.size.width as usize,
                        height: thumb_offset as usize,
                        item_type: UIItemType::AboveScrollThumb,
                    });
                }

                // Thumb
                ui_items.push(UIItem {
                    x: thumb_rect.origin.x as usize,
                    y: thumb_rect.origin.y as usize,
                    width: thumb_rect.size.width as usize,
                    height: thumb_rect.size.height as usize,
                    item_type: UIItemType::ScrollThumb,
                });

                // Below thumb
                let below_start = thumb_rect.origin.y + thumb_rect.size.height;
                let below_height = bounds.origin.y + bounds.size.height - below_start;
                if below_height > 0.0 {
                    ui_items.push(UIItem {
                        x: bounds.origin.x as usize,
                        y: below_start as usize,
                        width: bounds.size.width as usize,
                        height: below_height as usize,
                        item_type: UIItemType::BelowScrollThumb,
                    });
                }
            }
            ScrollbarOrientation::Horizontal => {
                // TODO: Implement horizontal scrollbar UI items
                // Similar to vertical but for horizontal orientation
            }
        }

        Ok(ui_items)
    }

    /// Hit test a point against the scrollbar
    pub fn hit_test(
        &self,
        point: euclid::Point2D<f32, PixelUnit>,
        bounds: RectF,
    ) -> ScrollbarHitTarget {
        if !bounds.contains(point) {
            return ScrollbarHitTarget::None;
        }

        let track_length = match self.orientation {
            ScrollbarOrientation::Vertical => bounds.size.height,
            ScrollbarOrientation::Horizontal => bounds.size.width,
        };
        let (thumb_offset, thumb_size) = self.calculate_thumb_geometry(track_length);

        let relative_pos = match self.orientation {
            ScrollbarOrientation::Vertical => point.y - bounds.origin.y,
            ScrollbarOrientation::Horizontal => point.x - bounds.origin.x,
        };

        if relative_pos < thumb_offset {
            ScrollbarHitTarget::AboveThumb
        } else if relative_pos < thumb_offset + thumb_size {
            ScrollbarHitTarget::Thumb
        } else {
            ScrollbarHitTarget::BelowThumb
        }
    }

    /// Handle mouse events
    pub fn handle_mouse_event(&mut self, event: &window::MouseEvent, bounds: RectF) -> Option<f32> {
        use window::{MouseButtons, MouseEventKind as WMEK, MousePress};

        let point = euclid::point2(event.coords.x as f32, event.coords.y as f32);
        let hit_target = self.hit_test(point, bounds);

        // Update hover state
        self.state.hover_target = hit_target;

        match event.kind {
            WMEK::Press(MousePress::Left) => match hit_target {
                ScrollbarHitTarget::Thumb => {
                    self.state.is_dragging = true;
                    self.state.drag_start_y = event.coords.y as f32;
                    self.state.drag_start_offset = self.scroll_offset;
                    None
                }
                ScrollbarHitTarget::AboveThumb => {
                    // Page up
                    Some(self.scroll_offset - self.viewport_size)
                }
                ScrollbarHitTarget::BelowThumb => {
                    // Page down
                    Some(self.scroll_offset + self.viewport_size)
                }
                ScrollbarHitTarget::None => None,
            },
            WMEK::Release(MousePress::Left) => {
                self.state.is_dragging = false;
                None
            }
            WMEK::Move => {
                if self.state.is_dragging {
                    let delta = event.coords.y as f32 - self.state.drag_start_y;
                    let track_length = match self.orientation {
                        ScrollbarOrientation::Vertical => bounds.size.height,
                        ScrollbarOrientation::Horizontal => bounds.size.width,
                    };
                    let (_, thumb_size) = self.calculate_thumb_geometry(track_length);
                    let max_thumb_offset = track_length - thumb_size;

                    if max_thumb_offset > 0.0 {
                        let scroll_delta =
                            delta / max_thumb_offset * (self.total_size - self.viewport_size);
                        Some(self.state.drag_start_offset + scroll_delta)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Get the current state
    pub fn state(&self) -> &ScrollbarState {
        &self.state
    }

    /// Check if scrollbar is needed
    pub fn is_needed(&self) -> bool {
        self.total_size > self.viewport_size
    }
}

// TODO: Add terminal-specific scrollbar helper when needed

/// Information needed to render a scrollbar
#[derive(Debug, Clone)]
pub struct ScrollInfo {
    pub total_size: f32,
    pub viewport_size: f32,
    pub scroll_offset: f32,
    pub min_thumb_size: f32,
}
