// IMPORTANT: Scrollbar Rendering Architecture Notes
//
// This scrollable container implements scrollbars as Elements within WezTerm's rendering system.
// Key constraints that shaped this implementation:
//
// 1. Layer Limitation: WezTerm uses a fixed 3-layer rendering system (layers 0, 1, 2).
//    The system is hardcoded throughout the codebase with fixed-size arrays.
//    Note, these 3 layers are separate from the z-layers (which are unlimited).
//
// 2. Render Order: The `render_element` method in box_model.rs invalidates the layers
//    quad allocator after use. This means any direct rendering with `filled_rectangle`
//    must happen BEFORE calling `render_element`, not after.
//
// 3. Element System: Since we can't render scrollbars after the main content using
//    direct drawing calls, the scrollbar must be part of the Element tree itself.
//
// Solution:
// - The scrollbar is rendered as an Element child of the viewport container
// - Uses negative margins to position the scrollbar over the content area
// - Uses Float::Right to align to the right edge
// - The track is the container's background, the thumb is a child element
//
// This approach works within WezTerm's constraints while providing a functional scrollbar.

use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent, Float,
};
use ::window::color::LinearRgba;
use config::{Dimension, DimensionContext};
use std::rc::Rc;
use termwiz::input::{MouseButtons, MouseEvent};
use wezterm_font::LoadedFont;

pub struct ScrollableContainer {
    content: Vec<Element>,
    viewport_height: f32,     // Pixel height of viewport
    content_height: f32,      // Total pixel height of all content
    item_heights: Vec<f32>,   // Actual height of each item
    item_positions: Vec<f32>, // Y position of each item
    scroll_offset: f32,       // Pixel scroll offset
    show_scrollbar: bool,
    scrollbar_width: f32,
    auto_hide_scrollbar: bool,
    smooth_scroll: bool,
    scroll_speed: f32,
    hovering_scrollbar: bool,
    dragging_scrollbar: bool,
    drag_start_y: Option<f32>,
    drag_start_offset: Option<f32>,
    // Font metrics context
    font_context: Option<DimensionContext>,
    // For backwards compatibility, keep item-based tracking
    top_row: usize,
    max_visible_items: usize,
}

/// Information needed to render a scrollbar externally
#[derive(Debug, Clone)]
pub struct ScrollbarInfo {
    /// Whether scrollbar should be shown
    pub should_show: bool,
    /// Thumb position as a fraction (0.0 = top, 1.0 = bottom)
    pub thumb_position: f32,
    /// Thumb size as a fraction of total height (0.0 to 1.0)
    pub thumb_size: f32,
    /// Total content height in pixels
    pub content_height: f32,
    /// Visible viewport height in pixels  
    pub viewport_height: f32,
    /// Current scroll offset in pixels
    pub scroll_offset: f32,
    /// DEPRECATED: Total scrollable items (kept for compatibility)
    pub total_items: usize,
    /// DEPRECATED: Visible viewport items (kept for compatibility)
    pub viewport_items: usize,
}

impl ScrollableContainer {
    pub fn new(viewport_items: usize) -> Self {
        // For backwards compatibility, accept item count but convert to pixels
        // Assume ~40px per item as default
        // TODO: This is not a safe assumption and we should not be calculating item heights here.
        let viewport_height = viewport_items as f32 * 40.0;
        Self {
            content: Vec::new(),
            viewport_height,
            content_height: 0.0,
            item_heights: Vec::new(),
            item_positions: Vec::new(),
            scroll_offset: 0.0,
            top_row: 0,
            max_visible_items: viewport_items,
            show_scrollbar: true,
            scrollbar_width: 8.0,
            auto_hide_scrollbar: true,
            smooth_scroll: true,
            scroll_speed: 40.0, // Pixels per scroll step
            hovering_scrollbar: false,
            dragging_scrollbar: false,
            drag_start_y: None,
            drag_start_offset: None,
            font_context: None,
        }
    }

    /// Create a scrollable container with pixel-based viewport height
    pub fn new_with_pixel_height(viewport_height: f32) -> Self {
        Self {
            content: Vec::new(),
            viewport_height,
            content_height: 0.0,
            item_heights: Vec::new(),
            item_positions: Vec::new(),
            scroll_offset: 0.0,
            top_row: 0,
            max_visible_items: (viewport_height / 40.0).ceil() as usize,
            show_scrollbar: true,
            scrollbar_width: 8.0,
            auto_hide_scrollbar: true,
            smooth_scroll: true,
            scroll_speed: 40.0,
            hovering_scrollbar: false,
            dragging_scrollbar: false,
            drag_start_y: None,
            drag_start_offset: None,
            font_context: None,
        }
    }

    pub fn with_content(mut self, content: Vec<Element>) -> Self {
        self.content = content;
        self.update_content_metrics();
        self
    }

