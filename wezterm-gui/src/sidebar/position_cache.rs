//! Position tracking infrastructure for text selection in the sidebar
//!
//! This module provides the core infrastructure for tracking text positions
//! in the sidebar activity log to enable pixel-perfect text selection.
//! It uses item-relative positioning to handle the dynamic nature of the
//! activity log where items are continuously added and virtual scrolling
//! changes which items are rendered.

use euclid::{Point2D, Rect, Size2D, Vector2D};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use window::PixelUnit;

/// Represents different types of markdown elements with their specific properties
#[derive(Debug, Clone)]
pub enum ElementType {
    /// Regular paragraph text
    Paragraph { line_height: f32, margin: f32 },
    /// Heading text with level (1-6)
    Heading {
        level: u8,
        font_size: f32,
        margin: f32,
    },
    /// Code block with monospace font
    CodeBlock {
        line_height: f32,
        padding: f32,
        bg_color: crate::color::LinearRgba,
    },
    /// List item with indentation
    ListItem {
        indent: f32,
        marker_width: f32,
        depth: usize,
        is_ordered: bool,
    },
    /// Inline text with style
    InlineText {
        style: TextStyle,
        is_link: Option<String>,
    },
    /// Inline code with background
    InlineCode {
        bg_color: crate::color::LinearRgba,
        padding: f32,
    },
}

/// Text style variations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

/// Text affinity for cursor positioning
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAffinity {
    Leading,  // Before the character
    Trailing, // After the character
}

/// Position within an item's text
///
/// # Unicode Safety
/// The byte_offset must be on a valid UTF-8 character boundary.
/// This is guaranteed by the position extraction process which uses
/// HarfBuzz clusters (always on boundaries) and character-aware string operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemPosition {
    pub byte_offset: usize,
    pub affinity: TextAffinity,
}

impl ItemPosition {
    /// Validate that this position's byte offset is on a character boundary
    ///
    /// # Arguments
    /// * `text` - The text to validate against
    ///
    /// # Returns
    /// * `Ok(())` if the offset is valid
    /// * `Err(String)` with details if the offset is invalid
    #[must_use]
    pub fn validate(&self, text: &str) -> Result<(), String> {
        if self.byte_offset > text.len() {
            return Err(format!(
                "Byte offset {} exceeds text length {}",
                self.byte_offset,
                text.len()
            ));
        }

        if self.byte_offset > 0
            && self.byte_offset < text.len()
            && !text.is_char_boundary(self.byte_offset)
        {
            return Err(format!(
                "Byte offset {} is not on a character boundary",
                self.byte_offset
            ));
        }

        Ok(())
    }

    /// Convert byte offset to character index
    ///
    /// # Arguments
    /// * `text` - The text to calculate character index in
    ///
    /// # Returns
    /// The character index, or None if the byte offset is invalid
    pub fn to_char_index(&self, text: &str) -> Option<usize> {
        if self.validate(text).is_ok() {
            Some(text[..self.byte_offset].chars().count())
        } else {
            None
        }
    }
}

/// Hierarchical position tree that mirrors markdown structure
#[derive(Debug, Clone)]
pub struct PositionTree {
    /// Type of this element
    pub element_type: ElementType,
    /// Bounds of this element relative to parent
    pub bounds: Rect<f32, PixelUnit>,
    /// Offset from element bounds to content area where text renders
    /// This is the difference between content_rect and bounds (accounts for padding/border)
    pub content_offset: Vector2D<f32, PixelUnit>,
    /// Text positions within this element (if it contains text)
    pub text_positions: Vec<TextPosition>,
    /// Child elements
    pub children: Vec<PositionTree>,
}

