// Refactored version of activity_log_positions.rs with the mega-function broken down
// This file demonstrates the proposed refactoring approach

use crate::sidebar::{SidebarFonts, position_cache::PositionTreeBuilder};
use crate::termwindow::box_model::{ComputedElement, ComputedElementContent, ElementCell};
use euclid::Point2D;
use window::PixelUnit;
use std::collections::HashSet;

/// Main dispatcher function - delegates to type-specific handlers
/// Target: ~50 lines (down from 502)
pub fn extract_positions_recursively_with_text_and_wraps(
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &SidebarFonts,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    // Calculate element padding once for all handlers
    let element_padding = calculate_element_padding(computed);
    let content_offset = Point2D::new(
        offset.x + element_padding.x,
        offset.y + element_padding.y,
    );

    match &computed.content {
        ComputedElementContent::Text(cells) => {
            extract_text_element_positions(
                cells,
                computed,
                builder,
                content_offset,
                cumulative_byte_offset,
                rendered_text,
            );
        }
        ComputedElementContent::MultilineText { lines, line_height, line_positions, line_info, .. } => {
            extract_multiline_element_positions(
                lines,
                line_height,
                line_positions,
                line_info,
                computed,
                builder,
                content_offset,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
        ComputedElementContent::Children(children) => {
            extract_children_element_positions(
                children,
                computed,
                builder,
                offset,
                fonts,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
        _ => {
            // Skip other content types (Empty, FrameBuffer, etc.)
        }
    }
}

/// Calculate padding for an element
fn calculate_element_padding(computed: &ComputedElement) -> Point2D<f32, PixelUnit> {
    Point2D::new(
        computed.content_rect.min_x() - computed.bounds.min_x(),
        computed.content_rect.min_y() - computed.bounds.min_y(),
    )
}

/// Extract positions from single-line text elements
/// Target: ~40 lines
fn extract_text_element_positions(
    cells: &[ElementCell],
    _computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
) {
    // Extract positions at the actual text rendering position
    extract_positions_from_cells_with_byte_offset(
        cells,
        builder,
        offset,
        *cumulative_byte_offset,
    );

    // Update cumulative offset and rendered text
    let text = extract_text_from_cells(cells);
    rendered_text.push_str(&text);
    *cumulative_byte_offset += text.len();
}

/// Extract positions from multiline text elements
/// Target: ~120 lines
fn extract_multiline_element_positions(
    lines: &[Vec<ElementCell>],
    line_height: &f32,
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
    log::debug!(
        "Found MultilineText with {} lines at offset ({:.1},{:.1})",
        lines.len(),
        offset.x,
        offset.y,
    );

    // Validate and warn about missing cluster data
    validate_multiline_clusters(lines);

    // Calculate paragraph byte length
    let paragraph_byte_length = calculate_paragraph_byte_length(lines, line_info);

    log::debug!(
        "  📊 MultilineText: {} bytes, starts at byte offset {}",
        paragraph_byte_length,
        cumulative_byte_offset
    );

    // Process each line
    for (local_line_index, line) in lines.iter().enumerate() {
        let y = line_positions
            .get(local_line_index)
            .copied()
            .unwrap_or_else(|| local_line_index as f32 * *line_height);

        process_multiline_text_line(
            line,
            local_line_index,
            y,
            line_info,
            builder,
            offset,
            rendered_text,
            wrap_newlines,
            global_line_index,
            cumulative_byte_offset,
        );
    }
}

/// Validate that multiline text has proper cluster data
fn validate_multiline_clusters(lines: &[Vec<ElementCell>]) {
    let mut has_regular_glyphs = false;
    
    for line in lines.iter() {
        for cell in line.iter() {
            if matches!(cell, ElementCell::Glyph(_)) {
                has_regular_glyphs = true;
            }
        }
    }
    
    if has_regular_glyphs {
        log::warn!("⚠️ Found Glyph without cluster in sidebar text! This should not happen.");
    }
}

/// Calculate total byte length of a paragraph
fn calculate_paragraph_byte_length(
    lines: &[Vec<ElementCell>],
    line_info: &Option<Vec<crate::termwindow::box_model::WrappedLine>>,
) -> usize {
    if let Some(wrapped_lines) = line_info {
        // Get the byte length from the last wrapped line
        if let Some(last_line) = wrapped_lines.last() {
            log::debug!("  ✓ Using WrappedLine info: paragraph has {} bytes", last_line.byte_end);
            return last_line.byte_end;
        }
    }
    
    // Fallback to estimation from cells (unreliable for multi-byte chars)
    log::error!("  ❌ ERROR: WrappedLine info missing for MultilineText! Falling back to estimation.");
    lines.iter().map(|line| calculate_text_byte_length_from_cells(line)).sum()
}

/// Process a single line of multiline text
/// Target: ~80 lines
fn process_multiline_text_line(
    line: &[ElementCell],
    local_line_index: usize,
    y: f32,
    line_info: &Option<Vec<crate::termwindow::box_model::WrappedLine>>,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
    cumulative_byte_offset: &mut usize,
) {
    let line_offset = Point2D::new(offset.x, offset.y + y);
    
    if let Some(wrapped_lines) = line_info {
        if let Some(wrapped_line) = wrapped_lines.get(local_line_index) {
            // Track actual position in rendered_text
            let line_start_in_rendered = rendered_text.len();
            
            log::debug!(
                "    Line {} (global {}): rendered_pos={}, text='{}'",
                local_line_index,
                global_line_index,
                line_start_in_rendered,
                wrapped_line.shaped_text.chars().take(20).collect::<String>()
            );

            // Add the shaped text to rendered
            rendered_text.push_str(&wrapped_line.shaped_text);
            
            // Extract positions using the correct byte offset
            extract_positions_from_cells_with_line_info(
                line,
                builder,
                line_offset,
                line_start_in_rendered,
                *global_line_index,
            );

            // Handle line endings
            handle_line_ending(
                local_line_index,
                wrapped_lines.len(),
                wrapped_line,
                rendered_text,
                wrap_newlines,
            );
        }
    } else {
        // Fallback for lines without wrapped info
        extract_positions_from_cells_with_byte_offset(
            line,
            builder,
            line_offset,
            rendered_text.len(),
        );
        
        let line_text = extract_text_from_cells(line);
        rendered_text.push_str(&line_text);
        
        if local_line_index < (lines.len() - 1) {
            rendered_text.push('\n');
        }
    }
    
    *global_line_index += 1;
}

/// Handle line endings and wrap tracking
fn handle_line_ending(
    local_line_index: usize,
    total_lines: usize,
    wrapped_line: &crate::termwindow::box_model::WrappedLine,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
) {
    // Add newline if not the last line
    if local_line_index < total_lines - 1 {
        let newline_byte_offset = rendered_text.len();
        rendered_text.push('\n');
        
        // Track if this is an artificial wrap newline
        if wrapped_line.wrapped_at_soft_break {
            wrap_newlines.insert(newline_byte_offset);
            log::debug!(
                "      Tracked artificial newline at byte {} (soft wrap)",
                newline_byte_offset
            );
        }
    }
}

/// Extract positions from children elements
/// Target: ~100 lines
fn extract_children_element_positions(
    children: &[ComputedElement],
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &SidebarFonts,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    // Check for semantic types that need special handling
    if let Some(semantic_type) = &computed.semantic_type {
        handle_semantic_element(
            semantic_type,
            children,
            builder,
            offset,
            fonts,
            cumulative_byte_offset,
            rendered_text,
            wrap_newlines,
            global_line_index,
        );
    } else {
        // Regular children processing
        for child in children {
            let child_offset = Point2D::new(
                offset.x + child.bounds.min_x(),
                offset.y + child.bounds.min_y(),
            );
            
            extract_positions_recursively_with_text_and_wraps(
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
}

/// Handle semantic elements (headings, code blocks, etc.)
/// Target: ~80 lines
fn handle_semantic_element(
    semantic_type: &crate::termwindow::box_model::SemanticType,
    children: &[ComputedElement],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &SidebarFonts,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    use crate::termwindow::box_model::SemanticType;
    
    match semantic_type {
        SemanticType::Heading(level) => {
            log::debug!("Processing Heading level {} at offset ({:.1}, {:.1})", level, offset.x, offset.y);
            // Process heading children normally
            process_children_recursively(
                children,
                builder,
                offset,
                fonts,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
        SemanticType::CodeBlock { .. } => {
            log::debug!("Processing CodeBlock at offset ({:.1}, {:.1})", offset.x, offset.y);
            let adjusted_offset = apply_code_block_offset_adjustment(offset);
            process_children_recursively(
                children,
                builder,
                adjusted_offset,
                fonts,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
        SemanticType::ListItem { .. } => {
            log::debug!("Processing ListItem at offset ({:.1}, {:.1})", offset.x, offset.y);
            process_children_recursively(
                children,
                builder,
                offset,
                fonts,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
        _ => {
            // Other semantic types processed normally
            process_children_recursively(
                children,
                builder,
                offset,
                fonts,
                cumulative_byte_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
            );
        }
    }
}

/// Process children elements recursively
fn process_children_recursively(
    children: &[ComputedElement],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &SidebarFonts,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    for child in children {
        let child_offset = Point2D::new(
            offset.x + child.bounds.min_x(),
            offset.y + child.bounds.min_y(),
        );
        
        extract_positions_recursively_with_text_and_wraps(
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

// Constants from sidebar_constants.rs
const CODE_BLOCK_PADDING: f32 = 12.0;
const CODE_BLOCK_TOP_MARGIN: f32 = 8.0;

/// Apply code block offset adjustments
fn apply_code_block_offset_adjustment(offset: Point2D<f32, PixelUnit>) -> Point2D<f32, PixelUnit> {
    Point2D::new(
        offset.x + CODE_BLOCK_PADDING,
        offset.y + CODE_BLOCK_PADDING + CODE_BLOCK_TOP_MARGIN,
    )
}

// Helper functions that would need to be imported or implemented
fn extract_positions_from_cells_with_byte_offset(
    _cells: &[ElementCell],
    _builder: &mut PositionTreeBuilder,
    _offset: Point2D<f32, PixelUnit>,
    _byte_offset: usize,
) {
    // Implementation would be copied from original file
}

fn extract_positions_from_cells_with_line_info(
    _cells: &[ElementCell],
    _builder: &mut PositionTreeBuilder,
    _offset: Point2D<f32, PixelUnit>,
    _byte_offset: usize,
    _line_index: usize,
) {
    // Implementation would be copied from original file
}

fn calculate_text_byte_length_from_cells(_cells: &[ElementCell]) -> usize {
    // Implementation would be copied from original file
    0
}

fn extract_text_from_cells(_cells: &[ElementCell]) -> String {
    // Implementation would be copied from original file
    String::new()
}