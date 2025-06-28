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
    BoxDimension, ClipBounds, DisplayType, Element, ElementColors, ElementContent, Float,
};
use crate::termwindow::UIItemType;
use config::Dimension;
use std::rc::Rc;
use wezterm_font::LoadedFont;
use wezterm_term::color::SrgbaTuple;

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
            scrollbar_height: 8.0, // 2x the previous thickness
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
    base_zindex: Option<i8>,
) -> Vec<Element> {
    let mut elements = Vec::new();
    let needs_scrollbar = content_width > viewport_width;

    if needs_scrollbar {
        // Create a clipping container first
        let mut inner_content = Vec::new();

        // Add each line with proper width constraints
        log::trace!(
            "Processing {} lines for horizontal scroll, content_width={}",
            content.len(),
            content_width
        );
        for (idx, mut line_element) in content.into_iter().enumerate() {
            // Ensure each line doesn't wrap by setting a large min width
            // Remove any max_width constraints that might truncate content
            line_element = line_element
                .min_width(Some(Dimension::Pixels(content_width)))
                .max_width(None)
                .display(DisplayType::Block);
            log::trace!(
                "Line {}: set min_width to {}, removed max_width",
                idx,
                content_width
            );
            inner_content.push(line_element);
        }

        // Wrap all content in a container that will be shifted
        let mut content_container = Element::new(font, ElementContent::Children(inner_content))
            .min_width(Some(Dimension::Pixels(content_width)))
            .max_width(None) // Ensure no max_width constraint
            .display(DisplayType::Block);

        // Apply negative left margin to shift content based on scroll offset
        if scroll_offset > 0.0 {
            log::debug!(
                "Applying negative margin of {} to shift content",
                -scroll_offset
            );
            content_container = content_container.margin(BoxDimension {
                left: Dimension::Pixels(-scroll_offset),
                ..Default::default()
            });
        }

        // Create viewport that enforces width constraints and clips content
        log::trace!("Creating viewport for code block: viewport_width={}, content_width={}, scroll_offset={}", 
            viewport_width, content_width, scroll_offset);
        // Don't set max_width - let content be its natural width
        // The clipping should handle visual overflow
        let mut viewport = Element::new(font, ElementContent::Children(vec![content_container]))
            .min_width(Some(Dimension::Pixels(viewport_width)))
            .display(DisplayType::Block)
            .with_clip_bounds(ClipBounds::ContentBounds);

        // Apply z-index if provided to ensure proper layering for clipping
        if let Some(zindex) = base_zindex {
            viewport = viewport.zindex(zindex);
        }

        elements.push(viewport);

        // Always render scrollbar space - control visibility with opacity
        let scrollbar_height = config.scrollbar_height; // Should be 4.0

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

        log::debug!(
            "Scrollbar geometry: viewport={:.1}, content={:.1}, thumb_width={:.1}, thumb_offset={:.1}, scroll_offset={:.1}, opacity={:.2}",
            viewport_width, content_width, thumb_width, thumb_offset, scroll_offset, scrollbar_opacity
        );

        // Create scrollbar with thumb inside track
        let mut track_children = Vec::new();

        // Add thumb as child of track with proper positioning
        if thumb_width > 0.0 && viewport_width > 0.0 && scrollbar_opacity > 0.01 {
            let scrollbar_thumb = Element::new(font, ElementContent::Children(vec![]))
                .colors(ElementColors {
                    bg: LinearRgba::with_components(
                        0.5,
                        0.5,
                        0.5,
                        config.thumb_opacity * scrollbar_opacity,
                    )
                    .into(),
                    ..Default::default()
                })
                .min_width(Some(Dimension::Pixels(thumb_width)))
                .min_height(Some(Dimension::Pixels(scrollbar_height)))
                .margin(BoxDimension {
                    left: Dimension::Pixels(thumb_offset),
                    ..Default::default()
                })
                .display(DisplayType::Block);

            track_children.push(scrollbar_thumb);
        }

        // Create scrollbar track with thumb as child
        // Always render track to reserve space, but use opacity to control visibility
        let track_opacity = if scrollbar_opacity > 0.01 {
            config.track_opacity * scrollbar_opacity
        } else {
            0.0 // Invisible but still takes up space
        };

        let mut scrollbar_track = Element::new(font, ElementContent::Children(track_children))
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.15, 0.15, 0.15, track_opacity).into(),
                ..Default::default()
            })
            .min_width(Some(Dimension::Pixels(viewport_width)))
            .min_height(Some(Dimension::Pixels(scrollbar_height)))
            .margin(BoxDimension {
                top: Dimension::Pixels(config.scrollbar_margin),
                ..Default::default()
            })
            .display(DisplayType::Block)
            .item_type(scrollbar_ui_item.clone());

        // Apply z-index to scrollbar as well
        if let Some(zindex) = base_zindex {
            scrollbar_track = scrollbar_track.zindex(zindex);
        }

        elements.push(scrollbar_track);

        log::debug!("Scrollbar elements created: track + thumb");
    } else {
        // No scrolling needed, return content as-is
        elements.extend(content);
    }

    log::debug!(
        "create_horizontal_scroll_container returning {} elements",
        elements.len()
    );
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