    pub fn with_auto_hide_scrollbar(mut self, auto_hide: bool) -> Self {
        self.auto_hide_scrollbar = auto_hide;
        self
    }

    /// Set the font context for accurate height calculations
    pub fn with_font_context(mut self, context: DimensionContext) -> Self {
        self.font_context = Some(context);
        self.scroll_speed = context.pixel_cell * 2.0; // 2 lines per scroll step
        self
    }

    pub fn with_smooth_scroll(mut self, smooth: bool) -> Self {
        self.smooth_scroll = smooth;
        self
    }

    pub fn set_content(&mut self, content: Vec<Element>) {
        self.content = content;
        self.update_content_metrics();
        self.constrain_scroll();
    }

    pub fn add_item(&mut self, item: Element) {
        self.content.push(item);
        self.update_content_metrics();
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.content_height = 0.0;
        self.scroll_offset = 0.0;
        self.top_row = 0;
    }

    fn update_content_metrics(&mut self) {
        // Clear previous measurements
        self.item_heights.clear();
        self.item_positions.clear();
        self.content_height = 0.0;

        // For now, we'll estimate heights based on content type
        // In a full implementation, we'd need to compute each element
        let mut current_y = 0.0;

        for (idx, element) in self.content.iter().enumerate() {
            let estimated_height = self.estimate_element_height(element);
            self.item_positions.push(current_y);
            self.item_heights.push(estimated_height);
            current_y += estimated_height;

            // Log first few items for debugging
            if idx < 5 {
                log::trace!(
                    "Item {}: position={:.1}, height={:.1}",
                    idx,
                    self.item_positions[idx],
                    estimated_height
                );
            }
        }

        self.content_height = current_y;

        log::debug!(
            "ScrollableContainer metrics: viewport_height={:.1}, content_height={:.1}, items={}, should_show_scrollbar={}",
            self.viewport_height,
            self.content_height,
            self.content.len(),
            self.content_height > self.viewport_height
        );

        // Log summary of height calculations
        if self.content.len() > 0 {
            let avg_item_height = self.content_height / self.content.len() as f32;
            log::debug!(
                "Average item height: {:.1}px, Total padding+margins estimated: {:.1}px",
                avg_item_height,
                self.content_height - (self.content.len() as f32 * 20.0) // Assuming 20px base text height
            );
        }

        // Update legacy item-based tracking
        let avg_height = if self.content.is_empty() {
            40.0
        } else {
            self.content_height / self.content.len() as f32
        };
        self.max_visible_items =
            ((self.viewport_height / avg_height).ceil() as usize).min(self.content.len());
    }

    /// Estimate element height based on content type
    fn estimate_element_height(&self, element: &Element) -> f32 {
        // Use the stored font context or a default one
        let context = self.font_context.unwrap_or(DimensionContext {
            dpi: 96.0,
            pixel_cell: 16.0, // Conservative default line height
            pixel_max: self.viewport_height,
        });
        self.estimate_element_height_recursive(element, 0, context)
    }

    fn estimate_element_height_recursive(
        &self,
        element: &Element,
        depth: usize,
        context: DimensionContext,
    ) -> f32 {
        let padding = element.padding.top.evaluate_as_pixels(context)
            + element.padding.bottom.evaluate_as_pixels(context);
        let margin = element.margin.top.evaluate_as_pixels(context)
            + element.margin.bottom.evaluate_as_pixels(context);

        // Account for borders if present
        let border_height = element.border.top.evaluate_as_pixels(context)
            + element.border.bottom.evaluate_as_pixels(context);

        // Get the actual line height from element or use default
        let line_height_multiplier = element.line_height.unwrap_or(1.0) as f32;
        let base_line_height = context.pixel_cell;
        let actual_line_height = base_line_height * line_height_multiplier;

        let content_height = match &element.content {
            ElementContent::Text(text) => {
                // Calculate height based on actual line count and font metrics
                let lines = text.lines().count().max(1);
                let text_height = lines as f32 * actual_line_height;

                // Log markdown text heights
                if lines > 1 || text.len() > 100 {
                    log::trace!(
                        "Text height (depth {}): {} lines, {}px, text_preview: {:?}",
                        depth,
                        lines,
                        text_height,
                        &text.chars().take(50).collect::<String>()
                    );
                }
                text_height
            }
            ElementContent::Children(children) => {
                // Recursively calculate height of all children
                if children.is_empty() {
                    0.0
                } else {
                    let mut total_height = 0.0;
                    for (idx, child) in children.iter().enumerate() {
                        let child_height =
                            self.estimate_element_height_recursive(child, depth + 1, context);
                        total_height += child_height;

                        // Log significant child heights
                        if child_height > actual_line_height * 2.0 && depth < 3 {
                            log::trace!(
                                "Child {} height (depth {}): {:.1}px",
                                idx,
                                depth,
                                child_height
                            );
                        }
                    }
                    // Don't add extra spacing - elements already have their own margins
                    total_height
                }
            }
            _ => actual_line_height, // Default to one line height for other types
        };

        let total = content_height + padding + margin + border_height;

        // Log significant element heights
        if total > actual_line_height * 2.0 && depth < 3 {
            log::trace!(
                "Element height (depth {}): content={:.1}, padding={:.1}, margin={:.1}, border={:.1}, total={:.1}",
                depth, content_height, padding, margin, border_height, total
            );
        }

        total
    }

