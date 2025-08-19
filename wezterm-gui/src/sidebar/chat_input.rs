//! Chat input handling for the AI sidebar
//!
//! This module provides a stateless chat input handler that manages text input,
//! cursor positioning, selection, and scrolling for the chat interface.

use crate::color::LinearRgba;
use crate::sidebar::components::forms::MultilineTextInput;
use crate::sidebar::SidebarFonts;
use crate::sidebar::sidebar_constants::{
    DEFAULT_SCROLLBAR_WIDTH,
};
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, ComputedElement, Corners, DisplayType, Element, ElementColors,
    ElementContent, LayoutContext, SizedPoly, StyleSpan,
};
use crate::termwindow::UIItemType;
use config::Dimension;
use std::rc::Rc;
use termwiz::input::{KeyCode, Modifiers as KeyModifiers};
use wezterm_font::LoadedFont;
use window::PixelUnit;

// Constants to eliminate magic numbers
pub const LINE_HEIGHT_MULTIPLIER: f32 = 1.1; // Consistent line spacing multiplier
pub const ESTIMATED_LINE_HEIGHT: f32 = 20.0; // Default line height for estimations
pub const ESTIMATED_CHAR_WIDTH: f32 = 8.5; // Default character width for estimations
pub const SCROLLBAR_WIDTH: f32 = DEFAULT_SCROLLBAR_WIDTH; // Width of the scrollbar
pub const SCROLLBAR_THUMB_HEIGHT: f32 = 40.0; // Height of scrollbar thumb
pub const CHAT_INPUT_PADDING: f32 = 12.0; // Horizontal padding in chat input
pub const CHAT_INPUT_VERTICAL_PADDING: f32 = 8.0; // Vertical padding in chat input
pub const CHAT_INPUT_TEXT_PADDING: f32 = 4.0; // Per-line text element padding
pub const CHAT_INPUT_BORDER_THICKNESS: f32 = 1.0; // Border thickness for chat input

/// Handles chat input rendering and interaction
pub struct ChatInputHandler;

