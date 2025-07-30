//! Activity log position extraction during rendering
//!
//! This module handles extracting glyph positions during the rendering
//! of activity log items to enable pixel-perfect text selection.

use crate::color::LinearRgba;
use crate::sidebar::position_cache::{
    ElementType, ItemPosition, ItemPositionData, PositionTree, PositionTreeBuilder, TextPosition,
    TextStyle,
};
use crate::termwindow::box_model::{ComputedElement, ComputedElementContent, ElementCell};
use crate::termwindow::UIItemType;
use euclid::{Point2D, Rect, Size2D};
use std::collections::HashMap;
use std::rc::Rc;
use wezterm_font::LoadedFont;
use window::PixelUnit;

/// Extract position data from a computed element representing an activity log item
pub fn extract_activity_item_positions(
    computed: &ComputedElement,
    ui_item_type: &UIItemType,
    fonts: &crate::sidebar::SidebarFonts,
) -> Option<PositionTree> {
    match ui_item_type {
        UIItemType::ActivityItemText { index, .. } => {
            let mut builder = PositionTreeBuilder::new();
            
            // Start with the root element for the activity item
            builder.start_element(
                ElementType::Paragraph {
                    line_height: 20.0, // Default line height
                    margin: 0.0,
                },
                Rect::new(
                    Point2D::new(0.0, 0.0),
                    Size2D::new(computed.bounds.width(), computed.bounds.height()),
                ),
            );
            
            // Extract positions from the computed content
            extract_positions_from_content(
                &computed.content,
                &mut builder,
                Point2D::new(0.0, 0.0),
                fonts,
            );
            
            builder.end_element()
        }
        _ => None,
    }
}

/// Recursively extract positions from computed element content
fn extract_positions_from_content(
    content: &ComputedElementContent,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &crate::sidebar::SidebarFonts,
) {
    match content {
        ComputedElementContent::Text(cells) => {
            extract_positions_from_cells(cells, builder, offset);
        }
        ComputedElementContent::MultilineText { lines, line_height, .. } => {
            let mut y = offset.y;
            for (line_index, line) in lines.iter().enumerate() {
                extract_positions_from_cells_with_line(line, builder, Point2D::new(offset.x, y), line_index);
                y += *line_height as f32;
            }
        }
        ComputedElementContent::Children(children) => {
            for child in children {
                let child_offset = offset + child.bounds.origin.to_vector();
                
                // Check if this is a special markdown element
                if let Some(element_type) = determine_element_type(child, fonts) {
                    builder.start_element(element_type, child.bounds);
                    extract_positions_from_content(&child.content, builder, Point2D::new(0.0, 0.0), fonts);
                    builder.end_element();
                } else {
                    extract_positions_from_content(&child.content, builder, child_offset, fonts);
                }
            }
        }
        _ => {} // Skip other content types
    }
}

/// Extract positions from a line of cells
fn extract_positions_from_cells(
    cells: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
) {
    extract_positions_from_cells_with_line(cells, builder, offset, 0);
}

/// Extract positions from a line of cells with line index
fn extract_positions_from_cells_with_line(
    cells: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    line_index: usize,
) {
    let mut x_pos = offset.x;
    
    for cell in cells {
        match cell {
            ElementCell::GlyphWithCluster {
                glyph,
                cluster,
            } => {
                let x_start = x_pos;
                let x_end = x_pos + glyph.x_advance.get() as f32;
                
                builder.add_text_position(TextPosition {
                    byte_offset: *cluster as usize, // Cluster is the byte offset in the shaped text
                    x_start,
                    x_end,
                    y: offset.y,
                    line_index,
                });
                
                x_pos = x_end;
            }
            ElementCell::Glyph(cached_glyph) => {
                // Regular glyphs without cluster info - skip position tracking
                x_pos += cached_glyph.x_advance.get() as f32;
            }
            ElementCell::Sprite(sprite) => {
                x_pos += sprite.coords.size.width as f32;
            }
        }
    }
}

/// Determine the element type based on computed element properties
fn determine_element_type(
    computed: &ComputedElement,
    fonts: &crate::sidebar::SidebarFonts,
) -> Option<ElementType> {
    // Basic detection based on element properties
    // This is a simplified version - full markdown detection would require
    // more context from the rendering pipeline
    
    // Check for code blocks by looking for monospace font
    if let Some(ref font) = computed.font {
        if font.font_id().name.contains("Mono") || font.font_id().name.contains("Code") {
            return Some(ElementType::CodeBlock {
                line_height: 20.0,
                padding: 8.0,
                bg_color: crate::color::LinearRgba::with_components(0.1, 0.1, 0.12, 1.0),
            });
        }
    }
    
    // For now, treat everything else as regular paragraphs
    // Full implementation would detect headings, lists, etc.
    None
}

/// Extract positions for markdown-specific elements
pub fn extract_markdown_positions(
    computed: &ComputedElement,
    markdown_type: MarkdownElementType,
    fonts: &crate::sidebar::SidebarFonts,
) -> Option<PositionTree> {
    let mut builder = PositionTreeBuilder::new();
    
    let element_type = match markdown_type {
        MarkdownElementType::Heading { level } => ElementType::Heading {
            level,
            font_size: heading_font_size(level),
            margin: 12.0,
        },
        MarkdownElementType::CodeBlock => ElementType::CodeBlock {
            line_height: 20.0, // Default code block line height
            padding: 8.0,
            bg_color: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0),
        },
        MarkdownElementType::ListItem { depth, is_ordered } => ElementType::ListItem {
            indent: depth as f32 * 20.0,
            marker_width: 20.0,
            depth,
            is_ordered,
        },
        MarkdownElementType::InlineCode => ElementType::InlineCode {
            bg_color: LinearRgba::with_components(0.15, 0.15, 0.17, 1.0),
            padding: 4.0,
        },
    };
    
    builder.start_element(
        element_type,
        Rect::new(
            Point2D::new(0.0, 0.0),
            Size2D::new(computed.bounds.width(), computed.bounds.height()),
        ),
    );
    
    extract_positions_from_content(
        &computed.content,
        &mut builder,
        Point2D::new(0.0, 0.0),
        fonts,
    );
    
    builder.end_element()
}

/// Types of markdown elements we track
#[derive(Debug, Clone, Copy)]
pub enum MarkdownElementType {
    Heading { level: u8 },
    CodeBlock,
    ListItem { depth: usize, is_ordered: bool },
    InlineCode,
}

/// Get font size for heading level
fn heading_font_size(level: u8) -> f32 {
    match level {
        1 => 24.0,
        2 => 20.0,
        3 => 18.0,
        4 => 16.0,
        5 => 14.0,
        6 => 12.0,
        _ => 16.0,
    }
}

/// Store extracted positions in the sidebar
pub fn store_activity_item_positions(
    sidebar: &mut crate::sidebar::ai_sidebar::AiSidebar,
    item_index: usize,
    position_tree: PositionTree,
    viewport_y: f32,
) {
    let position_data = ItemPositionData {
        position_tree,
        viewport_y: Some(viewport_y),
    };
    
    sidebar.store_item_positions(item_index, position_data);
}