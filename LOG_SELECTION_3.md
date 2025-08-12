# Text Selection System - Implementation Guide v3

## Overview

This document provides the complete implementation guide for fixing and refactoring the text selection system in WezTerm's AI sidebar activity log. The system enables pixel-perfect text selection using glyph position tracking from HarfBuzz clusters.

## Current State Summary

### What's Working ✅
- User message selection (perfectly aligned after fix in last commit - fixed Children bug passing (0,0) instead of accumulated offset)
- Deselection on single click
- Selection scrolling alignment
- 3-coordinate system (Window → Viewport → Item)
- Position extraction infrastructure (all items extract positions successfully)
- Mouse event capture and routing
- Vertical alignment

### Critical Bugs ❌
1. **AI Message Selection**: 4-character LEFT offset from click position (prior session attempted moving UIItemType to outer element - no effect)
2. **Selection Z-Index**: Renders above text instead of behind (rectangles visible and correctly positioned)
3. **Command Items**: Selection and rendering misaligned
4. **Markdown Code Blocks**: NO selection functionality at all
5. **Long AI Messages**: Only first section/chunk works

### Architecture Overview

The system uses a 3-tier coordinate system:
- **WindowCoord**: Absolute position from window origin (0,0)
- **ViewportCoord**: Position relative to visible sidebar activity log area
- **ItemCoord**: Position relative to an activity item's origin

Positions are stored at their render location in item coordinates, eliminating transformation bugs from earlier 5-tier approaches.

## Critical Context from Previous Work

### Virtual Scrolling Architecture
- Activity log uses virtual scrolling with negative margins
- Only visible items (+buffer) are rendered
- Item positions are viewport-relative and change during scrolling
- **Key insight**: Use item indices + item-relative positions for stable references

### Position Tracking via HarfBuzz Clusters
- `CachedGlyph` stores `cluster: Option<u32>` for sidebar text only
- Clusters represent byte offsets in the shaped text segment
- Terminal glyphs have `cluster = None` to avoid memory overhead
- Position extraction happens during rendering, not after

### Coordinate Space Rules
- **Never mix coordinate spaces** - always transform explicitly
- **Don't modify cluster values** - use them as-is from HarfBuzz
- **Positions stored at render location** - no separate content offset transformation
- **Item indexing is stable** - Index 0 = oldest, new items get higher indices

### Known Gotchas
1. **Don't assume uniform line heights** - Markdown has variable spacing
2. **Card wrapper structure** - UIItemType may be on wrapper, not content element
3. **Double transformations** - Common source of offset bugs
4. **Activity log receives entire computed element** - Must find specific item within

## Bug Fix Implementation

### Additional Issues Not Yet Specifically Addressed In This Plan

These issues also need fixing:

**Fix 3: Command Item Selection**
- **Problem**: Selection and rendering misaligned
- **Investigation**: Similar to AI message issue, likely coordinate offset problem

**Fix 4: Code Block Selection**  
- **Problem**: NO selection functionality at all
- **Investigation**: Check if position extraction handles code blocks, verify UIItemType assignment

**Fix 5: Long AI Messages**
- **Problem**: Only first section/chunk works
- **Investigation**: Position extraction may stop after first chunk, check element traversal

## Primary Bug Fixes

### Fix 1: AI Message Selection Offset

**Problem**: AI messages have 4-character LEFT offset from click position while user messages work perfectly.

**Prior Attempt Already Tried**: Moving UIItemType to outer padded element had no effect.

**Root Cause Analysis**: The offset is not due to element structure differences (both user and AI messages attach UIItemType at the same level). The issue is likely in:
1. **Coordinate transformation chain** during markdown content processing
2. **Content offset calculation** in nested markdown elements  
3. **Style/font differences** in markdown vs plain text (per text-layout.md "wrap-before-shape requirement")
4. **Consistent 4-character offset suggests a fixed padding/margin value being missed**

**Investigation Steps**:

1. **Focus on coordinate transformations in `extract_positions_recursively()`** (lines 200-450):
```rust
// Add this debugging around line 200 in extract_positions_recursively()
log::debug!("COORDINATE DEBUG: Element content_rect vs bounds offset: content=({:.1},{:.1}), bounds=({:.1},{:.1}), diff=({:.1},{:.1})",
    computed.content_rect.min_x(), computed.content_rect.min_y(),
    computed.bounds.min_x(), computed.bounds.min_y(),
    computed.content_rect.min_x() - computed.bounds.min_x(),
    computed.content_rect.min_y() - computed.bounds.min_y()
);

// Add position validation
if text_offset.x < 0.0 || text_offset.x > 1000.0 {
    log::warn!("Suspicious text_offset.x: {:.1} for element bounds: {:?}", text_offset.x, computed.bounds);
}
```

