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
use crate::termwindow::box_model::{ComputedElement, ComputedElementContent, ElementCell, WrappedLine};
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
    item_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,
) -> Option<PositionTree> {
    match ui_item_type {
        UIItemType::ActivityItemText { index, .. } => {
            log::debug!("Extracting positions for activity item {}", index);

            // First, try to find the specific activity item element within the computed element
            // This is necessary because we receive the entire activity log computed element
            // but need to extract positions from the specific activity item
            if let Some(activity_item_element) = find_activity_item_element(computed, *index) {
                log::debug!("Found activity item {} element", index);
                extract_positions_from_activity_item(activity_item_element, fonts, None)
            } else {
                // Fallback: if we can't find the specific item, try to extract from the whole element
                // This path is less accurate but maintains backward compatibility
                log::debug!("Failed to find activity item {} in computed element, using fallback extraction with bounds: {:?}", index, item_bounds);
                // Pass the actual item bounds to ensure correct position tree bounds
                extract_positions_from_activity_item(computed, fonts, item_bounds)
            }
        }
        _ => None,
    }
}

/// Find the specific activity item element within the activity log
fn find_activity_item_element(
    computed: &ComputedElement,
    target_index: usize,
) -> Option<&ComputedElement> {
    // First check if this element itself has the matching UIItemType
    if let Some(item_type) = &computed.item_type {
        if matches!(item_type, UIItemType::ActivityItemText { index, .. } if *index == target_index)
        {
            log::debug!("Found activity item {} at current element", target_index);
            return Some(computed);
        }
    }

    // Then search through children
    match &computed.content {
        ComputedElementContent::Children(children) => {
            log::debug!(
                "Searching {} children for activity item {}",
                children.len(),
                target_index
            );
            for (i, child) in children.iter().enumerate() {
                // Check if this child has the matching UIItemType
                if let Some(item_type) = &child.item_type {
                    if matches!(item_type, UIItemType::ActivityItemText { index, .. } if *index == target_index)
                    {
                        log::debug!(
                            "Found matching activity item {} at child {}",
                            target_index,
                            i
                        );
                        return Some(child);
                    }
                }

                // Recursively search in children
                if let Some(found) = find_activity_item_element(child, target_index) {
                    return Some(found);
                }
            }
        }
        _ => {}
    }
    None
}

/// Extract positions from a specific activity item element
fn extract_positions_from_activity_item(
    computed: &ComputedElement,
    fonts: &crate::sidebar::SidebarFonts,
    override_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,
) -> Option<PositionTree> {
    let mut builder = PositionTreeBuilder::new();

    // Determine the line height from the computed element
    let line_height = extract_line_height(computed);

    // Use override bounds if provided (for fallback case), otherwise use computed bounds
    let bounds = override_bounds.unwrap_or(computed.bounds);
    
    // Calculate the content offset - this is where text actually renders relative to bounds
    // Text renders at content_rect position, not at element origin (0,0)
    let content_offset = Point2D::new(
        computed.content_rect.min_x() - computed.bounds.min_x(),
        computed.content_rect.min_y() - computed.bounds.min_y()
    );

    log::debug!(
        "Extracting positions from activity item with bounds: {:?}, content_rect: {:?}, content_offset: {:?}, line_height: {}",
        bounds,
        computed.content_rect,
        content_offset,
        line_height
    );

    // Create a root element to hold everything
    // Store the content offset so we know where text actually renders relative to bounds
    builder.start_element_with_offset(
        ElementType::Paragraph {
            line_height,
            margin: 0.0,
        },
        Rect::new(
            Point2D::new(0.0, 0.0),
            Size2D::new(bounds.width(), bounds.height()),
        ),
        euclid::Vector2D::new(content_offset.x, content_offset.y),
    );

    // Extract positions from the entire computed element tree
    // Pass the content offset so positions are extracted where text actually renders
    extract_positions_recursively(computed, &mut builder, content_offset, fonts);

    // Don't call end_element() here - the root element should remain as current_element
    // so that build() can return it
    let result = builder.build();

    if let Some(ref tree) = result {
        log::debug!(
            "Successfully extracted position tree with {} text positions, bounds: {:?}",
            tree.text_positions.len(),
            tree.bounds
        );
    } else {
        log::debug!("Failed to build position tree");
    }

    result
}

/// Extract line height from computed element or its children
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
            // Recursively search deeper
            for child in children {
                let height = extract_line_height(child);
                if height != PARAGRAPH_LINE_HEIGHT {
                    return height;
                }
            }
            PARAGRAPH_LINE_HEIGHT
        }
        ComputedElementContent::Text(_) | ComputedElementContent::Poly { .. } => {
            PARAGRAPH_LINE_HEIGHT
        }
    }
}

