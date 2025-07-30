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
    Paragraph { 
        line_height: f32, 
        margin: f32 
    },
    /// Heading text with level (1-6)
    Heading { 
        level: u8, 
        font_size: f32, 
        margin: f32 
    },
    /// Code block with monospace font
    CodeBlock { 
        line_height: f32,
        padding: f32, 
        bg_color: crate::color::LinearRgba 
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
#[derive(Debug, Clone, PartialEq)]
pub struct ItemPosition {
    pub byte_offset: usize,
    pub affinity: TextAffinity,
}

/// Hierarchical position tree that mirrors markdown structure
#[derive(Debug, Clone)]
pub struct PositionTree {
    /// Type of this element
    pub element_type: ElementType,
    /// Bounds of this element relative to parent
    pub bounds: Rect<f32, PixelUnit>,
    /// Text positions within this element (if it contains text)
    pub text_positions: Vec<TextPosition>,
    /// Child elements
    pub children: Vec<PositionTree>,
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
#[derive(Debug, Clone)]
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
    
    pub fn item_to_element(&self, i: ItemCoord, element_bounds: &Rect<f32, PixelUnit>) -> ElementCoord {
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
        if let Some(current) = self.current_element.take() {
            self.element_stack.push(current);
        }
        
        self.current_element = Some(PositionTree {
            element_type,
            bounds,
            text_positions: Vec::new(),
            children: Vec::new(),
        });
        self.current_y = 0.0;
    }
    
    pub fn add_text_position(&mut self, position: TextPosition) {
        if let Some(ref mut current) = self.current_element {
            current.text_positions.push(position);
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
            self.end_element();
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