2. **Compare markdown vs plain text processing** in the "has_text_content" branch (lines 277-324):
```rust
// Add detailed logging in activity_log_positions.rs around line 300
log::debug!("MARKDOWN vs USER: Processing direct text content");
log::debug!("  child_position_in_parent: {:?}", child_position_in_parent);
log::debug!("  child.bounds: {:?}", child.bounds);
log::debug!("  computed.content_rect: {:?}", computed.content_rect);
log::debug!("  final child_offset: {:?}", child_offset);
```

3. **Check for cluster information availability**:
```rust
// In extract_positions_from_cells_with_wrapped_line() 
let cluster_count = cells.iter().filter(|c| {
    matches!(c, ElementCell::Glyph(g) if g.cluster.is_some())
}).count();

if cluster_count == 0 && cells.len() > 0 {
    log::warn!("No cluster info found for {} cells - position extraction may be inaccurate", cells.len());
}
```

**Fix Implementation**:

The issue is likely in coordinate offset calculation during markdown processing. Focus on these specific areas:

1. **Fix coordinate offset accumulation in markdown processing** (lines 330-360):
```rust
// In extract_positions_recursively(), around line 350
// The bug is likely here - child offset calculation for markdown
let child_position_in_parent: Point2D<f32, PixelUnit> = Point2D::new(
    child.bounds.min_x() - computed.content_rect.min_x(),
    child.bounds.min_y() - computed.content_rect.min_y(),
);

// POTENTIAL FIX: Check if we're double-applying offsets for markdown
// Log the actual values to understand the 4-char offset source
log::debug!("OFFSET ACCUMULATION: parent_offset={:?}, child_position={:?}, total={:?}",
    current_offset, child_position_in_parent, 
    current_offset.x + child_position_in_parent.x
);

// Verify we're not mixing content_rect and bounds incorrectly
if computed.semantic_type.is_some() {  // Markdown element
    log::debug!("MARKDOWN ELEMENT: semantic={:?}, content_offset={:.1}, bounds_offset={:.1}",
        computed.semantic_type,
        computed.content_rect.min_x() - computed.bounds.min_x(),
        child.content_rect.min_x() - child.bounds.min_x()
    );
}
```

2. **Check font metrics differences** between markdown and plain text:
```rust
// In extract_positions_from_cells_with_wrapped_line()
// Different fonts may have different advance widths causing offset
if let ElementCell::Glyph(glyph) = &cells[0] {
    log::debug!("FONT METRICS: x_advance={:.1}, bearing_x={:.1}, x_offset={:.1}",
        glyph.x_advance.get(), glyph.bearing_x.get(), glyph.x_offset.get()
    );
}
```

3. **Verify cluster-to-byte offset mapping**:
```rust
// The 4-character offset might be a consistent byte offset issue
// In wrapped_line.cluster_to_byte_offset()
log::debug!("CLUSTER MAPPING: cluster={}, shaped_offset={}, byte_offset={}, final={}",
    cluster, self.shaped_offset, self.byte_offset, 
    self.byte_offset + self.shaped_offset + cluster as usize
);
```

**Testing**:
1. Add test messages with identical content but different types (user vs AI)
2. Click same character position in both
3. Log and compare extracted positions and hit test results

### Fix 2: Selection Z-Index

**Problem**: Selection rectangles render above text instead of behind. Rectangles are visible and positioned correctly, but they cover the text.

**Confirmed**: The rectangles need to be on the same z-index as text, with rectangles at sub-layer 0 and text at sub-layer 1.

**Investigation**:

The issue is that text is NOT rendering at the expected sub-layer. The architecture is correct (sub-layer 0 for backgrounds, sub-layer 1 for text), but the implementation isn't following it.

1. **Find where activity log text is actually rendered**:
```rust
// In sidebar_render.rs, look for where text glyphs are added
// Search for calls to paint_cached_glyphs() or similar
// The text is likely being rendered at sub-layer 0 instead of 1
```

2. **Check the actual sub-layer used for text rendering**:
```rust
// In render_activity_item_element() or wherever text elements are processed
// Look for the actual sub-layer assignment for text
// It's probably using 0 when it should use 1
```

**Fix Implementation**:

The fix is to ensure text renders at sub-layer 1 while selection stays at sub-layer 0:

