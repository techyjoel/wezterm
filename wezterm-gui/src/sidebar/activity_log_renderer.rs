//! Activity log rendering for the AI sidebar
//!
//! This module handles the rendering of activity items including commands,
//! chat messages, suggestions, and goals with virtual scrolling support.

use super::components::markdown::CodeBlockContainer;
use super::components::{
    Card, CardState, Chip, ChipSize, ChipStyle, MarkdownRenderer, ScrollbarInfo,
};
use super::position_cache::{ItemPositionData, TextPositionCache};
use super::sidebar_constants::*;
use super::{ActivityFilter, ActivityItem, AgentMode, CommandStatus, SidebarFonts};
use crate::color::LinearRgba;
use crate::sidebar::ai_sidebar::CurrentGoal;
use crate::sidebar::text_selection::SelectionTarget;
use crate::termwindow::box_model::*;
use crate::termwindow::render::scrollbar_renderer::{ScrollbarOrientation, ScrollbarRenderer};
use std::sync::{Arc, Mutex};
// Note: extract_positions_from_content is not used in this module
use crate::termwindow::UIItemType;
use chrono::{DateTime, Local};
use config::{ConfigHandle, Dimension};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::time::SystemTime;
use wezterm_font::LoadedFont;
use wezterm_term::color::ColorPalette;
use window::PixelUnit;

// Virtual scrolling constants - copied from ai_sidebar.rs
const RENDER_MARGIN: f32 = 200.0; // Pixels to render beyond viewport
const WIDTH_CHANGE_THRESHOLD: f32 = 5.0; // Pixels of width change to trigger cache clear
const HEIGHT_CHANGE_HYSTERESIS: f32 = 2.0; // Minimum height change to update cache

// Activity item spacing constants (must match render_activity_item)
const CARD_DEFAULT_MARGIN: f32 = 8.0; // Default card margin (for commands/suggestions)

/// Tracks height measurement state for activity items - copied from ai_sidebar.rs
#[derive(Debug, Clone, Default)]
pub struct HeightTracker {
    /// For tall items - scroll offset when top edge entered viewport
    pub top_entered_at: Option<f32>,
    /// For tall items - scroll offset when bottom edge entered viewport  
    pub bottom_entered_at: Option<f32>,
    /// Whether we've seen this item's full height (unclipped)
    pub seen_full_height: bool,
    /// Final measured height (from rendering or scroll tracking)
    pub measured_height: Option<f32>,
}

/// Visual anchor for maintaining scroll position stability - copied from ai_sidebar.rs
#[derive(Debug, Clone)]
pub struct VisualAnchor {
    /// Index of the anchored item in filtered items
    pub item_index: usize,
    /// Offset within the item (0 = top of item)
    pub offset_within_item: f32,
    /// Position on screen where anchor appears (0 = top of viewport)
    pub screen_position: f32,
}

/// Contains all state related to activity log rendering and scrolling
/// Does not contain the activity log data itself - that stays in AiSidebar
pub struct ActivityLogState {
    // Height caching for virtual scrolling
    pub activity_log_height_cache: HashMap<String, f32>,
    pub height_trackers: HashMap<String, HeightTracker>,
    pub activity_log_last_width: Option<f32>,

    // Visible range for virtual scrolling
    pub activity_log_visible_range: Range<usize>,

    // Scrollbar state
    pub activity_log_scrollbar: Option<crate::sidebar::components::ScrollbarInfo>,
    pub activity_log_scrollbar_renderer: Option<ScrollbarRenderer>,
    pub activity_log_scrollbar_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,
    pub activity_log_scroll_offset: f32,

    // Visual anchor for maintaining position during height changes
    pub visual_anchor: Option<VisualAnchor>,

    // UI element bounds for hit testing
    pub activity_item_bounds: HashMap<usize, euclid::Rect<f32, window::PixelUnit>>,
}

impl ActivityLogState {
    pub fn new() -> Self {
        Self {
            activity_log_height_cache: HashMap::new(),
            height_trackers: HashMap::new(),
            activity_log_last_width: None,
            activity_log_visible_range: 0..0,
            activity_log_scrollbar: None,
            activity_log_scrollbar_renderer: None,
            activity_log_scrollbar_bounds: None,
            activity_log_scroll_offset: 0.0,
            visual_anchor: None,
            activity_item_bounds: HashMap::new(),
        }
    }

    /// Clear cached data (should be called when content changes)
    pub fn clear_caches(&mut self) {
        self.activity_log_height_cache.clear();
        self.height_trackers.clear();
        self.activity_item_bounds.clear();
    }
}

/// Renderer for the activity log
pub struct ActivityLogRenderer;

impl ActivityLogRenderer {
    pub fn new() -> Self {
        ActivityLogRenderer
    }

