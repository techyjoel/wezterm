//! Activity log position extraction during rendering
//!
//! This module handles extracting glyph positions during the rendering
//! of activity log items to enable pixel-perfect text selection.

use crate::color::LinearRgba;
use crate::sidebar::position_cache::{
    ElementType, ItemPosition, ItemPositionData, PositionTree, PositionTreeBuilder, TextPosition,
    TextStyle,
};
use crate::sidebar::sidebar_constants::*;
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
            // Extract actual line height if this is multiline text
            let line_height = match &computed.content {
                ComputedElementContent::MultilineText { line_height, .. } => *line_height,
                _ => PARAGRAPH_LINE_HEIGHT,
            };

            builder.start_element(
                ElementType::Paragraph {
                    line_height,
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
                computed,
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
    parent_element: &ComputedElement,
) {
    match content {
        ComputedElementContent::Text(cells) => {
            extract_positions_from_cells(cells, builder, offset);
        }
        ComputedElementContent::MultilineText {
            lines,
            line_height,
            line_positions,
            line_info,
            ..
        } => {
            for (line_index, line) in lines.iter().enumerate() {
                // Use actual line positions if available, otherwise calculate
                let y = line_positions
                    .get(line_index)
                    .copied()
                    .unwrap_or_else(|| offset.y + (line_index as f32 * *line_height));

                // Extract positions with line information
                if let Some(wrapped_lines) = line_info {
                    if let Some(wrapped_line) = wrapped_lines.get(line_index) {
                        extract_positions_from_cells_with_wrapped_line(
                            line,
                            builder,
                            Point2D::new(offset.x, y),
                            line_index,
                            wrapped_line,
                        );
                    } else {
                        // Fallback if line info is incomplete
                        extract_positions_from_cells_with_line(
                            line,
                            builder,
                            Point2D::new(offset.x, y),
                            line_index,
                        );
                    }
                } else {
                    extract_positions_from_cells_with_line(
                        line,
                        builder,
                        Point2D::new(offset.x, y),
                        line_index,
                    );
                }
            }
        }
        ComputedElementContent::Children(children) => {
            for child in children {
                let child_offset = offset + child.bounds.origin.to_vector();

                // Check if this is a special markdown element
                if let Some(element_type) = determine_element_type(child, fonts, parent_element) {
                    builder.start_element(element_type, child.bounds);
                    extract_positions_from_content(
                        &child.content,
                        builder,
                        Point2D::new(0.0, 0.0),
                        fonts,
                        child,
                    );
                    builder.end_element();
                } else {
                    extract_positions_from_content(
                        &child.content,
                        builder,
                        child_offset,
                        fonts,
                        child,
                    );
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

/// Common logic for extracting positions from cells
fn extract_cell_positions_internal(
    cells: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    line_index: usize,
    cluster_to_byte: impl Fn(u32) -> usize,
) {
    let mut x_pos = offset.x;

    for cell in cells {
        match cell {
            ElementCell::GlyphWithCluster { glyph, cluster } => {
                let x_start = x_pos;
                let x_end = x_pos + glyph.x_advance.get() as f32;

                // Convert cluster to byte offset using provided function
                let byte_offset = cluster_to_byte(*cluster);

                // Validate that the byte offset is on a character boundary
                // Note: We can't validate against the actual text here since we don't have access to it,
                // but the cluster values from HarfBuzz should always be on character boundaries.
                // The validation happens at a higher level when we have access to the text.

                builder.add_text_position(TextPosition {
                    byte_offset,
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

/// Extract positions from a line of cells with line index
fn extract_positions_from_cells_with_line(
    cells: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    line_index: usize,
) {
    extract_cell_positions_internal(
        cells,
        builder,
        offset,
        line_index,
        |cluster| cluster as usize, // Direct conversion for simple case
    );
}

/// Extract positions from a line of cells with wrapped line information
fn extract_positions_from_cells_with_wrapped_line(
    cells: &[ElementCell],
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    line_index: usize,
    wrapped_line: &crate::termwindow::box_model::WrappedLine,
) {
    extract_cell_positions_internal(cells, builder, offset, line_index, |cluster| {
        wrapped_line.cluster_to_byte_offset(cluster)
    });
}

/// Determine the element type based on semantic type or computed element properties
fn determine_element_type(
    computed: &ComputedElement,
    fonts: &crate::sidebar::SidebarFonts,
    parent: &ComputedElement,
) -> Option<ElementType> {
    // First, check if we have semantic type information
    if let Some(semantic_type) = &computed.semantic_type {
        match semantic_type {
            crate::termwindow::box_model::SemanticType::Heading(level) => {
                let line_height = match &computed.content {
                    ComputedElementContent::MultilineText { line_height, .. } => *line_height,
                    _ => PARAGRAPH_LINE_HEIGHT * HEADING_LINE_HEIGHT_MULTIPLIER,
                };

                // Calculate font size based on user's configured sidebar font size
                let base_font_size = fonts.body.metrics().cell_height.get() as f32;
                let font_size = base_font_size
                    * match level {
                        pulldown_cmark::HeadingLevel::H1 => H1_FONT_SIZE_MULTIPLIER,
                        pulldown_cmark::HeadingLevel::H2 => H2_FONT_SIZE_MULTIPLIER,
                        pulldown_cmark::HeadingLevel::H3 => H3_FONT_SIZE_MULTIPLIER,
                        pulldown_cmark::HeadingLevel::H4 => H4_FONT_SIZE_MULTIPLIER,
                        pulldown_cmark::HeadingLevel::H5 => H5_FONT_SIZE_MULTIPLIER,
                        pulldown_cmark::HeadingLevel::H6 => H6_FONT_SIZE_MULTIPLIER,
                    };

                return Some(ElementType::Heading {
                    level: match level {
                        pulldown_cmark::HeadingLevel::H1 => 1,
                        pulldown_cmark::HeadingLevel::H2 => 2,
                        pulldown_cmark::HeadingLevel::H3 => 3,
                        pulldown_cmark::HeadingLevel::H4 => 4,
                        pulldown_cmark::HeadingLevel::H5 => 5,
                        pulldown_cmark::HeadingLevel::H6 => 6,
                    },
                    font_size,
                    margin: computed.padding.origin.y,
                });
            }
            crate::termwindow::box_model::SemanticType::CodeBlock { .. } => {
                let line_height = match &computed.content {
                    ComputedElementContent::MultilineText { line_height, .. } => *line_height,
                    _ => CODE_LINE_HEIGHT,
                };

                return Some(ElementType::CodeBlock {
                    line_height,
                    padding: computed.padding.width() / 2.0,
                    bg_color: match &computed.colors.bg {
                        crate::termwindow::box_model::InheritableColor::Color(color) => *color,
                        _ => CODE_BLOCK_BG,
                    },
                });
            }
            crate::termwindow::box_model::SemanticType::ListItem { ordered, depth } => {
                let indent = computed.padding.origin.x;

                return Some(ElementType::ListItem {
                    indent,
                    marker_width: LIST_MARKER_WIDTH,
                    depth: *depth,
                    is_ordered: *ordered,
                });
            }
            crate::termwindow::box_model::SemanticType::InlineCode => {
                return Some(ElementType::InlineCode {
                    bg_color: match &computed.colors.bg {
                        crate::termwindow::box_model::InheritableColor::Color(color) => *color,
                        _ => INLINE_CODE_BG,
                    },
                    padding: computed.padding.width() / 2.0,
                });
            }
            // For other semantic types, fall through to default behavior
            _ => {}
        }
    }

    // If no semantic type, fall back to visual detection for backwards compatibility
    // This ensures we don't break existing code that hasn't been updated with semantic tagging
    // TODO: Remove this fallback once all markdown rendering uses semantic types
    log::debug!("No semantic type found, using visual detection fallback for element");

    // DEPRECATED: Visual detection - fragile and theme-dependent
    // Check for code block characteristics
    let has_code_bg = match &computed.colors.bg {
        crate::termwindow::box_model::InheritableColor::Color(color) => {
            // Code blocks typically have a dark gray background
            // Using tuple access for LinearRgba components (r, g, b, a)
            color.0 < CODE_BG_THRESHOLD_R
                && color.1 < CODE_BG_THRESHOLD_G
                && color.2 < CODE_BG_THRESHOLD_B
                && color.3 > 0.9
        }
        crate::termwindow::box_model::InheritableColor::Inherited => false,
        crate::termwindow::box_model::InheritableColor::Animated { .. } => false,
    };

    let has_code_padding = computed.padding.width() > CODE_PADDING_THRESHOLD
        && computed.padding.height() > CODE_PADDING_THRESHOLD;

    if has_code_bg && has_code_padding {
        let line_height = match &computed.content {
            ComputedElementContent::MultilineText { line_height, .. } => *line_height,
            _ => CODE_LINE_HEIGHT,
        };

        return Some(ElementType::CodeBlock {
            line_height,
            padding: computed.padding.width() / 2.0, // padding is total, we want per-side
            bg_color: match &computed.colors.bg {
                crate::termwindow::box_model::InheritableColor::Color(color) => *color,
                _ => CODE_BLOCK_BG,
            },
        });
    }

    // Default: regular paragraph
    None
}

/// Extract positions for markdown-specific elements
pub fn extract_markdown_positions(
    computed: &ComputedElement,
    markdown_type: MarkdownElementType,
    fonts: &crate::sidebar::SidebarFonts,
) -> Option<PositionTree> {
    let mut builder = PositionTreeBuilder::new();

    let base_font_size = fonts.body.metrics().cell_height.get() as f32;
    let element_type = match markdown_type {
        MarkdownElementType::Heading { level } => ElementType::Heading {
            level,
            font_size: base_font_size
                * match level {
                    1 => H1_FONT_SIZE_MULTIPLIER,
                    2 => H2_FONT_SIZE_MULTIPLIER,
                    3 => H3_FONT_SIZE_MULTIPLIER,
                    4 => H4_FONT_SIZE_MULTIPLIER,
                    5 => H5_FONT_SIZE_MULTIPLIER,
                    6 => H6_FONT_SIZE_MULTIPLIER,
                    _ => 1.0,
                },
            margin: 12.0,
        },
        MarkdownElementType::CodeBlock => ElementType::CodeBlock {
            line_height: CODE_LINE_HEIGHT, // Use constant from sidebar_constants
            padding: CODE_BLOCK_PADDING,
            bg_color: CODE_BLOCK_BG,
        },
        MarkdownElementType::ListItem { depth, is_ordered } => ElementType::ListItem {
            indent: LIST_BASE_INDENT + (depth as f32 * LIST_INDENT_STEP),
            marker_width: LIST_MARKER_WIDTH,
            depth,
            is_ordered,
        },
        MarkdownElementType::InlineCode => ElementType::InlineCode {
            bg_color: INLINE_CODE_BG,
            padding: INLINE_CODE_PADDING,
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
        computed,
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
