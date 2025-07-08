// Virtual scrolling implementation for ScrollableContainer
// This new version stores raw data instead of pre-rendered Elements

use super::scrollbar_state::{ScrollbarConfig, ScrollbarState};
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent, Float,
};
use ::window::color::LinearRgba;
use config::{Dimension, DimensionContext};
use std::collections::HashMap;
use std::hash::Hash;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use termwiz::input::{MouseButtons, MouseEvent};
use wezterm_font::LoadedFont;

/// Trait for items that can be scrolled
pub trait ScrollableItem: Clone {
    /// Get a unique ID for this item (for height caching)
    fn id(&self) -> String;
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

/// Generic scrollable container that supports virtual scrolling
pub struct ScrollableContainer<T: ScrollableItem> {
    /// Raw items data (not Elements)
    items: Vec<T>,

    /// Height cache persists across frames
    height_cache: HashMap<String, f32>,

    /// Current viewport dimensions
    viewport_height: f32,
    viewport_width: f32,

    /// Current scroll offset
    scroll_offset: f32,

    /// Virtual scrolling state
    visible_range: Range<usize>,

    /// Scrollbar configuration
    show_scrollbar: bool,
    smooth_scroll: bool,
    scroll_speed: f32,
    scrollbar_state: ScrollbarState,
    scrollbar_config: ScrollbarConfig,

    /// Font metrics context
    font_context: Option<DimensionContext>,

    /// Callback to render an item to an Element
    render_item: Arc<dyn Fn(&T, usize) -> Element + Send + Sync>,

    /// Callback to estimate item height
    estimate_height: Arc<dyn Fn(&T) -> f32 + Send + Sync>,
}

impl<T: ScrollableItem> ScrollableContainer<T> {
    /// Create a new scrollable container with the given viewport height
    pub fn new(
        viewport_height: f32,
        render_item: Arc<dyn Fn(&T, usize) -> Element + Send + Sync>,
        estimate_height: Arc<dyn Fn(&T) -> f32 + Send + Sync>,
    ) -> Self {
        Self {
            items: Vec::new(),
            height_cache: HashMap::new(),
            viewport_height,
            viewport_width: 300.0, // Default width
            scroll_offset: 0.0,
            visible_range: 0..0,
            show_scrollbar: true,
            smooth_scroll: true,
            scroll_speed: 40.0,
            scrollbar_state: ScrollbarState::new(),
            scrollbar_config: ScrollbarConfig::default(),
            font_context: None,
            render_item,
            estimate_height,
        }
    }

    /// Set the viewport dimensions
    pub fn with_viewport_dimensions(mut self, width: f32, height: f32) -> Self {
        self.viewport_width = width;
        self.viewport_height = height;
        self.update_visible_range();
        self
    }

    /// Set the font context for accurate height calculations
    pub fn with_font_context(mut self, context: DimensionContext) -> Self {
        self.font_context = Some(context);
        self.scroll_speed = context.pixel_cell * 2.0; // 2 lines per scroll step
        self
    }

    /// Set auto-hide scrollbar behavior
    pub fn with_auto_hide_scrollbar(mut self, auto_hide: bool) -> Self {
        self.scrollbar_config.auto_hide = auto_hide;
        self
    }

    /// Set the items to display
    pub fn set_items(&mut self, items: Vec<T>) {
        self.items = items;
        self.update_visible_range();
        self.update_scrollbar_state();
        self.constrain_scroll();
    }

    /// Add a single item
    pub fn add_item(&mut self, item: T) {
        self.items.push(item);
        self.update_visible_range();
        self.update_scrollbar_state();
    }

    /// Clear all items
    pub fn clear(&mut self) {
        self.items.clear();
        self.height_cache.clear();
        self.visible_range = 0..0;
        self.scroll_offset = 0.0;
        self.scrollbar_state.set_scroll_offset(0.0);
        self.update_scrollbar_state();
    }

