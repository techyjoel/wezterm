// Helper functions extracted from the mega-function in activity_log_positions.rs
// These can be integrated back into the main file after testing

use crate::sidebar::position_cache::PositionTreeBuilder;
use crate::termwindow::box_model::{ComputedElement, ElementCell, WrappedLine};
use euclid::Point2D;
use window::PixelUnit;
use std::collections::HashSet;

/// Process a single line in multiline text
pub fn process_multiline_text_line(
    line: &[ElementCell],
    local_line_index: usize,
    y: f32,
    line_info: &Option<Vec<WrappedLine>>,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
    cumulative_byte_offset: &mut usize,
    total_lines: usize,
) {
    let line_offset = Point2D::new(offset.x, offset.y + y);
    
    if let Some(wrapped_lines) = line_info {
        if let Some(wrapped_line) = wrapped_lines.get(local_line_index) {
            process_wrapped_line(
                line,
                local_line_index,
                wrapped_line,
                builder,
                line_offset,
                rendered_text,
                wrap_newlines,
                global_line_index,
                total_lines,
            );
        } else {
            // Fallback case - shouldn't happen with proper global offsets
            log::warn!(
                "    Line {} (global {}) missing WrappedLine info - using cumulative offset {}",
                local_line_index,
                global_line_index,
                cumulative_byte_offset
            );
            process_unwrapped_line(
                line,
                builder,
                line_offset,
                global_line_index,
                cumulative_byte_offset,
            );
        }
    } else {
        // Fallback case - shouldn't happen with proper global offsets
        log::warn!(
            "    No line_info available - using cumulative offset {}",
            cumulative_byte_offset
        );
        process_unwrapped_line(
            line,
            builder,
            line_offset,
            global_line_index,
            cumulative_byte_offset,
        );
    }
}

/// Process a wrapped line with position tracking
fn process_wrapped_line(
    line: &[ElementCell],
    local_line_index: usize,
    wrapped_line: &WrappedLine,
    builder: &mut PositionTreeBuilder,
    line_offset: Point2D<f32, PixelUnit>,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
    total_lines: usize,
) {
    // Track actual position in rendered_text
    let line_start_in_rendered = rendered_text.len();
    
    log::debug!(
        "    Line {} (global {}): wrapped.byte_offset={} (original text), rendered_pos={} (actual), text='{}'",
        local_line_index,
        global_line_index,
        wrapped_line.byte_offset,
        line_start_in_rendered,
        wrapped_line.shaped_text.chars().take(20).collect::<String>()
    );

    // Collect the shaped_text - this is the actual rendered text!
    rendered_text.push_str(&wrapped_line.shaped_text);
    
    // Extract positions using the correct byte offset
    extract_positions_from_cells_with_wrapped_line_and_offset(
        line,
        builder,
        line_offset,
        *global_line_index,
        wrapped_line,
        line_start_in_rendered,
    );
    
    // Increment global line index after processing this line
    *global_line_index += 1;
    
    // Add newline between lines (but not after the last line)
    if local_line_index < total_lines - 1 {
        // Track this as an artificial newline from text wrapping
        let newline_pos = rendered_text.len();
        wrap_newlines.insert(newline_pos);
        rendered_text.push('\n');
    }
}

/// Process an unwrapped line (fallback case)
fn process_unwrapped_line(
    line: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    line_offset: Point2D<f32, PixelUnit>,
    global_line_index: &mut usize,
    cumulative_byte_offset: &usize,
) {
    extract_positions_from_cells_with_line_and_offset(
        line,
        builder,
        line_offset,
        *global_line_index,
        *cumulative_byte_offset,
    );
    *global_line_index += 1;
}

/// Handle semantic elements with special processing
pub fn handle_semantic_element(
    semantic_type: &crate::termwindow::box_model::SemanticType,
    children: &[ComputedElement],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &crate::sidebar::SidebarFonts,
    cumulative_byte_offset: &mut usize,
    rendered_text: &mut String,
    wrap_newlines: &mut HashSet<usize>,
    global_line_index: &mut usize,
) {
    use crate::termwindow::box_model::SemanticType;
    
    match semantic_type {
        SemanticType::Heading(level) => {
            log::debug!("Processing Heading level {} at offset ({:.1}, {:.1})", level, offset.x, offset.y);
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
    fonts: &crate::sidebar::SidebarFonts,
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
        
        // This would call back to the main function
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

// These functions would need to be imported from the main file
fn extract_positions_from_cells_with_wrapped_line_and_offset(
    _cells: &[ElementCell],
    _builder: &mut PositionTreeBuilder,
    _offset: Point2D<f32, PixelUnit>,
    _line_index: usize,
    _wrapped_line: &WrappedLine,
    _byte_offset: usize,
) {
    // Stub - implementation in main file
}

fn extract_positions_from_cells_with_line_and_offset(
    _cells: &[ElementCell],
    _builder: &mut PositionTreeBuilder,
    _offset: Point2D<f32, PixelUnit>,
    _line_index: usize,
    _byte_offset: usize,
) {
    // Stub - implementation in main file
}

fn extract_positions_recursively_with_text_and_wraps(
    _computed: &ComputedElement,
    _builder: &mut PositionTreeBuilder,
    _offset: Point2D<f32, PixelUnit>,
    _fonts: &crate::sidebar::SidebarFonts,
    _cumulative_byte_offset: &mut usize,
    _rendered_text: &mut String,
    _wrap_newlines: &mut HashSet<usize>,
    _global_line_index: &mut usize,
) {
    // Stub - would call back to main function
}