impl PositionTree {
    /// Calculate selection rectangles for a byte range within this element tree
    ///
    /// # Arguments
    /// * `start_byte` - Start byte offset (inclusive)
    /// * `end_byte` - End byte offset (exclusive)
    /// * `parent_offset` - Offset of parent element for coordinate transformation
    ///
    /// # Returns
    /// Vector of rectangles representing the selection, in element-relative coordinates
    pub fn calculate_selection_rectangles(
        &self,
        start_byte: usize,
        end_byte: usize,
        parent_offset: Vector2D<f32, PixelUnit>,
    ) -> Vec<Rect<f32, PixelUnit>> {
        // Validate selection range
        if start_byte > end_byte {
            log::warn!(
                "Invalid selection range: start {} > end {}",
                start_byte,
                end_byte
            );
            return Vec::new();
        }

        let mut rects = Vec::new();
        let element_offset = parent_offset + self.bounds.origin.to_vector();

        // Process text positions in this element
        if !self.text_positions.is_empty() {
            self.add_text_selection_rects(start_byte, end_byte, element_offset, &mut rects);
        }

        // Recurse into children
        for child in &self.children {
            let child_rects =
                child.calculate_selection_rectangles(start_byte, end_byte, element_offset);
            rects.extend(child_rects);
        }

        rects
    }

    /// Get the line height for this element type
    pub fn get_line_height(&self) -> f32 {
        match &self.element_type {
            ElementType::Paragraph { line_height, .. } => *line_height,
            ElementType::Heading { font_size, .. } => font_size * 1.2,
            ElementType::CodeBlock { line_height, .. } => *line_height,
            ElementType::InlineText { .. } => 20.0, // Default
            ElementType::InlineCode { .. } => 20.0, // Default
            ElementType::ListItem { .. } => 20.0,   // Default
        }
    }

    /// Add selection rectangles for text positions
    fn add_text_selection_rects(
        &self,
        start_byte: usize,
        end_byte: usize,
        offset: Vector2D<f32, PixelUnit>,
        rects: &mut Vec<Rect<f32, PixelUnit>>,
    ) {
        // Group positions by line
        let mut lines: HashMap<usize, Vec<&TextPosition>> = HashMap::new();
        for pos in &self.text_positions {
            lines.entry(pos.line_index).or_default().push(pos);
        }

        // Get line height for this element
        let line_height = self.get_line_height();

        // Process each line
        for (line_index, line_positions) in lines {
            if let Some(line_rect) = self.calculate_line_selection_rect(
                line_positions,
                start_byte,
                end_byte,
                line_height,
            ) {
                // Apply offset to convert to absolute coordinates
                let absolute_rect = Rect::new(
                    Point2D::new(line_rect.origin.x + offset.x, line_rect.origin.y + offset.y),
                    line_rect.size,
                );
                rects.push(absolute_rect);
            }
        }
    }

    /// Calculate selection rectangle for a single line
    fn calculate_line_selection_rect(
        &self,
        line_positions: Vec<&TextPosition>,
        start_byte: usize,
        end_byte: usize,
        line_height: f32,
    ) -> Option<Rect<f32, PixelUnit>> {
        // Handle zero-width selection (cursor position)
        if start_byte == end_byte {
            // Find the position at or just before the cursor
            let mut best_pos: Option<&TextPosition> = None;
            for pos in &line_positions {
                if pos.byte_offset <= start_byte {
                    best_pos = Some(pos);
                } else {
                    break;
                }
            }

            if let Some(pos) = best_pos {
                // Place cursor at the end of the found position if it matches exactly,
                // or at the start of the next position
                let x = if pos.byte_offset == start_byte {
                    pos.x_start
                } else {
                    pos.x_end
                };

                return Some(Rect::new(
                    Point2D::new(x, pos.y),
                    Size2D::new(2.0, line_height), // 2px wide cursor
                ));
            }
            return None;
        }

        // Find the leftmost and rightmost positions within the selection range
        let mut min_x = f32::MAX;
        let mut max_x = f32::MIN;
        let mut y_pos = 0.0;
        let mut found_any = false;

        for pos in line_positions {
            // Check if this position is within the selection range
            if pos.byte_offset >= start_byte && pos.byte_offset < end_byte {
                min_x = min_x.min(pos.x_start);
                max_x = max_x.max(pos.x_end);
                y_pos = pos.y;
                found_any = true;
            }
            // Note: We don't need to check for glyphs that span the selection boundary
            // because byte_offset represents the start of the glyph, and selections
            // are made at glyph boundaries in our implementation
        }

        if found_any {
            Some(Rect::new(
                Point2D::new(min_x, y_pos),
                Size2D::new(max_x - min_x, line_height),
            ))
        } else {
            None
        }
    }
}