    /// Update which items are visible based on scroll position
    fn update_visible_range(&mut self) {
        const BUFFER_ITEMS: usize = 3;

        if self.items.is_empty() {
            self.visible_range = 0..0;
            return;
        }

        let mut current_y = 0.0;
        let mut start_idx = 0;
        let mut end_idx = self.items.len();
        let viewport_start = self.scroll_offset;
        let viewport_end = self.scroll_offset + self.viewport_height;

        // Find first visible item
        for (idx, item) in self.items.iter().enumerate() {
            let item_height = self.get_item_height(item);

            if current_y + item_height > viewport_start {
                start_idx = idx.saturating_sub(BUFFER_ITEMS);
                break;
            }
            current_y += item_height;
        }

        // Find last visible item
        current_y = 0.0;
        for (idx, item) in self.items.iter().enumerate() {
            if current_y > viewport_end {
                end_idx = (idx + BUFFER_ITEMS).min(self.items.len());
                break;
            }
            let item_height = self.get_item_height(item);
            current_y += item_height;
        }

        self.visible_range = start_idx..end_idx;

        log::trace!(
            "Visible range: {}..{} (of {} items), viewport: {:.1}..{:.1}",
            start_idx,
            end_idx,
            self.items.len(),
            viewport_start,
            viewport_end
        );
    }

    /// Get the height of an item (cached or estimated)
    fn get_item_height(&self, item: &T) -> f32 {
        let id = item.id();
        self.height_cache
            .get(&id)
            .copied()
            .unwrap_or_else(|| (self.estimate_height)(item))
    }

    /// Calculate total content height
    fn calculate_total_height(&self) -> f32 {
        self.items
            .iter()
            .map(|item| self.get_item_height(item))
            .sum()
    }

    /// Update scrollbar state based on content
    fn update_scrollbar_state(&mut self) {
        let content_height = self.calculate_total_height();
        self.scrollbar_state
            .set_dimensions(content_height, self.viewport_height);
    }

    /// Constrain scroll offset to valid range
    fn constrain_scroll(&mut self) {
        let content_height = self.calculate_total_height();
        if content_height <= self.viewport_height {
            self.scroll_offset = 0.0;
            self.scrollbar_state.set_scroll_offset(0.0);
        } else {
            let max_offset = content_height - self.viewport_height;
            self.scroll_offset = self.scroll_offset.clamp(0.0, max_offset);
            self.scrollbar_state.set_scroll_offset(self.scroll_offset);
        }
    }

    /// Set scroll offset directly
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = offset;
        self.scrollbar_state.set_scroll_offset(offset);
        self.constrain_scroll();
        self.update_visible_range();
    }

    /// Scroll up by the given number of lines
    pub fn scroll_up(&mut self, lines: usize) {
        let pixels = lines as f32 * self.scroll_speed;
        self.set_scroll_offset(self.scroll_offset - pixels);
    }

    /// Scroll down by the given number of lines
    pub fn scroll_down(&mut self, lines: usize) {
        let pixels = lines as f32 * self.scroll_speed;
        self.set_scroll_offset(self.scroll_offset + pixels);
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        self.set_scroll_offset(0.0);
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        let content_height = self.calculate_total_height();
        if content_height > self.viewport_height {
            self.set_scroll_offset(content_height - self.viewport_height);
        }
    }

    /// Check if scrolled to bottom
    pub fn is_at_bottom(&self) -> bool {
        let content_height = self.calculate_total_height();
        if content_height <= self.viewport_height {
            true
        } else {
            self.scroll_offset >= content_height - self.viewport_height - 1.0
        }
    }

    /// Update cached heights from rendered elements
    pub fn update_height_cache(&mut self, computed_heights: &[(String, f32)]) {
        for (id, height) in computed_heights {
            self.height_cache.insert(id.clone(), *height);
        }
        // After updating cache, recalculate visible range and scrollbar
        self.update_visible_range();
        self.update_scrollbar_state();
    }