impl ChatInputHandler {
    /// Render the chat input text area with proper wrapping and scrolling
    pub fn render_chat_input_text(
        chat_input: &mut MultilineTextInput,
        font: &Rc<LoadedFont>,
        width: f32,
        viewport_height: f32,
    ) -> Element {
        let metrics = font.metrics();
        let line_height = metrics.cell_height.get() as f32;
        let line_height_with_spacing = line_height * LINE_HEIGHT_MULTIPLIER;

        // Use visual line count if available, otherwise actual line count
        let line_count = if chat_input.visual_line_count > 0 {
            chat_input.visual_line_count
        } else {
            chat_input.lines.len()
        };

        log::trace!(
            "render_chat_input_text: width={:.1}, viewport_height={:.1}, line_height={:.1}, line_height_with_spacing={:.1}, lines={}, focused={}",
            width, viewport_height, line_height, line_height_with_spacing, chat_input.lines.len(), chat_input.focused
        );

        // Calculate scroll offset
        let max_scroll =
            ((line_count as f32 * line_height_with_spacing) - viewport_height).max(0.0);
        let scroll_offset = if !chat_input.user_has_scrolled {
            // Auto-scroll to bottom for new content
            max_scroll
        } else {
            chat_input.scroll_pixel_offset.min(max_scroll)
        };

        // Update the scroll offset
        chat_input.scroll_pixel_offset = scroll_offset;

        // Prepare text content (not used currently but kept for reference)

        // Check if we should show placeholder
        let is_placeholder =
            chat_input.lines.len() == 1 && chat_input.lines[0].is_empty() && !chat_input.focused;

        let combined_text = if is_placeholder {
            chat_input.placeholder.clone()
        } else {
            let mut combined_text = String::new();
            for (idx, line) in chat_input.lines.iter().enumerate() {
                if idx > 0 {
                    combined_text.push('\n');
                }
                combined_text.push_str(line);
            }
            combined_text
        };

        // Build the text element with proper styling
        let text_color = if is_placeholder {
            LinearRgba::with_components(0.5, 0.5, 0.5, 0.7)
        } else {
            LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
        };

        let mut text_element = Element::new(&font, ElementContent::WrappedText(combined_text))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: LinearRgba::TRANSPARENT.into(),
                text: text_color.into(),
            })
            .max_width(Some(Dimension::Pixels(width)))
            .display(DisplayType::Block);

        // Add cursor rendering if focused
        if chat_input.focused && !is_placeholder {
            // TODO: Add cursor rendering logic here
            // This would involve calculating cursor position and adding a cursor element
        }

        // Apply scroll offset
        if scroll_offset > 0.0 {
            text_element = text_element.margin(BoxDimension {
                top: Dimension::Pixels(-scroll_offset),
                left: Dimension::Pixels(0.0),
                right: Dimension::Pixels(0.0),
                bottom: Dimension::Pixels(0.0),
            });
        }

        text_element
    }

    /// Render the complete chat input area including container and scrollbar
    pub fn render_chat_input(
        chat_input: &mut MultilineTextInput,
        fonts: &SidebarFonts,
        chat_input_bg_color: LinearRgba,
        chat_input_border_color: LinearRgba,
        width: f32,
    ) -> Element {
        let font = &fonts.body;
        let metrics = font.metrics();
        let line_height = metrics.cell_height.get() as f32;

        // Calculate viewport height based on display lines
        let viewport_height =
            (chat_input.display_lines as f32 * line_height * LINE_HEIGHT_MULTIPLIER) + CHAT_INPUT_VERTICAL_PADDING;

        // Store line positions for click detection
        let line_positions = vec![];

        // Create the text content
        let text_element = Self::render_chat_input_text(
            chat_input,
            font,
            width - (CHAT_INPUT_PADDING * 2.0), // Account for padding
            viewport_height,
        );

        // Calculate if scrollbar is needed
        let content_height = chat_input.lines.len() as f32 * line_height * LINE_HEIGHT_MULTIPLIER;
        let needs_scrollbar = content_height > viewport_height;

        // Create container with text and optional scrollbar
        let mut container_children = vec![text_element];

        if needs_scrollbar {
            // Calculate scrollbar position
            let scroll_ratio =
                chat_input.scroll_pixel_offset / (content_height - viewport_height).max(1.0);
            let thumb_position = scroll_ratio * (viewport_height - SCROLLBAR_THUMB_HEIGHT);

            // Create scrollbar element using a filled rectangle
            let scrollbar = Element::new(&font, ElementContent::Text(String::new()))
                .colors(ElementColors {
                    border: BorderColor::default(),
                    bg: LinearRgba::with_components(0.5, 0.5, 0.5, 0.3).into(),
                    text: LinearRgba::TRANSPARENT.into(),
                })
                .min_width(Some(Dimension::Pixels(SCROLLBAR_WIDTH)))
                .min_height(Some(Dimension::Pixels(SCROLLBAR_THUMB_HEIGHT)))
                .margin(BoxDimension {
                    top: Dimension::Pixels(thumb_position),
                    left: Dimension::Pixels(width - SCROLLBAR_WIDTH - 4.0),
                    right: Dimension::Pixels(0.0),
                    bottom: Dimension::Pixels(0.0),
                })
                .zindex(16);

            container_children.push(scrollbar);
        }

        // Create the container
        Element::new(&font, ElementContent::Children(container_children))
            .colors(ElementColors {
                border: BorderColor::new(chat_input_border_color.into()),
                bg: chat_input_bg_color.into(),
                text: LinearRgba::TRANSPARENT.into(),
            })
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .border_corners(Some(Corners {
                top_left: SizedPoly {
                    width: Dimension::Pixels(8.0),
                    height: Dimension::Pixels(8.0),
                    poly: Default::default(),
                },
                top_right: SizedPoly {
                    width: Dimension::Pixels(8.0),
                    height: Dimension::Pixels(8.0),
                    poly: Default::default(),
                },
                bottom_left: SizedPoly {
                    width: Dimension::Pixels(8.0),
                    height: Dimension::Pixels(8.0),
                    poly: Default::default(),
                },
                bottom_right: SizedPoly {
                    width: Dimension::Pixels(8.0),
                    height: Dimension::Pixels(8.0),
                    poly: Default::default(),
                },
            }))
            .padding(BoxDimension {
                left: Dimension::Pixels(CHAT_INPUT_PADDING),
                right: Dimension::Pixels(CHAT_INPUT_PADDING),
                top: Dimension::Pixels(CHAT_INPUT_VERTICAL_PADDING),
                bottom: Dimension::Pixels(CHAT_INPUT_VERTICAL_PADDING),
            })
            .min_height(Some(Dimension::Pixels(viewport_height)))
            .display(DisplayType::Block)
            .item_type(UIItemType::ChatInput { line_positions })
            .zindex(14)
    }

    /// Handle keyboard input for the chat area
    pub fn handle_key_event(
        chat_input: &mut MultilineTextInput,
        key: &KeyCode,
        modifiers: KeyModifiers,
    ) -> bool {
        // Special handling for Enter key - we want to control submission vs newline
        if matches!(key, KeyCode::Enter) && !modifiers.contains(KeyModifiers::SHIFT) {
            // Enter without shift should submit (handled by parent)
            return false;
        }

        // Delegate all other key handling to MultilineTextInput
        // which properly handles cursor movement, text editing, etc.
        match chat_input.handle_key_event(key, modifiers) {
            Ok(handled) => handled,
            Err(_) => false,
        }
    }

    /// Handle character input
    pub fn handle_char_input(chat_input: &mut MultilineTextInput, c: char) {
        // Delegate to the MultilineTextInput's insert_char method
        // which properly handles all the state updates
        chat_input.insert_char(c);
    }

    /// Handle mouse wheel scrolling
    pub fn handle_wheel_scroll(
        chat_input: &mut MultilineTextInput,
        delta: f32,
        line_height: f32,
    ) -> bool {
        // Negative delta scrolls up, positive scrolls down
        let old_offset = chat_input.scroll_pixel_offset;

        // Calculate content height
        let content_height = chat_input.lines.len() as f32 * line_height * 1.1;
        let viewport_height = chat_input.display_lines as f32 * line_height * 1.1;

        if content_height <= viewport_height {
            // No scrolling needed
            return false;
        }

        // Update scroll offset
        let max_scroll = (content_height - viewport_height).max(0.0);
        chat_input.scroll_pixel_offset = (chat_input.scroll_pixel_offset + delta)
            .max(0.0)
            .min(max_scroll);

        // Mark that user has scrolled
        if chat_input.scroll_pixel_offset != old_offset {
            chat_input.user_has_scrolled = true;
            return true;
        }

        false
    }

    /// Handle click events for cursor positioning
    pub fn handle_click(
        chat_input: &mut MultilineTextInput,
        relative_x: f32,
        relative_y: f32,
        line_positions: &[Vec<(f32, f32, usize)>],
        is_drag: bool,
        shift_held: bool,
    ) -> bool {
        // Find which visual line was clicked
        let line_height = ESTIMATED_LINE_HEIGHT * LINE_HEIGHT_MULTIPLIER;
        let clicked_visual_line =
            ((relative_y + chat_input.scroll_pixel_offset) / line_height) as usize;

        // Check if click is within valid visual lines (from line_positions)
        if clicked_visual_line < line_positions.len() {
            let line_glyph_positions = &line_positions[clicked_visual_line];

            // Find which character was clicked using exact glyph positions
            let clicked_document_byte_offset = if line_glyph_positions.is_empty() {
                0
            } else {
                // Find the glyph that contains the click position
                let mut found_offset = None;
                for (x_start, x_end, byte_offset) in line_glyph_positions.iter() {
                    if relative_x < *x_start {
                        // Click is before this glyph
                        found_offset = Some(*byte_offset);
                        break;
                    } else if relative_x >= *x_start && relative_x <= *x_end {
                        // Click is within this glyph - decide if it's closer to start or end
                        let mid = (*x_start + *x_end) / 2.0;
                        if relative_x < mid {
                            found_offset = Some(*byte_offset);
                        } else {
                            // Try to get next glyph's offset
                            let next_idx = line_glyph_positions
                                .iter()
                                .position(|(s, _, _)| *s == *x_start)
                                .and_then(|idx| line_glyph_positions.get(idx + 1))
                                .map(|(_, _, next_offset)| *next_offset);
                            found_offset = next_idx.or(Some(*byte_offset + 1));
                        }
                        break;
                    }
                }

                // If click is past all glyphs, position at end of visual line
                if found_offset.is_none() {
                    if let Some((_, _, last_offset)) = line_glyph_positions.last() {
                        found_offset = Some(*last_offset + 1);
                    }
                }

                found_offset.unwrap_or(0)
            };

            // Now map the document byte offset to logical line and column
            let mut current_byte = 0;
            let mut found_logical_position = false;

            for (logical_line_idx, line_text) in chat_input.lines.iter().enumerate() {
                let line_start = current_byte;
                let line_end = current_byte + line_text.len();

                if clicked_document_byte_offset >= line_start
                    && clicked_document_byte_offset <= line_end
                {
                    // Found the logical line containing this byte offset
                    let line_relative_byte = clicked_document_byte_offset - line_start;

                    // Convert byte offset to character index
                    let char_index = line_text
                        .char_indices()
                        .take_while(|(byte_idx, _)| *byte_idx < line_relative_byte)
                        .count();

                    // Update cursor position
                    chat_input.cursor_line = logical_line_idx;
                    chat_input.cursor_col = char_index;
                    found_logical_position = true;

                    log::debug!(
                        "Mapped click to logical line {}, col {}, doc_byte={}",
                        logical_line_idx,
                        char_index,
                        clicked_document_byte_offset
                    );
                    break;
                }

                current_byte = line_end + 1; // +1 for newline
            }

            // If we couldn't map to a logical position, fallback to end of last line
            if !found_logical_position {
                chat_input.cursor_line = chat_input.lines.len().saturating_sub(1);
                chat_input.cursor_col = chat_input.lines[chat_input.cursor_line].len();
            }

            // Handle selection
            if shift_held {
                // Extend selection
                if chat_input.selection_start.is_none() {
                    chat_input.selection_start =
                        Some((chat_input.cursor_line, chat_input.cursor_col));
                }
            } else if !is_drag {
                // Clear selection on non-drag click
                chat_input.selection_start = None;
            }

            chat_input.focused = true;
            return true;
        } else if clicked_visual_line < chat_input.lines.len() {
            // Fallback for when we don't have glyph positions but click is within logical lines
            chat_input.cursor_line = clicked_visual_line;
            chat_input.cursor_col = ((relative_x / ESTIMATED_CHAR_WIDTH) as usize)
                .min(chat_input.lines[clicked_visual_line].len());

            // Handle selection
            if shift_held {
                if chat_input.selection_start.is_none() {
                    chat_input.selection_start =
                        Some((chat_input.cursor_line, chat_input.cursor_col));
                }
            } else if !is_drag {
                chat_input.selection_start = None;
            }

            chat_input.focused = true;
            return true;
        }

        false
    }

    /// Calculate precise cursor position for rendering using glyph positions
    pub fn get_cursor_position(
        chat_input: &MultilineTextInput,
        font: &Rc<LoadedFont>,
    ) -> Option<(f32, f32)> {
        if !chat_input.focused {
            return None;
        }

        let metrics = font.metrics();
        let line_height = metrics.cell_height.get() as f32;
        let line_height_with_spacing = line_height * LINE_HEIGHT_MULTIPLIER;

        // First, calculate the document byte offset for the cursor position
        let mut cursor_document_byte_offset = 0;

        // Add bytes from all lines before the cursor line
        for (line_idx, line) in chat_input.lines.iter().enumerate() {
            if line_idx < chat_input.cursor_line {
                cursor_document_byte_offset += line.len() + 1; // +1 for newline
            } else if line_idx == chat_input.cursor_line {
                // Add bytes up to cursor column in current line
                let byte_in_line = line
                    .char_indices()
                    .nth(chat_input.cursor_col)
                    .map(|(idx, _)| idx)
                    .unwrap_or(line.len());
                cursor_document_byte_offset += byte_in_line;
                break;
            }
        }

        log::debug!(
            "Cursor at logical line {}, col {} = document byte offset {}, glyph_positions available: {}, visual_lines={}",
            chat_input.cursor_line,
            chat_input.cursor_col,
            cursor_document_byte_offset,
            !chat_input.exact_glyph_positions.is_empty(),
            chat_input.exact_glyph_positions.len()
        );

        // Now find which visual line contains this byte offset
        let mut visual_line_idx = None;
        let mut cursor_x = 0.0;

        for (vis_line_idx, visual_line_positions) in
            chat_input.exact_glyph_positions.iter().enumerate()
        {
            if visual_line_positions.is_empty() {
                continue;
            }

            // Check if this visual line contains our byte offset
            let first_byte = visual_line_positions
                .first()
                .map(|(_, _, b)| *b)
                .unwrap_or(0);
            let last_byte = visual_line_positions
                .last()
                .map(|(_, _, b)| *b)
                .unwrap_or(0);

            log::debug!(
                "Visual line {}: byte range {} - {}, cursor looking for {}, contains={}",
                vis_line_idx,
                first_byte,
                last_byte,
                cursor_document_byte_offset,
                cursor_document_byte_offset >= first_byte
                    && cursor_document_byte_offset <= last_byte + 1
            );

            if cursor_document_byte_offset >= first_byte
                && cursor_document_byte_offset <= last_byte + 1
            {
                // Found the visual line containing our cursor
                visual_line_idx = Some(vis_line_idx);

                // Find X position within this visual line
                if cursor_document_byte_offset == first_byte {
                    // Cursor at start of visual line
                    cursor_x = 0.0;
                } else {
                    // Find position within line
                    let mut found = false;
                    let mut prev_end = 0.0;

                    for (x_start, x_end, byte_offset) in visual_line_positions.iter() {
                        if *byte_offset == cursor_document_byte_offset {
                            // Cursor is exactly at this glyph's position - put it at the start
                            cursor_x = *x_start;
                            found = true;
                            break;
                        } else if *byte_offset > cursor_document_byte_offset {
                            // We've passed the cursor position - use the end of the previous glyph
                            cursor_x = prev_end;
                            found = true;
                            break;
                        }
                        prev_end = *x_end;
                    }

                    // If we didn't find a position (cursor at end of line), use the last glyph's end
                    if !found && !visual_line_positions.is_empty() {
                        cursor_x = visual_line_positions.last().unwrap().1;
                    }
                }

                break;
            }
        }

        // Calculate Y position based on visual line index
        let y = if let Some(vis_line_idx) = visual_line_idx {
            vis_line_idx as f32 * line_height_with_spacing - chat_input.scroll_pixel_offset
        } else {
            // Fallback: use logical line if we couldn't find visual line
            log::warn!(
                "Could not find visual line for cursor at byte offset {}, positions_count={}, visual_lines={}",
                cursor_document_byte_offset,
                chat_input.exact_glyph_positions.len(),
                chat_input.visual_line_count
            );
            chat_input.cursor_line as f32 * line_height_with_spacing
                - chat_input.scroll_pixel_offset
        };

        Some((cursor_x, y))
    }

    /// Clear the chat input
    pub fn clear(chat_input: &mut MultilineTextInput) {
        // Use the MultilineTextInput's clear method which properly resets all state
        chat_input.clear();
        // Also reset scroll state
        chat_input.scroll_pixel_offset = 0.0;
        chat_input.user_has_scrolled = false;
    }

    /// Get the current text content
    pub fn get_text(chat_input: &MultilineTextInput) -> String {
        // Use the MultilineTextInput's get_text method
        chat_input.get_text()
    }

    /// Check if input is empty
    pub fn is_empty(chat_input: &MultilineTextInput) -> bool {
        chat_input.lines.len() == 1 && chat_input.lines[0].is_empty()
    }
}
