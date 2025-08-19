//! Position extraction for suggestion cards
//!
//! This module provides position extraction capabilities for suggestion cards,
//! using the same hierarchical PositionTree approach as activity log items.
//! This enables proper multi-line text selection in suggestions.

use crate::sidebar::position_cache::{
    ElementType, PositionTree, PositionTreeBuilder, TextPosition, TextStyle,
};
use crate::sidebar::sidebar_constants::*;
use crate::termwindow::box_model::{
    ComputedElement, ComputedElementContent, ElementCell, WrappedLine,
};
use crate::termwindow::UIItemType;
use euclid::{Point2D, Rect, Size2D};
use std::collections::{HashMap, HashSet};
use window::PixelUnit;

/// Extract position data from a suggestion card's computed element
/// 
/// This function uses the same approach as activity log items to build
/// a hierarchical position tree that enables accurate multi-line selection.
pub fn extract_suggestion_positions(
    computed: &ComputedElement,
    fonts: &crate::sidebar::SidebarFonts,
) -> Option<(PositionTree, String, HashSet<usize>)> {
    // Check if this is actually a suggestion text element
    if let Some(ref item_type) = computed.item_type {
        if !matches!(item_type, UIItemType::SuggestionText { .. }) {
            return None;
        }
    } else {
        return None;
    }
    
    let mut builder = PositionTreeBuilder::new();
    
    // Extract line height from the element
    let line_height = extract_line_height(computed);
    
    // Calculate content offset within the element
    let content_offset = Point2D::new(
        computed.content_rect.min_x() - computed.bounds.min_x(),
        computed.content_rect.min_y() - computed.bounds.min_y(),
    );
    
    log::debug!(
        "Extracting suggestion positions with bounds: {:?}, content_rect: {:?}, content_offset: {:?}",
        computed.bounds,
        computed.content_rect,
        content_offset
    );
    
    // Create root element for the suggestion
    builder.start_element_with_offset(
        ElementType::Paragraph {
            line_height,
            margin: 0.0,
        },
        Rect::new(
            Point2D::new(0.0, 0.0),
            Size2D::new(computed.bounds.width(), computed.bounds.height()),
        ),
        euclid::Vector2D::new(content_offset.x, content_offset.y),
    );
    
    let mut cumulative_byte_offset = 0;
    let mut rendered_text = String::new();
    let mut wrap_newlines = HashSet::new();
    let mut global_line_index = 0usize;
    
    // Extract positions recursively
    extract_positions_recursively(
        computed,
        &mut builder,
        content_offset,
        fonts,
        &mut cumulative_byte_offset,
        &mut rendered_text,
        &mut wrap_newlines,
        &mut global_line_index,
    );
    
    let result = builder.build();
    
    
    result.map(|tree| (tree, rendered_text, wrap_newlines))
}

/// Extract line height from computed element
fn extract_line_height(computed: &ComputedElement) -> f32 {
    match &computed.content {
        ComputedElementContent::MultilineText { line_height, .. } => *line_height,
        ComputedElementContent::Children(children) => {
            // Search children for line height
            for child in children {
                if let ComputedElementContent::MultilineText { line_height, .. } = &child.content {
                    return *line_height;
                }
            }
            // Default to standard paragraph line height
            PARAGRAPH_LINE_HEIGHT
        }
        _ => PARAGRAPH_LINE_HEIGHT,
    }
}