/// Position information for a single text segment
#[derive(Debug, Clone)]
pub struct TextPosition {
    /// Byte offset in the original text
    pub byte_offset: usize,
    /// X coordinate start (relative to element)
    pub x_start: f32,
    /// X coordinate end (relative to element)
    pub x_end: f32,
    /// Y coordinate (line position, relative to element)
    pub y: f32,
    /// Line index within the element
    pub line_index: usize,
}

/// Item-specific position data
#[derive(Debug, Clone)]
pub struct ItemPositionData {
    /// Position tree relative to item origin (0,0) - stable
    pub position_tree: PositionTree,
    /// Current viewport Y position (changes with scroll)
    pub viewport_y: Option<f32>,
}

/// Coordinate types for explicit transformation tracking
#[derive(Debug, Clone, Copy)]
pub struct WindowCoord(pub Point2D<f32, PixelUnit>);

#[derive(Debug, Clone, Copy)]
pub struct ViewportCoord(pub Point2D<f32, PixelUnit>);

#[derive(Debug, Clone, Copy)]
pub struct ItemCoord(pub Point2D<f32, PixelUnit>);

#[derive(Debug, Clone, Copy)]
pub struct ElementCoord(pub Point2D<f32, PixelUnit>);

/// Stable selection position using item index and byte offset
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionPosition {
    /// Which activity item (0 = oldest)
    pub item_index: usize,
    /// Position within that item
    pub position_in_item: ItemPosition,
}

/// Selection state using stable positions
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub anchor: Option<SelectionPosition>,
    pub current: Option<SelectionPosition>,
    pub is_dragging: bool,
}

/// Result of hit testing
#[derive(Debug, Clone, Copy)]
pub struct HitResult {
    pub item_index: usize,
    pub position_in_item: ItemPosition,
}

/// Selection rectangle for rendering
#[derive(Debug, Clone)]
pub struct SelectionRect {
    pub rect: Rect<f32, PixelUnit>,
    pub element_type: ElementType,
}

/// Coordinate transformation helper
#[derive(Debug, Clone)]
pub struct CoordinateTransform {
    pub sidebar_x: f32,
    pub sidebar_y: f32,
    pub sidebar_width: f32,
    pub sidebar_height: f32,
}

impl CoordinateTransform {
    pub fn window_to_viewport(&self, w: WindowCoord) -> ViewportCoord {
        ViewportCoord(w.0 - Vector2D::new(self.sidebar_x, self.sidebar_y))
    }

    pub fn viewport_to_item(&self, v: ViewportCoord, item_viewport_y: f32) -> ItemCoord {
        ItemCoord(v.0 - Vector2D::new(0.0, item_viewport_y))
    }

    pub fn item_to_element(
        &self,
        i: ItemCoord,
        element_bounds: &Rect<f32, PixelUnit>,
    ) -> ElementCoord {
        ElementCoord(i.0 - element_bounds.origin.to_vector())
    }
}

/// Cache key for position data
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct PositionCacheKey {
    pub item_index: usize,
    pub content_hash: u64,
}

/// LRU cache for position data
///
/// Note: This uses a Vec for LRU tracking with O(n) lookup for simplicity.
/// With our limit of 100 entries, this is acceptable performance-wise.
/// For larger caches, consider using a proper LRU data structure like
/// the `lru` crate or `IndexMap`.
pub struct TextPositionCache {
    /// Cached position trees by key
    positions: HashMap<PositionCacheKey, Arc<PositionTree>>,
    /// LRU tracking - most recently used at the back
    access_order: Vec<PositionCacheKey>,
    /// Maximum number of entries
    max_entries: usize,
}

