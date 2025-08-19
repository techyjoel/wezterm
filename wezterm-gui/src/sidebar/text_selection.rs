//! Text selection management for the AI sidebar
//!
//! This module handles text selection state, hit testing, and rendering
//! for selectable text across all sidebar components.

use crate::sidebar::ai_sidebar::{CurrentGoal, CurrentSuggestion};
use crate::sidebar::position_cache::{
    ElementType, ItemPosition, ItemPositionData, TextAffinity, TextPosition,
};
use crate::sidebar::sidebar_constants::*;
use crate::sidebar::{ActivityItem, AiSidebar};
use euclid::{Point2D, Rect};
use std::collections::HashMap;
use window::PixelUnit;

/// Tracks text selection state in the sidebar
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub active_selection: Option<SelectionTarget>,
    pub prepared_selection: Option<SelectionTarget>,
    pub is_dragging: bool,
}

impl SelectionState {
    pub fn clear(&mut self) {
        self.active_selection = None;
        self.prepared_selection = None;
        self.is_dragging = false;
    }
}

#[derive(Debug, Clone)]
pub enum SelectionTarget {
    ActivityItem {
        // Anchor position (where selection started)
        anchor_index: usize,
        anchor_byte: usize,
        // Current position (where selection ends)
        current_index: usize,
        current_byte: usize,
    },
    Suggestion {
        anchor_byte: usize,
        current_byte: usize,
    },
    Goal {
        anchor_byte: usize,
        current_byte: usize,
    },
    ChatInput {
        anchor_line: usize,
        anchor_byte: usize,
        current_line: usize,
        current_byte: usize,
    },
}

impl SelectionState {
    pub fn get_selected_text(&self, sidebar: &AiSidebar) -> Option<String> {
        self.get_selected_text_with_positions(sidebar, &sidebar.item_positions)
    }