    fn constrain_scroll(&mut self) {
        if self.content_height <= self.viewport_height {
            self.scroll_offset = 0.0;
            self.top_row = 0;
        } else {
            let max_scroll = (self.content_height - self.viewport_height).max(0.0);
            self.scroll_offset = self.scroll_offset.min(max_scroll);

            // Find first visible item based on actual positions
            self.top_row = 0;
            for (idx, &pos) in self.item_positions.iter().enumerate() {
                if pos + self.item_heights.get(idx).copied().unwrap_or(40.0) >= self.scroll_offset {
                    self.top_row = idx;
                    break;
                }
            }
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        let pixels = lines as f32 * self.scroll_speed;
        self.scroll_offset = (self.scroll_offset - pixels).max(0.0);
        self.constrain_scroll();
    }

    pub fn scroll_down(&mut self, lines: usize) {
        let pixels = lines as f32 * self.scroll_speed;
        self.scroll_offset = self.scroll_offset + pixels;
        self.constrain_scroll();
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0.0;
        self.top_row = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.content_height > self.viewport_height {
            self.scroll_offset = self.content_height - self.viewport_height;
            let estimated_item_height = 60.0;
            self.top_row = (self.scroll_offset / estimated_item_height).floor() as usize;
        }
    }

    pub fn is_at_bottom(&self) -> bool {
        if self.content_height <= self.viewport_height {
            true
        } else {
            self.scroll_offset >= self.content_height - self.viewport_height - 1.0
        }
    }

    pub fn set_scroll_offset(&mut self, offset: f32) {
        log::debug!(
            "ScrollableContainer::set_scroll_offset - offset={} -> constrained to {}",
            offset,
            offset.clamp(0.0, (self.content_height - self.viewport_height).max(0.0))
        );
        self.scroll_offset = offset;
        self.constrain_scroll();
    }

    fn get_scrollbar_thumb_info(&self) -> (f32, f32) {
        if self.content_height <= self.viewport_height {
            return (0.0, self.viewport_height);
        }

        let viewport_ratio = self.viewport_height / self.content_height;
        let thumb_height = (viewport_ratio * self.viewport_height).max(20.0);

        let max_scroll = self.content_height - self.viewport_height;
        let scroll_ratio = if max_scroll > 0.0 {
            self.scroll_offset / max_scroll
        } else {
            0.0
        };
        let thumb_top = scroll_ratio * (self.viewport_height - thumb_height);

        (thumb_top, thumb_height)
    }

    /// Get scrollbar rendering information
    pub fn get_scrollbar_info(&self) -> ScrollbarInfo {
        let should_show = self.should_show_scrollbar();

        if !should_show || self.content_height == 0.0 {
            return ScrollbarInfo {
                should_show: false,
                thumb_position: 0.0,
                thumb_size: 1.0,
                content_height: self.content_height,
                viewport_height: self.viewport_height,
                scroll_offset: self.scroll_offset,
                // Deprecated fields
                total_items: self.content.len(),
                viewport_items: self.max_visible_items,
            };
        }

        // Calculate thumb size as ratio of viewport to total
        let thumb_size = (self.viewport_height / self.content_height).min(1.0);

        // Calculate thumb position
        let max_scroll = (self.content_height - self.viewport_height).max(0.0);
        let thumb_position = if max_scroll > 0.0 {
            self.scroll_offset / max_scroll
        } else {
            0.0
        };

        ScrollbarInfo {
            should_show: true,
            thumb_position: thumb_position.clamp(0.0, 1.0),
            thumb_size: thumb_size.clamp(0.1, 1.0), // Minimum 10% size
            content_height: self.content_height,
            viewport_height: self.viewport_height,
            scroll_offset: self.scroll_offset,
            // Deprecated fields for compatibility
            total_items: self.content.len(),
            viewport_items: self.max_visible_items,
        }
    }

    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        log::debug!(
            "ScrollableContainer::render - scroll_offset={}, content_height={}, viewport_height={}, items={}",
            self.scroll_offset, self.content_height, self.viewport_height, self.content.len()
        );

        // Simply render all content items - let the viewport handle clipping
        // The negative margin will shift the content up for scrolling
        let content_area = Element::new(font, ElementContent::Children(self.content.clone()))
            .display(DisplayType::Block)
            .margin(BoxDimension {
                top: Dimension::Pixels(-self.scroll_offset),
                ..Default::default()
            });

        // Create viewport container with fixed height and clipping
        let mut viewport_children = vec![content_area];

        // Don't render scrollbar as Element - it will be rendered externally
        // using direct rendering at the appropriate z-index

        // Create viewport container with fixed height to enforce clipping
        // Remove background color so content shows through the "hole" in the sidebar
        Element::new(font, ElementContent::Children(viewport_children))
            .display(DisplayType::Block)
            .min_height(Some(Dimension::Pixels(self.viewport_height)))
    }