impl TextPositionCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            positions: HashMap::new(),
            access_order: Vec::new(),
            max_entries,
        }
    }

    pub fn get(&mut self, key: &PositionCacheKey) -> Option<Arc<PositionTree>> {
        if let Some(tree) = self.positions.get(key) {
            // Update LRU order
            self.access_order.retain(|k| k != key);
            self.access_order.push(key.clone());
            Some(tree.clone())
        } else {
            None
        }
    }

    pub fn insert(&mut self, key: PositionCacheKey, tree: PositionTree) {
        // Evict oldest if at capacity
        if self.positions.len() >= self.max_entries && !self.positions.contains_key(&key) {
            if let Some(oldest) = self.access_order.first().cloned() {
                self.positions.remove(&oldest);
                self.access_order.remove(0);
            }
        }

        self.positions.insert(key.clone(), Arc::new(tree));
        self.access_order.retain(|k| k != &key);
        self.access_order.push(key);
    }

    pub fn clear(&mut self) {
        self.positions.clear();
        self.access_order.clear();
    }
}

/// Builder for constructing position trees
pub struct PositionTreeBuilder {
    current_element: Option<PositionTree>,
    element_stack: Vec<PositionTree>,
    current_y: f32,
}

impl PositionTreeBuilder {
    pub fn new() -> Self {
        Self {
            current_element: None,
            element_stack: Vec::new(),
            current_y: 0.0,
        }
    }

    pub fn start_element(&mut self, element_type: ElementType, bounds: Rect<f32, PixelUnit>) {
        self.start_element_with_offset(element_type, bounds, Vector2D::zero())
    }
    
    pub fn start_element_with_offset(
        &mut self, 
        element_type: ElementType, 
        bounds: Rect<f32, PixelUnit>,
        content_offset: Vector2D<f32, PixelUnit>
    ) {
        if let Some(current) = self.current_element.take() {
            self.element_stack.push(current);
        }

        self.current_element = Some(PositionTree {
            element_type,
            bounds,
            content_offset,
            text_positions: Vec::new(),
            children: Vec::new(),
        });
        self.current_y = 0.0;
    }

    pub fn add_text_position(&mut self, position: TextPosition) {
        if let Some(ref mut current) = self.current_element {
            // Check for duplicate or overlapping positions
            // This prevents markdown elements from creating multiple selection rectangles
            // We consider positions duplicate if they have the same byte offset,
            // even if x coordinates differ slightly due to rounding or nested elements
            let is_duplicate = current.text_positions.iter().any(|p| {
                // Same byte offset is always a duplicate
                if p.byte_offset == position.byte_offset {
                    // Allow if on different lines (legitimate multi-line text)
                    if p.line_index != position.line_index {
                        return false;
                    }
                    // Same byte, same line = duplicate
                    return true;
                }
                false
            });
            
            if !is_duplicate {
                current.text_positions.push(position);
            } else {
                log::trace!(
                    "Skipping duplicate text position: byte_offset={}, line_index={}, x={:.1}-{:.1}",
                    position.byte_offset,
                    position.line_index,
                    position.x_start,
                    position.x_end
                );
            }
        }
    }

    pub fn end_element(&mut self) -> Option<PositionTree> {
        if let Some(completed) = self.current_element.take() {
            if let Some(mut parent) = self.element_stack.pop() {
                parent.children.push(completed);
                self.current_element = Some(parent);
                None
            } else {
                Some(completed)
            }
        } else {
            None
        }
    }

    pub fn build(mut self) -> Option<PositionTree> {
        // End any remaining elements
        while !self.element_stack.is_empty() {
            log::debug!(
                "PositionTreeBuilder::build: Ending {} remaining elements on stack",
                self.element_stack.len()
            );
            self.end_element();
        }

        if self.current_element.is_none() {
            log::debug!("PositionTreeBuilder::build: No current element - returning None");
        } else if let Some(ref elem) = self.current_element {
            log::debug!(
                "PositionTreeBuilder::build: Returning tree with {} text positions",
                elem.text_positions.len()
            );
        }

        self.current_element
    }
}

/// Helper to calculate content hash for cache invalidation
pub fn calculate_content_hash(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}
