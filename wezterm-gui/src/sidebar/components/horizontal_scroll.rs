//! Horizontal scrolling support for sidebar components
//!
//! This module provides a reusable horizontal scrolling container that can be used
//! by any component needing horizontal scroll functionality. The scrollbar_ui_item
//! parameter allows callers to specify their own UIItemType for mouse interaction.
//!
//! # Example
//!
//! ```no_run
//! let scrollable_content = create_horizontal_scroll_container(
//!     font,
//!     content_elements,
//!     viewport_width,
//!     content_width,
//!     scroll_offset,
//!     scrollbar_opacity,
//!     &config,
//!     UIItemType::MyCustomScrollbar(item_id),
//! );
//! ```

use crate::color::LinearRgba;
use crate::termwindow::box_model::{
    BoxDimension, DisplayType, Element, ElementColors, ElementContent,
};
use crate::termwindow::UIItemType;
use config::Dimension;
use std::rc::Rc;
use wezterm_font::LoadedFont;

/// Configuration for horizontal scrollbar appearance and behavior
#[derive(Debug, Clone)]
pub struct HorizontalScrollConfig {
    /// Height of the scrollbar track and thumb
    pub scrollbar_height: f32,
    /// Minimum width of the scrollbar thumb
    pub min_thumb_width: f32,
    /// Opacity of the track when visible
    pub track_opacity: f32,
    /// Opacity of the thumb when visible
    pub thumb_opacity: f32,
    /// Opacity of the thumb when hovering (not currently used)
    pub thumb_hover_opacity: f32,
    /// Space between content and scrollbar
    pub scrollbar_margin: f32,
}

impl Default for HorizontalScrollConfig {
    fn default() -> Self {
        Self {
            scrollbar_height: 6.0,
            min_thumb_width: 30.0,
            track_opacity: 0.3,
            thumb_opacity: 0.6,
            thumb_hover_opacity: 0.8,
            scrollbar_margin: 4.0,
        }
    }
}

/// Create a horizontally scrollable container with optional scrollbar
pub fn create_horizontal_scroll_container(
    font: &Rc<LoadedFont>,
    content: Vec<Element>,
    viewport_width: f32,
    content_width: f32,
    scroll_offset: f32,
    scrollbar_opacity: f32,
    config: &HorizontalScrollConfig,
    scrollbar_ui_item: UIItemType,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let needs_scrollbar = content_width > viewport_width;

    if needs_scrollbar {
        // Create scrollable content with negative margin for offset
        let scrollable_content = Element::new(font, ElementContent::Children(content))
            .margin(BoxDimension {
                left: Dimension::Pixels(-scroll_offset),
                ..Default::default()
            })
            .display(DisplayType::Block);

        // Wrap in viewport container that clips overflow
        let viewport = Element::new(font, ElementContent::Children(vec![scrollable_content]))
            .min_width(Some(Dimension::Pixels(viewport_width)))
            .max_width(Some(Dimension::Pixels(viewport_width)))
            .display(DisplayType::Block);

        elements.push(viewport);

        // Only render scrollbar if opacity > 0
        log::debug!("Scrollbar check: opacity={}, needs_scrollbar={}, rendering={}", 
            scrollbar_opacity, needs_scrollbar, scrollbar_opacity > 0.01);
        if scrollbar_opacity > 0.01 {
            // Create scrollbar track
            let track = Element::new(font, ElementContent::Text(String::new()))
                .colors(ElementColors {
                    bg: LinearRgba::with_components(
                        1.0,  // TEMPORARY: Bright red for debugging
                        0.0,
                        0.0,
                        1.0,  // TEMPORARY: Full opacity for debugging
                    )
                    .into(),
                    ..Default::default()
                })
                .min_width(Some(Dimension::Pixels(viewport_width)))
                .min_height(Some(Dimension::Pixels(config.scrollbar_height)))
                .display(DisplayType::Block)
                .item_type(scrollbar_ui_item);

            // Calculate thumb geometry
            let thumb_ratio = viewport_width / content_width;
            let thumb_width = (viewport_width * thumb_ratio).max(config.min_thumb_width);
            let max_scroll = content_width - viewport_width;
            let scroll_ratio = if max_scroll > 0.0 {
                scroll_offset / max_scroll
            } else {
                0.0
            };
            let thumb_offset = (viewport_width - thumb_width) * scroll_ratio;

            // Create scrollbar thumb
            let thumb = Element::new(font, ElementContent::Text(String::new()))
                .colors(ElementColors {
                    bg: LinearRgba::with_components(
                        0.0,  // TEMPORARY: Bright green for debugging
                        1.0,
                        0.0,
                        1.0,  // TEMPORARY: Full opacity for debugging
                    )
                    .into(),
                    ..Default::default()
                })
                .min_width(Some(Dimension::Pixels(thumb_width)))
                .min_height(Some(Dimension::Pixels(config.scrollbar_height)))
                .margin(BoxDimension {
                    left: Dimension::Pixels(thumb_offset),
                    ..Default::default()
                })
                .display(DisplayType::Block);

            // Stack track and thumb
            let scrollbar = Element::new(font, ElementContent::Children(vec![track, thumb]))
                .display(DisplayType::Block)
                .margin(BoxDimension {
                    top: Dimension::Pixels(config.scrollbar_margin),
                    ..Default::default()
                });

            elements.push(scrollbar);
            log::debug!("Scrollbar element created and added to elements vector");
        }
    } else {
        // No scrolling needed, return content as-is
        elements.extend(content);
    }

    log::debug!("create_horizontal_scroll_container returning {} elements", elements.len());
    elements
}

/// Calculate scroll offset from a drag operation
pub fn calculate_drag_scroll(
    drag_start_x: f32,
    current_x: f32,
    drag_start_offset: f32,
    viewport_width: f32,
    content_width: f32,
    thumb_width: f32,
) -> f32 {
    let delta_x = current_x - drag_start_x;
    let track_width = viewport_width - thumb_width;

    if track_width > 0.0 {
        let max_scroll = content_width - viewport_width;
        let scroll_delta = (delta_x / track_width) * max_scroll;
        drag_start_offset + scroll_delta
    } else {
        drag_start_offset
    }
}

/// Hit test for horizontal scrollbar interaction
pub fn hit_test_scrollbar(
    mouse_x: f32,
    scrollbar_x: f32,
    viewport_width: f32,
    content_width: f32,
    scroll_offset: f32,
    min_thumb_width: f32,
) -> ScrollbarHitTarget {
    let relative_x = mouse_x - scrollbar_x;

    if relative_x < 0.0 || relative_x > viewport_width {
        return ScrollbarHitTarget::None;
    }

    // Calculate thumb geometry
    let thumb_ratio = viewport_width / content_width;
    let thumb_width = (viewport_width * thumb_ratio).max(min_thumb_width);
    let max_scroll = content_width - viewport_width;
    let scroll_ratio = if max_scroll > 0.0 {
        scroll_offset / max_scroll
    } else {
        0.0
    };
    let thumb_offset = (viewport_width - thumb_width) * scroll_ratio;

    if relative_x < thumb_offset {
        ScrollbarHitTarget::BeforeThumb
    } else if relative_x < thumb_offset + thumb_width {
        ScrollbarHitTarget::Thumb
    } else {
        ScrollbarHitTarget::AfterThumb
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollbarHitTarget {
    None,
    BeforeThumb,
    Thumb,
    AfterThumb,
}