    /// Main render function for activity log  
    /// Note: ActivityLogRenderer is stateless - all data is passed as parameters
    pub fn render_activity_log(
        state: &mut ActivityLogState,
        activity_log: &[ActivityItem],
        activity_filter: ActivityFilter,
        fonts: &SidebarFonts,
        available_height: f32,
        available_width: f32,
        palette: &ColorPalette,
        position_cache: &mut TextPositionCache,
        item_positions: &mut HashMap<usize, ItemPositionData>,
        width: u16,
        selection_state: &crate::sidebar::text_selection::SelectionState,
        code_block_registry: Option<
            Arc<Mutex<HashMap<String, super::components::markdown::CodeBlockContainer>>>,
        >,
    ) -> Element {
        // Check if width has changed and invalidate cache if needed
        if let Some(last_width) = state.activity_log_last_width {
            let width_change = (last_width - available_width).abs();
            if width_change > WIDTH_CHANGE_THRESHOLD {
                log::info!(
                    "Significant width change from {} to {} (delta: {}), clearing height cache",
                    last_width,
                    available_width,
                    width_change
                );
                state.activity_log_height_cache.clear();
                state.height_trackers.clear();
            } else if width_change > 0.1 {
                log::trace!(
                    "Minor width change: {} -> {} (delta: {}), keeping cache",
                    last_width,
                    available_width,
                    width_change
                );
            }
        }
        state.activity_log_last_width = Some(available_width);

        // Filter items based on current filter
        let filtered_items: Vec<(usize, &ActivityItem)> = activity_log
            .iter()
            .enumerate()
            .filter(|(_, item)| match activity_filter {
                ActivityFilter::All => true,
                ActivityFilter::Commands => matches!(item, ActivityItem::Command { .. }),
                ActivityFilter::Chat => matches!(item, ActivityItem::Chat { .. }),
                ActivityFilter::Suggestions => matches!(item, ActivityItem::Suggestion { .. }),
            })
            .collect();

        let filtered_count = filtered_items.len();
        log::debug!(
            "Rendering activity log: {} total items, {} filtered items",
            activity_log.len(),
            filtered_count
        );

        // Get actual font metrics for accurate height calculations
        let font_metrics = fonts.body.metrics();
        let line_height = font_metrics.cell_height.get() as f32;

        // Calculate visible range based on scroll offset
        const BUFFER_ITEMS: usize = 3; // Render 3 items above and below viewport
        let viewport_start = state.activity_log_scroll_offset;
        let viewport_end = state.activity_log_scroll_offset + available_height;

        let mut first_visible: Option<usize> = None;
        let mut last_visible: Option<usize> = None;
        let mut current_y = 0.0;

        // Find which items are actually visible in the viewport
        for (idx, (orig_idx, item)) in filtered_items.iter().enumerate() {
            let item_id = match item {
                ActivityItem::Command { id, .. } => id.clone(),
                ActivityItem::Chat { id, .. } => id.clone(),
                ActivityItem::Suggestion { id, .. } => id.clone(),
                ActivityItem::Goal { id, .. } => id.clone(),
            };

            let item_height =
                Self::get_activity_item_height(state, item, line_height, available_width);
            let item_start = current_y;
            let item_end = current_y + item_height;

            // Check if this item overlaps with the actual viewport
            let overlaps_viewport = item_end > viewport_start && item_start < viewport_end;

            // Enhanced debug logging for items near the viewport
            if idx < 3
                || idx >= filtered_items.len() - 3
                || (item_end >= viewport_start - 200.0 && item_start <= viewport_end + 200.0)
            {
                log::debug!(
                    "[VSCROLL] Item {} (idx {}): y={:.0}-{:.0} (h={:.0}), viewport={:.0}-{:.0}, overlaps={}",
                    item_id, idx, item_start, item_end, item_height, viewport_start, viewport_end, overlaps_viewport
                );
            }

            // Special logging for items we expect but don't see
            if idx >= 5 && idx <= 10 {
                log::debug!(
                    "[VSCROLL] DEBUG Item {} (idx {}): start={:.0}, end={:.0}, height={:.0}",
                    item_id,
                    idx,
                    item_start,
                    item_end,
                    item_height
                );
            }

            // Track height measurement for tall items using scroll positions
            if item_height >= available_height
                || !state
                    .height_trackers
                    .get(&item_id)
                    .map(|t| t.seen_full_height)
                    .unwrap_or(false)
            {
                let tracker = state.height_trackers.entry(item_id.clone()).or_default();

                // Track when top edge enters viewport
                if overlaps_viewport && tracker.top_entered_at.is_none() {
                    tracker.top_entered_at = Some(state.activity_log_scroll_offset);
                    log::debug!(
                        "Item {} top entered viewport at scroll offset {}",
                        item_id,
                        state.activity_log_scroll_offset
                    );
                }

                // Track when bottom edge becomes visible
                if tracker.top_entered_at.is_some() && item_end <= viewport_end {
                    if tracker.bottom_entered_at.is_none() {
                        tracker.bottom_entered_at = Some(state.activity_log_scroll_offset);

                        // Calculate height from scroll distance
                        let top_offset = tracker.top_entered_at.unwrap();
                        let bottom_offset = tracker.bottom_entered_at.unwrap();

                        // Scroll tracking is disabled - it was incorrectly adding viewport height
                        log::debug!(
                            "[VSCROLL] Scroll tracking DISABLED for {} - scroll_distance={:.0}px",
                            item_id,
                            bottom_offset - top_offset
                        );
                    }
                }
            }

            // Check if any part of the item overlaps with the actual viewport
            if item_end > viewport_start && item_start < viewport_end {
                if first_visible.is_none() {
                    first_visible = Some(idx);
                }
                last_visible = Some(idx);
            }

            current_y = item_end; // Use item_end to be consistent

            // Note: We used to have an optimization here to stop scanning early,
            // but it was causing issues with calculating total height and finding all items.
            // We need to scan all items to get accurate total height.
        }

        // Log the scan result with more detail
        let total_content_height = current_y;
        log::debug!(
            "[VSCROLL] Scan complete: total_height={:.0}, viewport={:.0}-{:.0}, first_visible={:?}, last_visible={:?}",
            total_content_height, viewport_start, viewport_end, first_visible, last_visible
        );

        // DEBUG: Check if we can theoretically scroll to see all content
        let theoretical_max_scroll = (total_content_height - available_height).max(0.0);
        if state.activity_log_scroll_offset > theoretical_max_scroll - 10.0 {
            log::info!(
                "[VSCROLL] Near bottom: scroll={:.0}, max={:.0}, last_item_bottom={:.0}, viewport_bottom={:.0}",
                state.activity_log_scroll_offset, theoretical_max_scroll, total_content_height,
                state.activity_log_scroll_offset + available_height
            );
        }

        // Apply consistent pixel-based buffer around the visible items

        let (start_idx, end_idx) = if let (Some(first), Some(last)) = (first_visible, last_visible)
        {
            // Find start index by going backwards from first_visible
            // Always include at least one item before visible range, even if it's very tall
            let mut start = first;
            let mut accumulated_before = 0.0;
            let mut items_before = 0;

            while start > 0 && (accumulated_before < RENDER_MARGIN || items_before == 0) {
                start -= 1;
                items_before += 1;
                if let Some((_, item)) = filtered_items.get(start) {
                    let item_height =
                        Self::get_activity_item_height(state, item, line_height, available_width);
                    accumulated_before += item_height;
                }
            }

            // Find end index by going forward from last_visible
            // Always include at least one item after visible range, even if it's very tall
            let mut end = last + 1;
            let mut accumulated_after = 0.0;
            let mut items_after = 0;
            while end < filtered_items.len()
                && (accumulated_after < RENDER_MARGIN || items_after == 0)
            {
                if let Some((_, item)) = filtered_items.get(end) {
                    accumulated_after +=
                        Self::get_activity_item_height(state, item, line_height, available_width);
                }
                end += 1;
                items_after += 1;
            }

            log::debug!(
                "[VSCROLL] Pixel-based buffer: {}..{} (first_vis={}, last_vis={}, before={:.0}px, after={:.0}px)",
                start, end, first, last, accumulated_before, accumulated_after
            );

            (start, end)
        } else {
            // This should never happen if our heights are correct
            log::error!(
                "[VSCROLL] CRITICAL: No visible items found! viewport={:.0}-{:.0}, total_height={:.0}, item_count={}",
                viewport_start, viewport_end, current_y, filtered_items.len()
            );

            // Handle empty list case
            if filtered_items.is_empty() {
                (0, 0)
            } else {
                // Just show first few items as a fallback
                let count = BUFFER_ITEMS.min(filtered_items.len());
                (0, count)
            }
        };

        state.activity_log_visible_range = start_idx..end_idx;

        // DEBUG: Enhanced visible range logging
        log::debug!(
            "[VSCROLL] Visible range: {:?} ({}..{}), First visible: {:?}, Last visible: {:?}",
            state.activity_log_visible_range,
            start_idx,
            end_idx,
            first_visible,
            last_visible
        );

        log::debug!(
            "[VSCROLL] Rendering {} items (indices {}..{}) of {} total, viewport: {:.0}-{:.0} pixels",
            end_idx - start_idx,
            start_idx,
            end_idx,
            filtered_items.len(),
            viewport_start,
            viewport_end
        );

        // Only render visible items
        let mut rendered_items: Vec<Element> = Vec::new();

        // Calculate Y offset for items before our render range
        // IMPORTANT: We need the offset to start_idx (what we're actually rendering),
        // not first_visible (what's in viewport), to position content correctly
        let mut y_offset_before_visible = 0.0;
        for idx in 0..start_idx {
            if let Some((orig_idx, item)) = filtered_items.get(idx) {
                y_offset_before_visible +=
                    Self::get_activity_item_height(state, item, line_height, available_width);
            }
        }

        log::debug!(
            "[VSCROLL] y_offset_before_visible={:.0} (sum of {} items before start_idx={})",
            y_offset_before_visible,
            start_idx,
            start_idx
        );

        // Track global byte position for the entire document
        // Only count items that contribute to selectable text
        let mut document_byte_offset = 0usize;

        // Calculate byte offset up to the first visible item
        // Only include items that are part of selectable text (exclude commands)
        for idx in 0..start_idx {
            if let Some((_, item)) = filtered_items.get(idx) {
                if should_include_item_in_selection_offset(item) {
                    let item_text = get_item_text(item);
                    document_byte_offset += item_text.len();
                    // Add newline between items (except after the last one)
                    if idx < filtered_items.len() - 1 {
                        document_byte_offset += 1;
                    }
                }
            }
        }

        // Render visible items with proper global byte offsets
        for idx in state.activity_log_visible_range.clone() {
            if let Some((orig_idx, item)) = filtered_items.get(idx) {
                // Enhanced logging to debug offset issues
                let mut element = Self::render_activity_item(
                    state,
                    item,
                    fonts,
                    *orig_idx,
                    palette,
                    document_byte_offset,
                    width,
                    selection_state,
                    code_block_registry.clone(),
                );

                // Update document byte offset for next item
                // Only increment offset for items that contribute to selectable text
                if should_include_item_in_selection_offset(item) {
                    let item_text = get_item_text(item);
                    document_byte_offset += item_text.len();
                    // Add newline between items (except after the last one)
                    if idx < filtered_items.len() - 1 {
                        document_byte_offset += 1;
                    }
                }

                // Attach cached height if available
                let item_id = match item {
                    ActivityItem::Command { id, .. } => id.clone(),
                    ActivityItem::Chat { id, .. } => id.clone(),
                    ActivityItem::Suggestion { id, .. } => id.clone(),
                    ActivityItem::Goal { id, .. } => id.clone(),
                };
                if let Some(height) = state.activity_log_height_cache.get(&item_id) {
                    element = element.with_computed_height(*height);
                }

                rendered_items.push(element);
            }
        }

        log::debug!(
            "[VSCROLL] Rendering {} visible items (of {} total), visible range: {:?}",
            rendered_items.len(),
            filtered_items.len(),
            state.activity_log_visible_range.clone()
        );

        // Calculate total content height
        let total_content_height = Self::calculate_total_activity_log_height(
            &filtered_items,
            line_height,
            available_width,
        );

        // Log height information
        log::debug!(
            "Total content height: {} pixels, scroll_offset: {}, max valid scroll: {}",
            total_content_height,
            state.activity_log_scroll_offset,
            (total_content_height - available_height).max(0.0)
        );

        // Before updating total height, calculate visual anchor if heights are changing
        let old_height = state
            .activity_log_scrollbar
            .as_ref()
            .map(|s| s.content_height)
            .unwrap_or(0.0);

        let height_changing = (old_height - total_content_height).abs() > 1.0;

        // TEMPORARILY DISABLED: Visual anchor system to fix scrolling jumps
        // if height_changing {
        //     // Calculate anchor before any changes
        //     state.visual_anchor = self.calculate_visual_anchor(
        //         &filtered_items,
        //         line_height,
        //         available_width,
        //         available_height
        //     );
        //
        //     log::info!(
        //         "Total content height changing: {} -> {} (delta: {}, cache size: {})",
        //         old_height,
        //         total_content_height,
        //         total_content_height - old_height,
        //         state.activity_log_height_cache.len()
        //     );
        // }

        // DEBUG: Log comprehensive state information
        log::debug!(
            "[VSCROLL] Total items: {}, Filtered: {}, Total height: {:.0}px, Scroll: {:.0}px, Viewport: {:.0}px, Max scroll: {:.0}px",
            activity_log.len(),
            filtered_items.len(),
            total_content_height,
            state.activity_log_scroll_offset,
            available_height,
            (total_content_height - available_height).max(0.0)
        );

        // Debug: Show what's at the end of the list
        if let Some((idx, last_item)) = filtered_items.last() {
            let last_height =
                Self::get_activity_item_height(state, last_item, line_height, available_width);
            let last_id = match last_item {
                ActivityItem::Command { id, .. } => id,
                ActivityItem::Chat { id, .. } => id,
                ActivityItem::Suggestion { id, .. } => id,
                ActivityItem::Goal { id, .. } => id,
            };
            let is_cached = state.activity_log_height_cache.contains_key(last_id);
            log::debug!(
                "[VSCROLL] Last item: {} (idx={}, height={:.0}px, cached={}), can_reach_end={}",
                last_id,
                idx,
                last_height,
                is_cached,
                state.activity_log_scroll_offset + available_height >= total_content_height - 10.0
            );

            // Check last few items to see if they have cached heights
            let last_5_uncached = filtered_items
                .iter()
                .rev()
                .take(5)
                .filter(|(_, item)| {
                    let id = match item {
                        ActivityItem::Command { id, .. } => id,
                        ActivityItem::Chat { id, .. } => id,
                        ActivityItem::Suggestion { id, .. } => id,
                        ActivityItem::Goal { id, .. } => id,
                    };
                    !state.activity_log_height_cache.contains_key(id)
                })
                .count();
            if last_5_uncached > 0 {
                log::debug!(
                    "[VSCROLL] {} of last 5 items are using estimated heights (never been visible)",
                    last_5_uncached
                );

                // Show which specific items are uncached
                let uncached_info: Vec<String> = filtered_items
                    .iter()
                    .rev()
                    .take(5)
                    .filter_map(|(idx, item)| {
                        let id = match item {
                            ActivityItem::Command { id, .. } => id,
                            ActivityItem::Chat { id, .. } => id,
                            ActivityItem::Suggestion { id, .. } => id,
                            ActivityItem::Goal { id, .. } => id,
                        };
                        if !state.activity_log_height_cache.contains_key(id) {
                            Some(format!("{} (idx={})", id, idx))
                        } else {
                            None
                        }
                    })
                    .collect();
                log::debug!("[VSCROLL] Uncached items: {:?}", uncached_info);
            }
        }

        // Ensure scroll offset is within valid bounds
        let max_valid_scroll = (total_content_height - available_height).max(0.0);
        if state.activity_log_scroll_offset > max_valid_scroll {
            log::warn!(
                "Scroll offset {} exceeds max valid scroll {}, clamping",
                state.activity_log_scroll_offset,
                max_valid_scroll
            );
            state.activity_log_scroll_offset = max_valid_scroll;
        }

        // Update scrollbar state
        let scrollbar_info = ScrollbarInfo {
            should_show: total_content_height > available_height,
            thumb_position: if total_content_height > available_height {
                state.activity_log_scroll_offset / (total_content_height - available_height)
            } else {
                0.0
            },
            thumb_size: (available_height / total_content_height).min(1.0).max(0.1),
            content_height: total_content_height,
            viewport_height: available_height,
            scroll_offset: state.activity_log_scroll_offset,
            total_items: filtered_items.len(),
            viewport_items: state.activity_log_visible_range.len(),
        };

        state.activity_log_scrollbar = Some(scrollbar_info.clone());

        // Update scrollbar renderer
        if scrollbar_info.should_show {
            match &mut state.activity_log_scrollbar_renderer {
                Some(renderer) => {
                    renderer.update(
                        total_content_height,
                        available_height,
                        state.activity_log_scroll_offset,
                    );
                }
                None => {
                    state.activity_log_scrollbar_renderer = Some(ScrollbarRenderer::new_vertical(
                        total_content_height,
                        available_height,
                        state.activity_log_scroll_offset,
                        20.0, // min thumb size
                    ));
                }
            }
        } else {
            state.activity_log_scrollbar_renderer = None;
        }

        // Create scrollable container with only visible elements
        let margin_top = -state.activity_log_scroll_offset + y_offset_before_visible;
        log::debug!(
            "[VSCROLL] Content positioning: scroll_offset={:.0}, y_offset_before_visible={:.0}, margin_top={:.0}, start_idx={}, first_visible={:?}",
            state.activity_log_scroll_offset,
            y_offset_before_visible,
            margin_top,
            start_idx,
            first_visible
        );

        let content_area = Element::new(&fonts.body, ElementContent::Children(rendered_items))
            .display(DisplayType::Block)
            .margin(BoxDimension {
                top: Dimension::Pixels(margin_top),
                ..Default::default()
            });

        // Create viewport container with fixed height and clipping
        let viewport = Element::new(&fonts.body, ElementContent::Children(vec![content_area]))
            .display(DisplayType::Block)
            .min_height(Some(Dimension::Pixels(available_height)));

        // Log diagnostics when content might be invisible
        if margin_top < -5000.0 || state.activity_log_scroll_offset > total_content_height {
            log::debug!(
                "Potential visibility issue: margin_top={}, scroll_offset={}, total_height={}, viewport_height={}",
                margin_top,
                state.activity_log_scroll_offset,
                total_content_height,
                available_height
            );
        }

        viewport
    }