```rust
// In sidebar_render.rs where activity log text is rendered
// Find the paint_cached_glyphs() or text rendering call

// CURRENT (likely broken):
layer.paint_cached_glyphs(
    gl_state,
    0,  // <-- This is probably 0, making text render at same sub-layer as selection
    // ...
)?;

// FIXED:
layer.paint_cached_glyphs(
    gl_state,
    1,  // <-- Text should be at sub-layer 1 to appear above selection at sub-layer 0
    // ...
)?;
```

Alternative locations to check:
```rust
// The issue might be in how ElementContent::Text is processed
// Look for where MultilineText or Text content is rendered
// The sub_layer might be hardcoded to 0 there

// In render_element() or similar:
match &element.content {
    ComputedElementContent::Text(_) | 
    ComputedElementContent::MultilineText { .. } => {
        // Check what sub_layer is used here
        let sub_layer = 1; // Should be 1 for text, not 0
    }
}
```

**Verification**:
```rust
// Add debug logging to confirm the fix:
log::debug!("Rendering selection at z-index: {}, sub-layer: {}", z_index, 0);
log::debug!("Rendering text at z-index: {}, sub-layer: {}", z_index, 1);
```

**Testing**:
1. Render selection and verify it appears behind text
2. Test with different background colors to ensure visibility
3. Compare visually with goal card selection behavior

## Refactoring Plan

### Phase 1: Extract Selection Module (4 hours)

**Goal**: Move selection logic out of `ai_sidebar.rs` into dedicated module.

**Important Considerations**:
- Module must be `Send + Sync` for thread safety in `Arc<Mutex<dyn Sidebar>>`
- Must use correct import patterns per sidebar-patterns.md
- Need to handle RefCell borrow conflicts carefully

**Steps**:

1. **Create `wezterm-gui/src/sidebar/text_selection.rs`**:
```rust
//! Text selection handling for sidebar activity log

// CRITICAL: Use correct import pattern per sidebar-patterns.md
use ::window::color::LinearRgba;  // NOT crate::color::LinearRgba
use ::window::PixelUnit;

use crate::sidebar::position_cache::{
    CoordinateTransform, HitResult, ItemPositionData, SelectionPosition, 
    SelectionState, WindowCoord, ViewportCoord, ItemCoord
};
use crate::termwindow::render::activity_log_positions::extract_activity_item_positions;
use crate::termwindow::box_model::ComputedElement;
use crate::sidebar::sidebar_constants::*;
use std::collections::HashMap;
use std::sync::Arc;
use euclid::Point2D;

// Ensure Send + Sync for thread safety
pub struct TextSelectionManager {
    /// Current selection state
    selection_state: SelectionState,
    
    /// Cached position data for items - use Arc for thread safety
    item_positions: HashMap<usize, Arc<ItemPositionData>>,
    
    /// Coordinate transformer
    transform: CoordinateTransform,
}

// Explicitly mark as Send + Sync
unsafe impl Send for TextSelectionManager {}
unsafe impl Sync for TextSelectionManager {}

impl TextSelectionManager {
    pub fn new() -> Self { 
        Self {
            selection_state: SelectionState::default(),
            item_positions: HashMap::new(),
            transform: CoordinateTransform::default(),
        }
    }
    
    /// Start new selection at position
    pub fn start_selection(&mut self, position: SelectionPosition) { ... }
    
    /// Update selection drag
    pub fn update_selection(&mut self, position: SelectionPosition) { ... }
    
    /// Clear selection
    pub fn clear_selection(&mut self) { ... }
    
    /// Hit test at window coordinates
    pub fn hit_test(&self, window_point: Point2D<f32, PixelUnit>) -> Option<HitResult> {
        // Move hit_test_activity_log logic here
    }
    
    /// Calculate selection rectangles for rendering
    pub fn calculate_selection_rects(&self) -> Vec<SelectionRect> {
        // Move calculate_selection_rectangles logic here
    }
    
    /// Get selected text
    pub fn get_selected_text(&self, activity_log: &[ActivityItem]) -> Option<String> {
        // Move selection text extraction here
    }
}
```

2. **Update `ai_sidebar.rs`**:
```rust
pub struct AiSidebar {
    // Replace selection-related fields with:
    selection_manager: TextSelectionManager,
    // Remove: selection_state, item_positions, coordinate_transform
}

// Delegate selection methods:
impl AiSidebar {
    pub fn hit_test_activity_log(&self, point: Point2D<f32>) -> Option<HitResult> {
        self.selection_manager.hit_test(point)
    }
    
    pub fn update_activity_log_selection(&mut self, position: SelectionPosition) {
        self.selection_manager.update_selection(position)
    }
}
```