    pub fn get_selected_text_with_positions(
        &self,
        sidebar: &AiSidebar,
        item_positions: &HashMap<usize, ItemPositionData>,
    ) -> Option<String> {
        match &self.active_selection {
            None => None,
            Some(selection) => match selection {
                SelectionTarget::ActivityItem {
                    anchor_index,
                    anchor_byte,
                    current_index,
                    current_byte,
                } => {
                    // Handle single-item or multi-item selection
                    if anchor_index == current_index {
                        // Single item selection
                        item_positions.get(anchor_index).and_then(|position_data| {
                            let start = anchor_byte.min(current_byte);
                            let end = anchor_byte.max(current_byte);

                            // Use the smart text extraction that skips artificial newlines
                            let selected = position_data.get_selection_text(*start, *end);
                            if !selected.is_empty() {
                                Some(selected)
                            } else {
                                log::warn!(
                                    "Empty selection: start={}, end={}, text_len={}",
                                    start,
                                    end,
                                    position_data.rendered_text.len()
                                );
                                None
                            }
                        })
                    } else {
                        // Multi-item selection
                        let start_index = anchor_index.min(current_index);
                        let end_index = anchor_index.max(current_index);
                        let mut selected_text = String::new();

                        for index in *start_index..=*end_index {
                            if let Some(position_data) = item_positions.get(&index) {
                                if index == *start_index && index == *anchor_index {
                                    // First item, from anchor_byte to end
                                    let text = position_data.get_selection_text(
                                        *anchor_byte,
                                        position_data.rendered_text.len(),
                                    );
                                    selected_text.push_str(&text);
                                } else if index == *start_index {
                                    // First item, from current_byte to end
                                    let text = position_data.get_selection_text(
                                        *current_byte,
                                        position_data.rendered_text.len(),
                                    );
                                    selected_text.push_str(&text);
                                } else if index == *end_index && index == *anchor_index {
                                    // Last item, from start to anchor_byte
                                    let text = position_data.get_selection_text(0, *anchor_byte);
                                    selected_text.push_str(&text);
                                } else if index == *end_index {
                                    // Last item, from start to current_byte
                                    let text = position_data.get_selection_text(0, *current_byte);
                                    selected_text.push_str(&text);
                                } else {
                                    // Middle items, entire text (without artificial newlines)
                                    let text = position_data
                                        .get_selection_text(0, position_data.rendered_text.len());
                                    selected_text.push_str(&text);
                                }

                                // Add newline between items (this is a REAL newline between different activity items)
                                if index < *end_index {
                                    selected_text.push('\n');
                                }
                            }
                        }

                        if selected_text.is_empty() {
                            None
                        } else {
                            Some(selected_text)
                        }
                    }
                }
                SelectionTarget::Suggestion {
                    anchor_byte,
                    current_byte,
                } => {
                    if let Some(suggestion) = &sidebar.current_suggestion {
                        let start = anchor_byte.min(current_byte);
                        let end = anchor_byte.max(current_byte);
                        suggestion.content.get(*start..*end).map(|s| s.to_string())
                    } else {
                        None
                    }
                }
                SelectionTarget::Goal {
                    anchor_byte,
                    current_byte,
                } => {
                    if let Some(goal) = &sidebar.current_goal {
                        let start = anchor_byte.min(current_byte);
                        let end = anchor_byte.max(current_byte);
                        goal.text.get(*start..*end).map(|s| s.to_string())
                    } else {
                        None
                    }
                }
                SelectionTarget::ChatInput {
                    anchor_line,
                    anchor_byte,
                    current_line,
                    current_byte,
                } => {
                    // Get text from multi-line input
                    let input_text = sidebar.chat_input.get_text();
                    if input_text.is_empty() {
                        return None;
                    }

                    // Convert line/byte positions to absolute byte offsets
                    let lines: Vec<_> = input_text.lines().collect();
                    if lines.is_empty() {
                        return None;
                    }

                    // Calculate start and end byte offsets
                    let (start_line, start_byte, end_line, end_byte) =
                        if (anchor_line, anchor_byte) <= (current_line, current_byte) {
                            (*anchor_line, *anchor_byte, *current_line, *current_byte)
                        } else {
                            (*current_line, *current_byte, *anchor_line, *anchor_byte)
                        };

                    let mut result = String::new();
                    for line_idx in start_line..=end_line {
                        if line_idx >= lines.len() {
                            break;
                        }

                        let line = lines[line_idx];
                        if line_idx == start_line && line_idx == end_line {
                            // Selection within single line
                            if let Some(text) = line.get(start_byte..end_byte.min(line.len())) {
                                result.push_str(text);
                            }
                        } else if line_idx == start_line {
                            // First line of multi-line selection
                            if let Some(text) = line.get(start_byte..) {
                                result.push_str(text);
                                result.push('\n');
                            }
                        } else if line_idx == end_line {
                            // Last line of multi-line selection
                            if let Some(text) = line.get(..end_byte.min(line.len())) {
                                result.push_str(text);
                            }
                        } else {
                            // Middle lines
                            result.push_str(line);
                            result.push('\n');
                        }
                    }

                    if result.is_empty() {
                        None
                    } else {
                        Some(result)
                    }
                }
            },
        }
    }
}

/// Manager for text selection operations
pub struct TextSelectionManager;