    // Helper function copied from ai_sidebar.rs
    fn render_activity_item(
        state: &ActivityLogState,
        item: &ActivityItem,
        fonts: &SidebarFonts,
        item_index: usize,
        palette: &wezterm_term::color::ColorPalette,
        global_byte_offset: usize,
        width: u16,
        selection_state: &crate::sidebar::text_selection::SelectionState,
        code_block_registry: Option<Arc<Mutex<HashMap<String, CodeBlockContainer>>>>,
    ) -> Element {
        use super::components::markdown::CodeBlockContainer;
        match item {
            ActivityItem::Command {
                command,
                output,
                status,
                expanded,
                ..
            } => {
                let status_icon = match status {
                    CommandStatus::Running => "◐",
                    CommandStatus::Success => "✓",
                    CommandStatus::Failed(_) => "✕",
                };

                let status_color = match status {
                    CommandStatus::Running => LinearRgba::with_components(0.5, 0.7, 1.0, 1.0),
                    CommandStatus::Success => LinearRgba::with_components(0.4, 0.8, 0.4, 1.0),
                    CommandStatus::Failed(_) => LinearRgba::with_components(0.9, 0.4, 0.4, 1.0),
                };

                // Calculate available width for command content
                let sidebar_width = width as f32;
                let content_width = sidebar_width
                    - CHAT_ITEM_HORIZONTAL_MARGIN
                    - (CHAT_ITEM_PADDING * 2.0)
                    - (CHAT_ITEM_BORDER * 2.0)
                    - SCROLLBAR_SPACE;

                // Create command element with status icon
                // Commands don't contribute to selection offset, so don't set global_byte_offset
                let command_text = format!("{} {}", status_icon, command);
                let command_element = Element::new(
                    &fonts.body,
                    ElementContent::WrappedText(command_text.clone()),
                )
                .colors(ElementColors {
                    text: status_color.into(),
                    ..Default::default()
                })
                .max_width(Some(Dimension::Pixels(content_width)));

                let mut content = vec![command_element];

                // Add output if expanded
                if *expanded && output.is_some() {
                    // Commands don't contribute to selection, don't set global_byte_offset
                    content.push(
                        Element::new(
                            &fonts.body,
                            ElementContent::WrappedText(output.as_ref().unwrap().clone()),
                        )
                        .colors(ElementColors {
                            text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
                            ..Default::default()
                        })
                        .padding(BoxDimension {
                            top: Dimension::Pixels(8.0),
                            ..Default::default()
                        })
                        .max_width(Some(Dimension::Pixels(content_width))),
                    );
                }

                Card::new()
                    .with_content(content)
                    .render(&fonts.body)
                    .item_type(UIItemType::ActivityItemText {
                        index: item_index,
                        char_positions: Vec::new(), // Position data extracted after rendering
                    })
            }
            ActivityItem::Chat {
                message, is_user, ..
            } => {
                // Calculate available width for chat content
                let sidebar_width = width as f32;
                let content_width = sidebar_width
                    - CHAT_ITEM_HORIZONTAL_MARGIN
                    - (CHAT_ITEM_PADDING * 2.0)
                    - (CHAT_ITEM_BORDER * 2.0)
                    - SCROLLBAR_SPACE;

                let bg_color = if *is_user {
                    LinearRgba::with_components(0.05, 0.15, 0.25, 1.0) // 50% darker, full opacity
                } else {
                    LinearRgba::with_components(0.15, 0.15, 0.17, 1.0)
                };

                // Check if this message has a selection
                let selection = match &selection_state.active_selection {
                    Some(SelectionTarget::ActivityItem {
                        anchor_index,
                        anchor_byte,
                        current_index,
                        current_byte,
                    }) if *anchor_index == item_index || *current_index == item_index => {
                        // Determine selection bounds for this item
                        if *anchor_index == *current_index && *anchor_index == item_index {
                            // Single item selection
                            Some((
                                *anchor_byte.min(current_byte),
                                *anchor_byte.max(current_byte),
                            ))
                        } else if *anchor_index.min(current_index) == item_index {
                            // This is the first item in multi-item selection
                            let byte_start = if *anchor_index == item_index {
                                *anchor_byte
                            } else {
                                *current_byte
                            };
                            Some((byte_start, usize::MAX)) // Select to end
                        } else if *anchor_index.max(current_index) == item_index {
                            // This is the last item in multi-item selection
                            let byte_end = if *anchor_index == item_index {
                                *anchor_byte
                            } else {
                                *current_byte
                            };
                            Some((0, byte_end)) // Select from start
                        } else if item_index > *anchor_index.min(current_index)
                            && item_index < *anchor_index.max(current_index)
                        {
                            // This is a middle item - select entire text
                            Some((0, usize::MAX))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // Render message content with markdown if it's from AI
                let content = if *is_user {
                    // User messages - use WrappedText for proper width calculation
                    // Selection is now rendered as an overlay, not inline styles
                    Element::new(&fonts.body, ElementContent::WrappedText(message.clone()))
                        .colors(ElementColors {
                            text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                            ..Default::default()
                        })
                        .max_width(Some(Dimension::Pixels(content_width)))
                        .global_byte_offset(0) // Use item-relative offset, not document-global
                } else {
                    // AI messages - use markdown rendering
                    // Selection is now rendered as an overlay, not inline styles
                    if selection.is_some() {
                        // When there's a selection, we still use markdown but the selection
                        // will be rendered as an overlay
                        MarkdownRenderer::render_with_fonts(
                            message,
                            fonts,
                            Some(content_width),
                            code_block_registry.clone(),
                            Some(&format!("activity_{}", item_index)),
                            Some(palette),
                        )
                    } else {
                        // AI messages use markdown rendering with code font support
                        // Need to add width constraint for proper text wrapping
                        let sidebar_width = width as f32;
                        // Calculate available width accounting for all padding/margins:
                        // - Activity log container: no explicit padding
                        // - Chat message margin: CHAT_ITEM_HORIZONTAL_MARGIN on one side
                        // - Chat message padding: CHAT_ITEM_PADDING * 2
                        // - Chat message border: CHAT_ITEM_BORDER * 2
                        // - Scrollbar space: SCROLLBAR_SPACE
                        let spacing = CHAT_ITEM_HORIZONTAL_MARGIN
                            + (CHAT_ITEM_PADDING * 2.0)
                            + (CHAT_ITEM_BORDER * 2.0)
                            + SCROLLBAR_SPACE;
                        let content_width = sidebar_width - spacing;
                        log::debug!(
                            "Rendering markdown in activity log: sidebar_width={}, content_width={}",
                            sidebar_width,
                            content_width
                        );

                        // Use registry if available for horizontal scrolling support
                        MarkdownRenderer::render_with_fonts(
                            message,
                            fonts,
                            Some(content_width),
                            code_block_registry.clone(),
                            Some(&format!("activity_{}", item_index)),
                            Some(palette),
                        )
                    }
                };

                Element::new(&fonts.body, ElementContent::Children(vec![content]))
                    .display(DisplayType::Block)
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(CHAT_ITEM_PADDING)))
                    .margin(BoxDimension {
                        left: if *is_user {
                            Dimension::Pixels(CHAT_ITEM_HORIZONTAL_MARGIN)
                        } else {
                            Dimension::Pixels(0.0)
                        },
                        right: Dimension::Pixels(0.0), // No right margin for either
                        bottom: Dimension::Pixels(CHAT_ITEM_BOTTOM_MARGIN),
                        ..Default::default()
                    })
                    .item_type(UIItemType::ActivityItemText {
                        index: item_index,
                        char_positions: Vec::new(), // Position data extracted after rendering
                    })
            }
            ActivityItem::Suggestion { title, content, .. } => {
                // Add width constraint for proper text wrapping
                let sidebar_width = width as f32;
                // Calculate available width for suggestion card content:
                // - Card margin: 8px each side = 16px
                // - Card padding: 12px each side = 24px
                // - Card border: 1px each side = 2px
                // - Scrollbar space: ~12px
                // Total: 16 + 24 + 2 + 12 = 54px
                let content_width = sidebar_width - 54.0;
                let markdown_content = MarkdownRenderer::render_with_fonts(
                    content,
                    fonts,
                    Some(content_width),
                    code_block_registry.clone(),
                    Some(&format!("suggestion_{}", item_index)),
                    Some(palette),
                );

                Card::new()
                    .with_title(format!("Past: {}", title))
                    .with_content(vec![markdown_content])
                    .render(&fonts.heading)
                    .item_type(UIItemType::ActivityItemText {
                        index: item_index,
                        char_positions: Vec::new(), // Position data extracted after rendering
                    })
            }
            ActivityItem::Goal { text, .. } => Element::new(
                &fonts.body,
                ElementContent::WrappedText(format!("Goal: {}", text)),
            )
            .colors(ElementColors {
                text: LinearRgba::with_components(0.8, 0.8, 0.8, 1.0).into(),
                ..Default::default()
            })
            .padding(BoxDimension::new(Dimension::Pixels(8.0)))
            .item_type(UIItemType::ActivityItemText {
                index: item_index,
                char_positions: Vec::new(), // Position data extracted after rendering
            }),
        }
    }

    // Helper functions copied from ai_sidebar.rs
    fn get_activity_item_height(
        state: &ActivityLogState,
        item: &ActivityItem,
        line_height: f32,
        available_width: f32,
    ) -> f32 {
        let id = match item {
            ActivityItem::Command { id, .. } => id.clone(),
            ActivityItem::Chat { id, .. } => id.clone(),
            ActivityItem::Suggestion { id, .. } => id.clone(),
            ActivityItem::Goal { id, .. } => id.clone(),
        };

        if let Some(cached_height) = state.activity_log_height_cache.get(&id) {
            let estimated = estimate_activity_item_height(item, line_height, available_width);
            if (cached_height - estimated).abs() > 100.0 {
                log::info!(
                    "[VSCROLL] Large height difference for {}: cached={:.0} vs estimated={:.0} (delta={:.0})",
                    id, cached_height, estimated, cached_height - estimated
                );
            }
            *cached_height
        } else {
            estimate_activity_item_height(item, line_height, available_width)
        }
    }

    fn calculate_total_activity_log_height(
        filtered_items: &[(usize, &ActivityItem)],
        line_height: f32,
        available_width: f32,
    ) -> f32 {
        filtered_items
            .iter()
            .map(|(_, item)| estimate_activity_item_height(item, line_height, available_width))
            .sum::<f32>()
        // Removed +20px hack - virtual scrolling with accurate height caching handles this correctly
    }
}

// Global helper functions copied from ai_sidebar.rs
fn get_item_text(item: &ActivityItem) -> String {
    match item {
        ActivityItem::Chat { message, .. } => message.clone(),
        ActivityItem::Command {
            command,
            output,
            status,
            expanded,
            ..
        } => {
            // Match the rendering format exactly
            let status_icon = match status {
                CommandStatus::Running => "◐",
                CommandStatus::Success => "✓",
                CommandStatus::Failed(_) => "✕",
            };

            if *expanded && output.is_some() {
                format!(
                    "{} {}\n\n{}",
                    status_icon,
                    command,
                    output.as_ref().unwrap()
                )
            } else {
                format!("{} {}", status_icon, command)
            }
        }
        ActivityItem::Suggestion { content, .. } => content.clone(),
        ActivityItem::Goal { text, .. } => text.clone(),
    }
}

fn should_include_item_in_selection_offset(item: &ActivityItem) -> bool {
    true // All items are selectable and contribute to offset
}

fn estimate_activity_item_height(
    item: &ActivityItem,
    line_height: f32,
    available_width: f32,
) -> f32 {
    // Get the correct spacing for this item type
    let spacing = get_activity_item_spacing(item);

    match item {
        ActivityItem::Command {
            output, expanded, ..
        } => {
            // Command line height + spacing
            let mut height = line_height + spacing;

            // Add output height if expanded
            if *expanded {
                if let Some(output) = output {
                    let lines = output.lines().count() as f32;
                    height += lines * line_height + 16.0; // Extra padding for output
                }
            }

            height
        }
        ActivityItem::Chat {
            message, is_user, ..
        } => {
            // Estimate wrapped text height
            let horizontal_margin = CHAT_ITEM_HORIZONTAL_MARGIN; // Only one side has margin
            let horizontal_padding = CHAT_ITEM_PADDING * 2.0; // Left + right padding
            let border_width = CHAT_ITEM_BORDER * 2.0; // Left + right border
            let effective_width =
                available_width - horizontal_margin - horizontal_padding - border_width;
            let avg_char_width = line_height * 0.6; // Approximate

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                message,
                effective_width,
                avg_char_width,
            );

            lines * line_height + spacing
        }
        ActivityItem::Suggestion { content, .. } => {
            // Suggestions can be quite long with markdown
            let effective_width = available_width - spacing;
            let avg_char_width = line_height * 0.6;

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                content,
                effective_width,
                avg_char_width,
            );

            lines * line_height + spacing
        }
        ActivityItem::Goal { text, .. } => {
            // Goals are typically short
            let effective_width = available_width - spacing;
            let avg_char_width = line_height * 0.6;

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                text,
                effective_width,
                avg_char_width,
            );