3. **Move constants to module**:
```rust
// In text_selection.rs
const SELECTION_COLOR: Color = Color::rgba(0.3, 0.5, 0.8, 0.3);
const MIN_SELECTION_WIDTH: f32 = 2.0; // For zero-width cursor
```

### Phase 2: Simplify Position Extraction (3 hours)

**Goal**: Break down 400+ line `extract_positions_recursively` function.

**Steps**:

1. **Extract element type handlers**:
```rust
// In activity_log_positions.rs

fn extract_positions_recursively(computed: &ComputedElement, ...) {
    match &computed.content {
        ComputedElementContent::Text(text) => {
            extract_text_positions(text, computed, builder, offset);
        }
        ComputedElementContent::MultilineText { lines, .. } => {
            extract_multiline_positions(lines, computed, builder, offset);
        }
        ComputedElementContent::Children(children) => {
            extract_children_positions(children, computed, builder, offset, fonts);
        }
        _ => {}
    }
}

fn extract_text_positions(text: &str, computed: &ComputedElement, ...) {
    // Single text element extraction (20-30 lines)
}

fn extract_multiline_positions(lines: &[Vec<ElementCell>], ...) {
    // Multi-line text extraction (50-60 lines)
}

fn extract_children_positions(children: &[ComputedElement], ...) {
    // Handle nested elements (100 lines max)
    // Further break down into:
    // - extract_markdown_element()
    // - extract_plain_children()
}
```

2. **Extract markdown element handlers**:
```rust
fn handle_semantic_element(
    semantic_type: &SemanticType,
    child: &ComputedElement,
    builder: &mut PositionTreeBuilder,
    offset: Point2D<f32>,
) {
    match semantic_type {
        SemanticType::Heading(level) => handle_heading(level, child, builder, offset),
        SemanticType::CodeBlock { .. } => handle_code_block(child, builder, offset),
        SemanticType::ListItem { .. } => handle_list_item(child, builder, offset),
        // etc.
    }
}
```

### Phase 3: Add Performance Optimization (2 hours)

**Goal**: Add dirty tracking to avoid recalculating positions every frame.

**Important**: Must handle virtual scrolling height caching per sidebar-patterns.md requirements.

**Steps**:

1. **Add dirty flag to AiSidebar**:
```rust
pub struct AiSidebar {
    positions_dirty: bool,
    last_viewport_height: f32,
    last_scroll_offset: f32,
    // Cache heights for ALL visible items (not just fully visible)
    cached_item_heights: HashMap<usize, f32>,
}

impl AiSidebar {
    pub fn mark_positions_dirty(&mut self) {
        self.positions_dirty = true;
    }
    
    pub fn update_positions_if_needed(&mut self, viewport_height: f32) {
        let scroll_changed = (self.activity_log_scroll_offset - self.last_scroll_offset).abs() > 0.1;
        let viewport_changed = (viewport_height - self.last_viewport_height).abs() > 0.1;
        
        if self.positions_dirty || scroll_changed || viewport_changed {
            self.recalculate_positions(viewport_height);
            self.positions_dirty = false;
            self.last_viewport_height = viewport_height;
            self.last_scroll_offset = self.activity_log_scroll_offset;
        }
    }
    
    // CRITICAL: Cache ANY visible item per sidebar-patterns.md
    fn cache_item_height(&mut self, index: usize, height: f32, viewport_height: f32) {
        let item_top = self.get_item_top(index);
        let item_bottom = item_top + height;
        
        // CORRECT - Cache ANY visible item to prevent jumps
        if item_bottom > 0.0 && item_top < viewport_height {
            self.cached_item_heights.insert(index, height);
        }
    }
}
```

2. **Mark dirty on changes**:
```rust
// When activity log items added/removed
self.mark_positions_dirty();

// When window resizes
self.mark_positions_dirty();

// Clear cache for removed items
if let Some(removed_index) = removed_item_index {
    self.cached_item_heights.remove(&removed_index);
}
```

### Phase 4: Add Constants (1 hour)

**Goal**: Remove all magic numbers.

**Steps**:

1. **Extend `sidebar_constants.rs`**:
```rust
// Line spacing
pub const LINE_SPACING_MULTIPLIER: f32 = 1.1;

// Selection rendering
pub const SELECTION_COLOR: LinearRgba = LinearRgba { r: 0.3, g: 0.5, b: 0.8, a: 0.3 };
pub const MIN_SELECTION_WIDTH: f32 = 2.0;
pub const SELECTION_Z_INDEX: u8 = 13;
pub const SELECTION_SUB_LAYER: i8 = 0;

// Activity log rendering  
pub const ACTIVITY_LOG_Z_INDEX: u8 = 14;
pub const ACTIVITY_TEXT_SUB_LAYER: i8 = 0;

// Font sizes (already exist, verify complete)
pub const DEFAULT_LINE_HEIGHT: f32 = 20.0;
```

2. **Replace throughout codebase**:
```bash
# Find all numeric literals
grep -n "20\.0\|1\.1\|14\|13" wezterm-gui/src/sidebar/
grep -n "20\.0\|1\.1\|14\|13" wezterm-gui/src/termwindow/render/
```

## Testing Strategy

### Manual Testing Checklist

1. **AI Message Selection Fix**:
   - [ ] Create AI message with markdown (bold, code blocks)
   - [ ] Create user message with same text
   - [ ] Click same position in both - should select same character
   - [ ] Drag to select - should track mouse accurately

2. **Z-Index Fix**:
   - [ ] Select text in activity log
   - [ ] Verify selection appears behind text
   - [ ] Compare with goal card selection appearance
   - [ ] Test with light and dark themes

3. **Refactoring Verification**:
   - [ ] All existing selection features still work
   - [ ] No performance regression
   - [ ] No new warnings/errors in logs
   - [ ] Memory usage remains stable

### Debug Helpers

Add temporary debug visualization:
```rust
// In sidebar_render.rs
if std::env::var("WEZTERM_DEBUG_SELECTION").is_ok() {
    // Render click position as red dot
    // Render extracted positions as green dots
    // Render selection bounds in blue outline
}
```

### Unit Test Requirements

Create `wezterm-gui/src/sidebar/text_selection/tests.rs`:
```rust
#[test]
fn test_coordinate_transformations() {
    // Test Window → Viewport → Item transformations
}

#[test]
fn test_hit_testing_accuracy() {
    // Test clicking on specific characters
}

#[test]
fn test_position_extraction_markdown() {
    // Test extraction from complex markdown structures
}
```

## Success Metrics

1. **AI message selection**: Click position matches selected character exactly
2. **Selection rendering**: Appears behind text like goal card
3. **Code quality**: ai_sidebar.rs reduced from 2800 to <1500 lines
4. **Performance**: Position extraction <5ms for typical activity log
5. **Maintainability**: Each function <50 lines, clear single responsibility

## Implementation Order

1. **INVESTIGATE FIRST - Don't implement fixes yet** (Priority: CRITICAL)
   - Add comprehensive debugging per investigation steps
   - Understand actual root causes before fixing
   - Verify assumptions about z-index and coordinate offsets
   
2. **Fix AI message selection** (Priority: CRITICAL)
   - Focus on coordinate transformation chain
   - Check font metrics differences
   - Verify cluster-to-byte mapping
   
3. **Fix selection z-index** (Priority: HIGH) 
   - Verify actual text rendering sub-layer first
   - May not need any fix if investigation shows correct setup
   
4. **Extract selection module** (Priority: MEDIUM)
   - Only after bugs are fixed
   - Improves maintainability
   - Makes future debugging easier
   
5. **Remaining refactoring** (Priority: LOW)
   - Performance optimization with proper height caching
   - Constant extraction
   - Function simplification with performance considerations

## Key Files Reference

- `wezterm-gui/src/sidebar/ai_sidebar.rs` - Main sidebar logic (needs refactoring)
- `wezterm-gui/src/sidebar/position_cache.rs` - Position data structures
- `wezterm-gui/src/termwindow/render/activity_log_positions.rs` - Position extraction
- `wezterm-gui/src/termwindow/render/sidebar_render.rs` - Selection rendering
- `wezterm-gui/src/termwindow/mouseevent.rs` - Mouse event routing
- `wezterm-gui/src/sidebar/sidebar_constants.rs` - Centralized constants

## Final Notes

The architecture is fundamentally sound. These fixes address implementation bugs, not design flaws. The 3-coordinate system simplification was a good engineering decision that should be preserved. Focus on the two critical bugs first, then improve code organization for long-term maintainability.

Remember: We're building a first-class Rust UI. Don't accept workarounds or quick fixes. Each change should move us toward a clean, maintainable, performant text selection system.