    fn render_scrollbar_element(
        &self,
        font: &Rc<LoadedFont>,
        scrollbar_info: &ScrollbarInfo,
    ) -> Element {
        let scrollbar_width = self.scrollbar_width;
        let thumb_height = (scrollbar_info.thumb_size * self.viewport_height).max(20.0);
        let available_space = self.viewport_height - thumb_height;
        let thumb_offset = scrollbar_info.thumb_position * available_space;

        // Create scrollbar thumb
        let thumb = Element::new(font, ElementContent::Text(String::new()))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.7, 0.7, 0.7, 0.8).into(),
                ..Default::default()
            })
            .min_width(Some(Dimension::Pixels(scrollbar_width)))
            .min_height(Some(Dimension::Pixels(thumb_height)))
            .margin(BoxDimension {
                top: Dimension::Pixels(thumb_offset),
                ..Default::default()
            });

        // Create scrollbar container with thumb overlaid on track
        let scrollbar_container = Element::new(font, ElementContent::Children(vec![thumb]))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.3, 0.3, 0.3, 0.5).into(), // Track background
                ..Default::default()
            })
            .min_width(Some(Dimension::Pixels(scrollbar_width)))
            .min_height(Some(Dimension::Pixels(self.viewport_height)))
            .margin(BoxDimension {
                left: Dimension::Pixels(-scrollbar_width - 4.0), // Negative margin to position at right edge
                top: Dimension::Pixels(-self.viewport_height - 1.0), // Move up to align with viewport top
                ..Default::default()
            })
            .float(Float::Right)
            .zindex(2); // Render on top layer

        scrollbar_container
    }

    fn should_show_scrollbar(&self) -> bool {
        if !self.show_scrollbar {
            return false;
        }

        let needs_scrollbar = self.content_height > self.viewport_height;
        log::trace!(
            "should_show_scrollbar: content_height={:.1}, viewport_height={:.1}, needs={}, auto_hide={}, hovering={}",
            self.content_height, self.viewport_height, needs_scrollbar,
            self.auto_hide_scrollbar, self.hovering_scrollbar
        );

        if !needs_scrollbar {
            return false;
        }

        if self.auto_hide_scrollbar {
            self.hovering_scrollbar || self.dragging_scrollbar
        } else {
            true
        }
    }

    pub fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if event.mouse_buttons.contains(MouseButtons::VERT_WHEEL) {
            if event.mouse_buttons.contains(MouseButtons::WHEEL_POSITIVE) {
                self.scroll_up(3); // Scroll 3 lines
            } else {
                self.scroll_down(3); // Scroll 3 lines
            }
            true
        } else if event.mouse_buttons == MouseButtons::LEFT {
            if self.is_over_scrollbar(event.x, event.y) {
                self.dragging_scrollbar = true;
                self.drag_start_y = Some(event.y as f32);
                self.drag_start_offset = Some(self.scroll_offset);
                true
            } else {
                false
            }
        } else if event.mouse_buttons == MouseButtons::NONE {
            if self.dragging_scrollbar {
                if let (Some(start_y), Some(start_offset)) =
                    (self.drag_start_y, self.drag_start_offset)
                {
                    let delta_y = event.y as f32 - start_y;
                    let scroll_ratio = delta_y / self.viewport_height;
                    let scroll_delta = scroll_ratio * self.content_height;

                    self.scroll_offset = (start_offset + scroll_delta).max(0.0);
                    self.constrain_scroll();
                }
                true
            } else {
                let was_hovering = self.hovering_scrollbar;
                self.hovering_scrollbar = self.is_over_scrollbar(event.x, event.y);
                was_hovering != self.hovering_scrollbar
            }
        } else {
            false
        }
    }

    pub fn handle_mouse_release(&mut self) {
        self.dragging_scrollbar = false;
        self.drag_start_y = None;
        self.drag_start_offset = None;
    }

    fn is_over_scrollbar(&self, _x: u16, _y: u16) -> bool {
        // This is a simplified check - in practice you'd need the actual rendered bounds
        false
    }
}

// Note: Event handling will be integrated with the sidebar's mouse event handling