    /// Get scrollbar info for external rendering
    pub fn get_scrollbar_info(&self) -> ScrollbarInfo {
        let content_height = self.calculate_total_height();
        let should_show = self.show_scrollbar && self.scrollbar_state.is_needed();

        if !should_show || content_height == 0.0 {
            return ScrollbarInfo {
                should_show: false,
                thumb_position: 0.0,
                thumb_size: 1.0,
                content_height,
                viewport_height: self.viewport_height,
                scroll_offset: self.scroll_offset,
                total_items: self.items.len(),
                viewport_items: self.visible_range.len(),
            };
        }

        // Calculate thumb size as ratio of viewport to total
        let thumb_size = (self.viewport_height / content_height).min(1.0);

        // Calculate thumb position
        let max_scroll = (content_height - self.viewport_height).max(0.0);
        let thumb_position = if max_scroll > 0.0 {
            self.scroll_offset / max_scroll
        } else {
            0.0
        };

        ScrollbarInfo {
            should_show: true,
            thumb_position: thumb_position.clamp(0.0, 1.0),
            thumb_size: thumb_size.clamp(0.1, 1.0), // Minimum 10% size
            content_height,
            viewport_height: self.viewport_height,
            scroll_offset: self.scroll_offset,
            total_items: self.items.len(),
            viewport_items: self.visible_range.len(),
        }
    }

    /// Render the visible elements
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        // Only render visible items
        let mut visible_elements = Vec::new();

        // Calculate Y offset for the first visible item
        let mut y_offset = 0.0;
        for (idx, item) in self.items.iter().enumerate() {
            if idx >= self.visible_range.start {
                break;
            }
            y_offset += self.get_item_height(item);
        }

        // Render visible items
        for idx in self.visible_range.clone() {
            if let Some(item) = self.items.get(idx) {
                let mut element = (self.render_item)(item, idx);

                // If we have a cached height, attach it to the element
                if let Some(height) = self.height_cache.get(&item.id()) {
                    element = element.with_computed_height(*height);
                }

                visible_elements.push(element);
            }
        }

        log::trace!(
            "Rendering {} visible items (of {} total), scroll_offset={:.1}",
            visible_elements.len(),
            self.items.len(),
            self.scroll_offset
        );

        // Create content area with negative margin for scrolling
        let content_area = Element::new(font, ElementContent::Children(visible_elements))
            .display(DisplayType::Block)
            .margin(BoxDimension {
                top: Dimension::Pixels(-self.scroll_offset + y_offset),
                ..Default::default()
            });

        // Create viewport container with fixed height and clipping
        Element::new(font, ElementContent::Children(vec![content_area]))
            .display(DisplayType::Block)
            .min_height(Some(Dimension::Pixels(self.viewport_height)))
    }

    /// Get reference to scrollbar state
    pub fn scrollbar_state(&self) -> &ScrollbarState {
        &self.scrollbar_state
    }

    /// Update animation (returns true if animation needs update)
    pub fn update_animation(&mut self) -> bool {
        self.scrollbar_state
            .update_animation(&self.scrollbar_config)
    }

    /// Handle mouse events
    pub fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if event.mouse_buttons.contains(MouseButtons::VERT_WHEEL) {
            if event.mouse_buttons.contains(MouseButtons::WHEEL_POSITIVE) {
                self.scroll_down(3);
            } else {
                self.scroll_up(3);
            }
            true
        } else {
            // TODO: Handle scrollbar dragging
            false
        }
    }

    /// Search for an item
    pub fn search<F>(&self, predicate: F) -> Option<usize>
    where
        F: Fn(&T) -> bool,
    {
        self.items.iter().position(predicate)
    }

    /// Jump to a specific item
    pub fn jump_to_item(&mut self, index: usize) {
        if index >= self.items.len() {
            return;
        }

        // Calculate scroll offset to show this item
        let mut offset = 0.0;
        for i in 0..index {
            if let Some(item) = self.items.get(i) {
                offset += self.get_item_height(item);
            }
        }

        self.set_scroll_offset(offset);
    }
}