/// Recursively extract positions from the element tree
fn extract_positions_recursively(
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &crate::sidebar::SidebarFonts,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    match &computed.content {
        ComputedElementContent::Text(cells) => {
            // Simple text element with a single line of cells
            // Treat it like a single-line multiline text
            extract_multiline_positions_with_wrapped_lines(
                &[cells.clone()], // Wrap in a vec to match multiline format
                extract_line_height(computed),
                &[], // No line positions for single line
                &None, // No WrappedLine info for simple text
                computed,
                builder,
                offset,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
        ComputedElementContent::MultilineText { lines, line_height, line_positions, line_info, .. } => {
            // Wrapped text with multiple lines - use the proper extraction with WrappedLine info
            extract_multiline_positions_with_wrapped_lines(
                lines,
                *line_height,
                line_positions,
                line_info,
                computed,
                builder,
                offset,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
        ComputedElementContent::Children(children) => {
            // Process child elements
            for child in children {
                let child_offset = Point2D::new(
                    offset.x + child.bounds.min_x() - computed.bounds.min_x(),
                    offset.y + child.bounds.min_y() - computed.bounds.min_y(),
                );
                
                extract_positions_recursively(
                    child,
                    builder,
                    child_offset,
                    fonts,
                    cumulative_byte_offset,
                    rendered_text,
                    wrap_newlines,
                    global_line_index,
                );
            }
        }
        _ => {
            // Other content types don't have text positions
        }
    }
}

/// Extract positions from multiline wrapped text with proper WrappedLine support
/// This is the proper implementation that matches activity log behavior
fn extract_multiline_positions_with_wrapped_lines(
    lines: &[Vec<ElementCell>],
    line_height: f32,
    line_positions: &[f32],
    line_info: &Option<Vec<crate::termwindow::box_model::WrappedLine>>,
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    // The offset already includes padding from content_offset calculation above
    // Don't add padding again - just use the offset directly
    let text_offset: Point2D<f32, PixelUnit> = offset;
    
    log::debug!(
        "Extracting suggestion multiline text: {} lines, has line_info={}, text_offset=({:.1}, {:.1})",
        lines.len(),
        line_info.is_some(),
        text_offset.x,
        text_offset.y
    );
    
    
    // Process each line with proper WrappedLine information
    for (local_line_index, cells) in lines.iter().enumerate() {
        // Use line_positions if available, otherwise calculate based on line_height
        let y = line_positions
            .get(local_line_index)
            .copied()
            .unwrap_or_else(|| local_line_index as f32 * line_height);
            
        if let Some(wrapped_lines) = line_info {
            if let Some(wrapped_line) = wrapped_lines.get(local_line_index) {
                // Track actual position in rendered_text
                let line_start_in_rendered = rendered_text.len();
                
                log::debug!(
                    "  Suggestion line {} (global {}): wrapped.byte_offset={}, rendered_pos={}, text='{}'",
                    local_line_index,
                    global_line_index,
                    wrapped_line.byte_offset,
                    line_start_in_rendered,
                    wrapped_line.shaped_text.chars().take(30).collect::<String>()
                );
                
                // Collect the shaped_text - this is the actual rendered text!
                rendered_text.push_str(&wrapped_line.shaped_text);
                
                // Extract positions using the WrappedLine information
                extract_positions_from_cells_with_wrapped_line(
                    cells,
                    builder,
                    Point2D::new(text_offset.x, text_offset.y + y),
                    *global_line_index,
                    wrapped_line,
                    line_start_in_rendered,
                );
                
                // Increment global line index after processing this line
                *global_line_index += 1;
                
                // Add newline between lines (but not after the last line)
                if local_line_index < lines.len() - 1 {
                    let newline_pos = rendered_text.len();
                    wrap_newlines.insert(newline_pos);
                    rendered_text.push('\n');
                }
            } else {
                // Fallback if no WrappedLine for this line index
                log::warn!(
                    "Suggestion line {} missing WrappedLine info, using fallback",
                    local_line_index
                );
                extract_positions_from_cells_fallback(
                    cells,
                    builder,
                    Point2D::new(text_offset.x, text_offset.y + y),
                    *global_line_index,
                    *cumulative_byte_offset,
                );
                *global_line_index += 1;
            }
        } else {
            // No line_info available - use fallback
            log::warn!("No line_info available for suggestion, using fallback");
            extract_positions_from_cells_fallback(
                cells,
                builder,
                Point2D::new(text_offset.x, text_offset.y + y),
                *global_line_index,
                *cumulative_byte_offset,
            );
            *global_line_index += 1;
        }
    }
}

/// Extract positions using WrappedLine information - the proper way
fn extract_positions_from_cells_with_wrapped_line(
    cells: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    line_index: usize,
    wrapped_line: &crate::termwindow::box_model::WrappedLine,
    byte_offset_in_rendered: usize,
) {
    let mut x_pos = offset.x;
    
    for cell in cells {
        match cell {
            ElementCell::GlyphWithCluster { glyph, cluster } => {
                // Use the byte offset from the WrappedLine plus the cluster offset
                // byte_offset_in_rendered is the position of this line in the rendered text
                let position = TextPosition {
                    byte_offset: byte_offset_in_rendered + *cluster as usize,
                    x_start: x_pos,
                    x_end: x_pos + glyph.x_advance.get() as f32,
                    y: offset.y,
                    line_index,
                };
                
                builder.add_text_position(position);
                x_pos += glyph.x_advance.get() as f32;
            }
            ElementCell::Glyph(glyph) => {
                // Regular glyph without cluster info - just advance
                x_pos += glyph.x_advance.get() as f32;
            }
            _ => {
                // Other cell types don't contribute to positions
            }
        }
    }
}

/// Fallback position extraction when WrappedLine info is not available
fn extract_positions_from_cells_fallback(
    cells: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    line_index: usize,
    byte_offset: usize,
) {
    let mut x_pos = offset.x;
    
    for cell in cells {
        match cell {
            ElementCell::GlyphWithCluster { glyph, cluster } => {
                let position = TextPosition {
                    byte_offset: byte_offset + *cluster as usize,
                    x_start: x_pos,
                    x_end: x_pos + glyph.x_advance.get() as f32,
                    y: offset.y,
                    line_index,
                };
                
                builder.add_text_position(position);
                x_pos += glyph.x_advance.get() as f32;
            }
            ElementCell::Glyph(glyph) => {
                x_pos += glyph.x_advance.get() as f32;
            }
            _ => {}
        }
    }
}