impl TextSelectionManager {
    /// Hit test text positions to find the byte offset at a given point
    pub fn hit_test_text_positions(
        positions: &[TextPosition],
        point: Point2D<f32, PixelUnit>,
        element_type: &ElementType,
    ) -> Option<ItemPosition> {
        log::debug!("hit_test_text_positions: point=({:.1}, {:.1}), {} positions", 
            point.x, point.y, positions.len());
        log::debug!(
            "hit_test_text_positions: Testing point ({:.1}, {:.1}) against {} positions",
            point.x,
            point.y,
            positions.len()
        );

        // Debug: Log the y-coordinate range of positions
        if !positions.is_empty() {
            let min_y = positions
                .iter()
                .map(|p| p.y)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0);
            let max_y = positions
                .iter()
                .map(|p| p.y)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0);
            log::debug!(
                "  Position y-range: {:.1} to {:.1}, click y: {:.1}",
                min_y,
                max_y,
                point.y
            );
        }

        // Find the line containing the y coordinate
        let line_positions: Vec<_> = positions
            .iter()
            .filter(|p| {
                // Check if point is within line height
                let line_height = match element_type {
                    ElementType::Paragraph { line_height, .. } => *line_height,
                    ElementType::Heading { font_size, .. } => font_size * 1.2,
                    ElementType::CodeBlock { line_height, .. } => *line_height,
                    _ => PARAGRAPH_LINE_HEIGHT, // Default line height
                };
                point.y >= p.y && point.y < p.y + line_height
            })
            .collect();

        if line_positions.is_empty() {
            log::debug!("  No positions found on line at y={:.1}", point.y);
            return None;
        }

        // Find the closest position on the line
        let mut best_position = None;
        let mut best_distance = f32::MAX;

        for pos in &line_positions {
            if point.x >= pos.x_start && point.x < pos.x_end {
                // Point is within this glyph
                let mid = (pos.x_start + pos.x_end) / 2.0;
                if point.x < mid {
                    return Some(ItemPosition {
                        byte_offset: pos.byte_offset,
                        affinity: TextAffinity::Leading,
                    });
                } else {
                    // Find the next position if available
                    let next_offset = line_positions
                        .iter()
                        .find(|p| p.byte_offset > pos.byte_offset)
                        .map(|p| p.byte_offset)
                        .unwrap_or(pos.byte_offset + 1);
                    return Some(ItemPosition {
                        byte_offset: next_offset,
                        affinity: TextAffinity::Leading,
                    });
                }
            }

            // Track closest position for edge cases
            let dist_to_start = (point.x - pos.x_start).abs();
            let dist_to_end = (point.x - pos.x_end).abs();

            if dist_to_start < best_distance {
                best_distance = dist_to_start;
                best_position = Some(ItemPosition {
                    byte_offset: pos.byte_offset,
                    affinity: TextAffinity::Leading,
                });
            }

            if dist_to_end < best_distance {
                best_distance = dist_to_end;
                // Use next byte offset if available
                let next_offset = line_positions
                    .iter()
                    .find(|p| p.byte_offset > pos.byte_offset)
                    .map(|p| p.byte_offset)
                    .unwrap_or(pos.byte_offset + 1);
                best_position = Some(ItemPosition {
                    byte_offset: next_offset,
                    affinity: TextAffinity::Leading,
                });
            }
        }

        best_position
    }

    /// Calculate selection rectangles for rendering
    pub fn calculate_selection_rectangles(
        selection_state: &SelectionState,
        sidebar: &AiSidebar,
    ) -> Vec<Rect<f32, PixelUnit>> {
        let mut rects = Vec::new();

        match &selection_state.active_selection {
            None => {}
            Some(selection) => match selection {
                SelectionTarget::ActivityItem {
                    anchor_index,
                    anchor_byte,
                    current_index,
                    current_byte,
                } => {
                    let start_index = anchor_index.min(current_index);
                    let end_index = anchor_index.max(current_index);

                    for index in *start_index..=*end_index {
                        if let Some(bounds) = sidebar.get_activity_item_bounds(index) {
                            if let Some(position_data) = sidebar.item_positions.get(&index) {
                                // Determine byte range for this item
                                let (item_start_byte, item_end_byte) =
                                    if index == *start_index && index == *end_index {
                                        // Single item selection
                                        let start = anchor_byte.min(current_byte);
                                        let end = anchor_byte.max(current_byte);
                                        (*start, *end)
                                    } else if index == *start_index {
                                        // First item in multi-item selection
                                        if start_index == anchor_index {
                                            (*anchor_byte, position_data.rendered_text.len())
                                        } else {
                                            (*current_byte, position_data.rendered_text.len())
                                        }
                                    } else if index == *end_index {
                                        // Last item in multi-item selection
                                        if end_index == anchor_index {
                                            (0, *anchor_byte)
                                        } else {
                                            (0, *current_byte)
                                        }
                                    } else {
                                        // Middle item - select entire text
                                        (0, position_data.rendered_text.len())
                                    };

                                // Skip if no actual selection
                                if item_start_byte >= item_end_byte {
                                    continue;
                                }

                                // Calculate selection rectangles for this item
                                let item_rects = position_data
                                    .position_tree
                                    .calculate_local_selection_rectangles(
                                        item_start_byte,
                                        item_end_byte,
                                        euclid::Vector2D::new(0.0, 0.0),
                                    );

                                // Transform to absolute coordinates
                                for rect in item_rects {
                                    let absolute_rect = Rect::new(
                                        Point2D::new(
                                            bounds.origin.x + rect.origin.x,
                                            bounds.origin.y + rect.origin.y,
                                        ),
                                        rect.size,
                                    );
                                    rects.push(absolute_rect);
                                }
                            }
                        }
                    }
                }
                SelectionTarget::Suggestion {
                    anchor_byte,
                    current_byte,
                } => {
                    if let Some(bounds) = &sidebar.suggestion_bounds {
                        let start = anchor_byte.min(current_byte);
                        let end = anchor_byte.max(current_byte);
                        if start != end {
                            if let Some(suggestion) = &sidebar.current_suggestion {
                                // Calculate selection rectangles
                                let padding = 8.0;
                                let char_width = 8.5;
                                let line_height = PARAGRAPH_LINE_HEIGHT;

                                let start_char = suggestion.content.chars().take(*start).count();
                                let end_char = suggestion.content.chars().take(*end).count();

                                let start_x =
                                    bounds.origin.x + padding + (start_char as f32 * char_width);
                                let end_x =
                                    bounds.origin.x + padding + (end_char as f32 * char_width);

                                let max_x = bounds.origin.x + bounds.size.width - padding;
                                let end_x = end_x.min(max_x);

                                rects.push(euclid::rect(
                                    start_x,
                                    bounds.origin.y + padding,
                                    end_x - start_x,
                                    line_height,
                                ));
                            }
                        }
                    }
                }
                SelectionTarget::Goal {
                    anchor_byte,
                    current_byte,
                } => {
                    if let Some(bounds) = &sidebar.goal_bounds {
                        let start = anchor_byte.min(current_byte);
                        let end = anchor_byte.max(current_byte);

                        if start != end {
                            if let Some(goal) = &sidebar.current_goal {
                                // Use actual glyph positions if available
                                let (start_x, end_x) = if let Some(positions) =
                                    &sidebar.goal_char_positions
                                {
                                    let mut start_x_pos = bounds.origin.x + GOAL_CARD_PADDING;
                                    let mut end_x_pos = start_x_pos;

                                    // Find positions for byte offsets
                                    for &(x_start, x_end, byte_offset) in positions {
                                        if byte_offset == *start {
                                            start_x_pos =
                                                bounds.origin.x + GOAL_CARD_PADDING + x_start;
                                        }
                                        if byte_offset < *end {
                                            end_x_pos = bounds.origin.x + GOAL_CARD_PADDING + x_end;
                                        }
                                    }

                                    (start_x_pos, end_x_pos)
                                } else {
                                    // Fallback to approximation
                                    let char_width = 8.5;
                                    let start_char = goal.text.chars().take(*start).count();
                                    let end_char = goal.text.chars().take(*end).count();

                                    let start_x = bounds.origin.x
                                        + GOAL_CARD_PADDING
                                        + (start_char as f32 * char_width);
                                    let end_x = bounds.origin.x
                                        + GOAL_CARD_PADDING
                                        + (end_char as f32 * char_width);

                                    (start_x, end_x)
                                };

                                let line_height = PARAGRAPH_LINE_HEIGHT * 1.25; // Selection rectangle height
                                let vertical_offset = 4.0;

                                rects.push(euclid::rect(
                                    start_x,
                                    bounds.origin.y + GOAL_CARD_PADDING + vertical_offset,
                                    (end_x - start_x).max(2.0),
                                    line_height,
                                ));
                            }
                        }
                    }
                }
                SelectionTarget::ChatInput { .. } => {
                    // Chat input selection is handled by the chat input component itself
                }
            },
        }

        rects
    }
}