            lines * line_height + spacing
        }
    }
}

/// Calculate the total height of all activity log items
fn calculate_total_activity_log_height(
    state: &ActivityLogState,
    filtered_items: &[(usize, &ActivityItem)],
    viewport_width: f32,
) -> f32 {
    let line_height = 25.0; // Default line height
    filtered_items
        .iter()
        .map(|(index, item)| {
            let item_id = get_item_id(item);
            // Use cached height if available, otherwise estimate
            state
                .activity_log_height_cache
                .get(&item_id)
                .copied()
                .unwrap_or_else(|| estimate_activity_item_height(item, line_height, viewport_width))
        })
        .sum()
}

/// Get a unique ID for an activity item
fn get_item_id(item: &ActivityItem) -> String {
    match item {
        ActivityItem::Command { id, .. } => id.clone(),
        ActivityItem::Chat { id, .. } => id.clone(),
        ActivityItem::Suggestion { id, .. } => id.clone(),
        ActivityItem::Goal { id, .. } => id.clone(),
    }
}

/// Get the spacing for an activity item (margin + padding)
fn get_activity_item_spacing(item: &ActivityItem) -> f32 {
    match item {
        ActivityItem::Command { .. } | ActivityItem::Suggestion { .. } => {
            CARD_DEFAULT_MARGIN + CARD_CONTENT_PADDING * 2.0 + CARD_BORDER * 2.0
        }
        ActivityItem::Chat { .. } => {
            CHAT_ITEM_BOTTOM_MARGIN + CHAT_ITEM_PADDING * 2.0 + CHAT_ITEM_BORDER * 2.0
        }
        ActivityItem::Goal { .. } => {
            CARD_DEFAULT_MARGIN + GOAL_CARD_PADDING * 2.0 + CARD_BORDER * 2.0
        }
    }
}