/// Recursively extract positions from all text content in the element tree
fn extract_positions_recursively(
    computed: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32, PixelUnit>,
    fonts: &crate::sidebar::SidebarFonts,
) {
    match &computed.content {
        ComputedElementContent::Text(cells) => {
            // When we find text content, we need to add this element's content offset
            // The offset parameter is relative to the root content area
            // But this text renders at this element's content_rect
            let text_offset: Point2D<f32, PixelUnit> = Point2D::new(
                offset.x + (computed.content_rect.min_x() - computed.bounds.min_x()),
                offset.y + (computed.content_rect.min_y() - computed.bounds.min_y())
            );
            log::debug!(
                "Found Text content with {} cells at offset {:?}, text_offset {:?}",
                cells.len(),
                offset,
                text_offset
            );
            // Extract positions at the actual text rendering position
            extract_positions_from_cells(cells, builder, text_offset);
        }
        ComputedElementContent::MultilineText { lines, line_height, line_positions, line_info, .. } => {
            // When we find multiline text, we need to add this element's content offset
            let text_offset: Point2D<f32, PixelUnit> = Point2D::new(
                offset.x + (computed.content_rect.min_x() - computed.bounds.min_x()),
                offset.y + (computed.content_rect.min_y() - computed.bounds.min_y())
            );
            log::debug!(
                "Found MultilineText content with {} lines at offset {:?}, text_offset {:?}",
                lines.len(),
                offset,
                text_offset
            );
            // Extract positions at the actual text rendering position (content coordinates)
            for (line_index, line) in lines.iter().enumerate() {
                // Use line_positions if available, otherwise calculate based on line_height
                // These Y positions are relative to the content area
                let y = line_positions
                    .get(line_index)
                    .copied()
                    .unwrap_or_else(|| line_index as f32 * *line_height);

                if let Some(wrapped_lines) = line_info {
                    if let Some(wrapped_line) = wrapped_lines.get(line_index) {
                        extract_positions_from_cells_with_wrapped_line(
                            line,
                            builder,
                            Point2D::new(text_offset.x, text_offset.y + y),  // Use text_offset
                            line_index,
                            wrapped_line,
                        );
                    } else {
                        extract_positions_from_cells_with_line(
                            line,
                            builder,
                            Point2D::new(text_offset.x, text_offset.y + y),  // Use text_offset
                            line_index,
                        );
                    }
                } else {
                    extract_positions_from_cells_with_line(
                        line,
                        builder,
                        Point2D::new(text_offset.x, text_offset.y + y),  // Use text_offset
                        line_index,
                    );
                }
            }
        }
        ComputedElementContent::Children(children) => {
            log::debug!("Processing Children with {} elements", children.len());
            
            // IMPORTANT: To prevent duplicate position extraction in markdown,
            // we should only extract positions from the leaf elements that contain
            // actual text, not from every level of the tree.
            // 
            // Check if any child has actual text content (not just more Children)
            let has_text_content = children.iter().any(|child| {
                matches!(&child.content, 
                    ComputedElementContent::Text(_) | 
                    ComputedElementContent::MultilineText { .. })
            });
            
            // If this element directly contains text, don't recurse into children
            // as they would be duplicates of the same content
            if has_text_content {
                log::debug!("Children contain direct text content, processing only text elements");
                for child in children {
                    match &child.content {
                        ComputedElementContent::Text(_) | 
                        ComputedElementContent::MultilineText { .. } => {
                            // Process text content directly
                            extract_positions_recursively(child, builder, Point2D::new(0.0, 0.0), fonts);
                        }
                        _ => {
                            // Skip non-text children to avoid duplicates
                        }
                    }
                }
                return;  // Don't process children recursively
            }
            
            // Otherwise, process children normally for nested structures
            let mut current_offset = offset;

            // For Card structures, we need to search through all nested children
            // to find the actual text content, even if they don't have semantic types
            for (i, child) in children.iter().enumerate() {
                log::debug!(
                    "Processing child {} at offset {:?}, bounds: {:?}",
                    i,
                    current_offset,
                    child.bounds
                );

                // For nested children, we need to calculate the position relative to the root content area
                // The current_offset already includes the root's content offset
                // We just need to add this child's position relative to its parent
                let child_position_in_parent: Point2D<f32, PixelUnit> = Point2D::new(
                    child.bounds.min_x() - computed.content_rect.min_x(),
                    child.bounds.min_y() - computed.content_rect.min_y()
                );
                
                // The child's offset for text extraction is the parent's offset plus child's position
                // We don't add the child's own content offset here - that will be handled
                // when we actually extract text from this child
                let child_absolute_offset = Point2D::new(
                    current_offset.x + child_position_in_parent.x,
                    current_offset.y + child_position_in_parent.y
                );

                // Debug log the child structure
                log::debug!(
                    "Child {} offset: parent_offset={:?}, child_position_in_parent={:?}, child_absolute_offset={:?}",
                    i, current_offset, child_position_in_parent, child_absolute_offset
                );
                
                // Check if this child has semantic type (for markdown elements)
                let mut started_element = false;
                if let Some(semantic_type) = &child.semantic_type {
                    // Handle specific markdown elements with proper element types
                    match semantic_type {
                        crate::termwindow::box_model::SemanticType::Heading(level) => {
                            // Convert pulldown_cmark::HeadingLevel to u8
                            let level_u8 = match level {
                                pulldown_cmark::HeadingLevel::H1 => 1,
                                pulldown_cmark::HeadingLevel::H2 => 2,
                                pulldown_cmark::HeadingLevel::H3 => 3,
                                pulldown_cmark::HeadingLevel::H4 => 4,
                                pulldown_cmark::HeadingLevel::H5 => 5,
                                pulldown_cmark::HeadingLevel::H6 => 6,
                            };

                            // Calculate font size based on level
                            let font_size_multiplier = match level_u8 {
                                1 => H1_FONT_SIZE_MULTIPLIER,
                                2 => H2_FONT_SIZE_MULTIPLIER,
                                3 => H3_FONT_SIZE_MULTIPLIER,
                                4 => H4_FONT_SIZE_MULTIPLIER,
                                5 => H5_FONT_SIZE_MULTIPLIER,
                                6 => H6_FONT_SIZE_MULTIPLIER,
                                _ => 1.0,
                            };
                            let font_size = fonts.body.metrics().cell_height.get() as f32
                                * font_size_multiplier;

                            builder.start_element(
                                ElementType::Heading {
                                    level: level_u8,
                                    font_size,
                                    margin: 0.0, // TODO: Add proper heading margin constant
                                },
                                Rect::new(
                                    Point2D::new(0.0, 0.0),  // Element-relative coordinates
                                    Size2D::new(child.bounds.width(), child.bounds.height()),
                                ),
                            );
                            started_element = true;
                        }
                        crate::termwindow::box_model::SemanticType::CodeBlock { .. } => {
                            builder.start_element(
                                ElementType::CodeBlock {
                                    line_height: CODE_LINE_HEIGHT,
                                    padding: CODE_BLOCK_PADDING,
                                    bg_color: CODE_BLOCK_BG,
                                },
                                Rect::new(
                                    Point2D::new(0.0, 0.0),  // Element-relative coordinates
                                    Size2D::new(child.bounds.width(), child.bounds.height()),
                                ),
                            );
                            started_element = true;
                        }
                        _ => {
                            // Other semantic types we don't create elements for
                            log::debug!(
                                "Child {} has semantic type {:?} but no element created",
                                i,
                                semantic_type
                            );
                        }
                    }
                } else {
                    // No semantic type - this might be a Card wrapper or other container
                    // We still need to process its children to find text content
                    log::debug!(
                        "Child {} has no semantic type, recursing to find text content",
                        i
                    );
                }

                // ALWAYS recursively process this child, whether it has semantic type or not
                // This is crucial for Card wrappers where the text is nested inside
                // Use the calculated child offset to position text where it actually renders
                extract_positions_recursively(child, builder, child_absolute_offset, fonts);

                // End element ONLY if we actually started one
                if started_element {
                    log::debug!(
                        "Ending semantic element after processing child {} of {}",
                        i,
                        children.len()
                    );
                    builder.end_element();
                }

                // Don't update current_offset.y here - use the actual child bounds for positioning
            }
        }
        ComputedElementContent::Poly { .. } => {
            // Poly content is for drawing shapes, not text - skip it
        }
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
            // Don't process children here - let extract_positions_recursively handle them
            // This prevents duplicate position extraction
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
    let mut glyph_count = 0;
    let mut cluster_count = 0;

    log::debug!(
        "extract_cell_positions_internal: Processing {} cells at offset {:?}",
        cells.len(),
        offset
    );

    for (i, cell) in cells.iter().enumerate() {
        match cell {
            ElementCell::GlyphWithCluster { glyph, cluster } => {
                cluster_count += 1;
                let x_start = x_pos;
                let x_end = x_pos + glyph.x_advance.get() as f32;

                // Convert cluster to byte offset using provided function
                let byte_offset = cluster_to_byte(*cluster);

                if i < 5 {
                    // Log first few glyphs for debugging
                    log::debug!(
                        "GlyphWithCluster {}: cluster={}, byte_offset={}, x_start={}, x_end={}",
                        i,
                        cluster,
                        byte_offset,
                        x_start,
                        x_end
                    );
                }

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
                
                // Debug first few positions to understand coordinate system
                if cluster_count <= 3 {
                    log::debug!(
                        "Added text position: byte_offset={}, x_start={:.1}, x_end={:.1}, y={:.1} (offset was x={:.1}, y={:.1})",
                        byte_offset, x_start, x_end, offset.y, offset.x, offset.y
                    );
                }

                x_pos = x_end;
            }
            ElementCell::Glyph(cached_glyph) => {
                // Regular glyphs without cluster info - skip position tracking
                glyph_count += 1;
                x_pos += cached_glyph.x_advance.get() as f32;
            }
            ElementCell::Sprite(sprite) => {
                x_pos += sprite.coords.size.width as f32;
            }
        }
    }

    log::debug!(
        "Extracted positions from {} cells: {} with clusters, {} regular glyphs",
        cells.len(),
        cluster_count,
        glyph_count
    );
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
    log::debug!(
        "Storing position tree for activity item {} with {} text positions at viewport_y={}",
        item_index,
        position_tree.text_positions.len(),
        viewport_y
    );

    let position_data = ItemPositionData {
        position_tree,
        viewport_y: Some(viewport_y),
    };

    sidebar.store_item_positions(item_index, position_data);
}
