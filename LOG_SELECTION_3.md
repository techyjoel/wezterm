# Text Selection System - Implementation Guide v3

## Overview

This document provides the complete implementation guide for fixing and refactoring the text selection system in WezTerm's AI sidebar activity log. The system enables pixel-perfect text selection using glyph position tracking from HarfBuzz clusters.

## Implementation Status (Updated Session 18)

### Completed Work ✅

#### Session 16: Markdown Selection Fix
- **Root Cause Identified**: Markdown rendering strips formatting (e.g., `**bold**` → `bold`), but positions were extracted for rendered text while selection extracted from original markdown
- **Solution Implemented**: Extract plain text from markdown during selection operations
- **Approach**: WYSIWYG behavior - users copy what they see (rendered text without markdown formatting)

#### Session 17: Multi-Paragraph & Command Items
- **Zero-width selections**: Fixed - no longer shows cursor for single clicks
- **Command item rendering**: Fixed half-width issue by reverting from StyledWrappedText to separate elements
- **Multi-paragraph investigation**: Identified root cause of selection issues
- **Cumulative byte offset tracking**: Partially implemented for multi-paragraph messages

#### Session 18: Deep Dive into Cluster Issues
- **Enhanced debug logging**: Added comprehensive logging with visual indicators (boxes, emojis) to trace byte offsets
- **Position extraction confirmed working**: All glyphs have cluster information, positions ARE being extracted
- **Root cause identified**: Styled text segments (bold, italic) are shaped independently with clusters restarting from 0
- **Attempted fixes**: 
  - Fixed paragraph separator consistency (adding `\n` between text elements)
  - Ensured WrappedLine info is always present (added error logging when missing)
  - Improved cluster_to_byte_offset usage in position extraction

### What's Working ✅
- **User message selection**: Perfectly aligned
- **AI message first paragraph**: Selection works correctly
- **Position extraction infrastructure**: Successfully extracts 200+ positions per message
- **Deselection on single click**: Working
- **Selection scrolling alignment**: Working
- **3-coordinate system**: (Window → Viewport → Item) - stable and correct
- **Position extraction**: Now processes ALL paragraphs (confirmed with logging)
- **Command item rendering**: Fixed display issues
- **Cluster information**: All sidebar text has GlyphWithCluster (no missing clusters)

### Partially Working 🚧
- **Multi-paragraph AI messages with styled text**: 
  - First plain paragraph: Works perfectly ✅
  - Headings with bold text: Off by 1 character to the right
  - Code blocks: Severely misaligned, almost unusable
  - Issue correlates with styled text segments (bold, italic, code)
  - Can select beyond first paragraph but with increasing offset errors

### Remaining Bugs 🔧
1. ~~**AI Message Selection**: 4-character LEFT offset~~ **FIXED in Session 16**
2. ~~**Selection Z-Index**~~ **FIXED in Session 16**
3. **Styled Text Cluster Issue**: Each style segment (bold, italic) gets shaped independently with clusters restarting from 0
4. **Command Items**: Selection and rendering misaligned with the content
5. **Markdown Code Blocks**: Selection severely misaligned due to styled text issue
6. **Long AI Messages**: Progressive offset accumulation makes lower sections unusable
7. ~~**0-width selections**~~ **FIXED in Session 17**

### Critical Implementation Details (Session 18)

#### The Styled Text Segment Problem (ROOT CAUSE)
**Discovery**: Found the actual root cause through detailed logging
- When markdown has styled text (bold, italic), each style segment is shaped INDEPENDENTLY
- Example: "Hello **world**" becomes two segments:
  - Segment 1: "Hello " with clusters [0, 1, 2, 3, 4, 5]
  - Segment 2: "world" with clusters [0, 1, 2, 3, 4] (RESTARTS from 0!)
- The position extraction code sees: [0,1,2,3,4,5,0,1,2,3,4] and misinterprets the byte offsets

**Evidence from logs**:
```
byte_offset=418, 419, 420, 421 (first segment)
byte_offset=423, 424, 423, 423, 424 (second segment - WRONG! Clusters were 0,1,0,0,1)
```

**Why this happens**:
1. `StyledWrappedText` in box_model.rs shapes each style span separately
2. Each shaped segment has its own cluster numbering starting from 0
3. The `WrappedLine::cluster_to_byte_offset()` method expects continuous clusters
4. Position extraction doesn't know about segment boundaries

#### Session 18 Fixes Applied
```rust
// Fixed paragraph separator consistency
if !is_first_text_element {
    *cumulative_byte_offset += 1; // Single \n between paragraphs
}

// Added comprehensive logging
log::debug!("📊 MultilineText: {} glyphs with clusters, {} bytes, starts at byte offset {}", ...);

// Improved WrappedLine usage (attempted fix, didn't solve styled text issue)
extract_cell_positions_internal(cells, builder, offset, line_index, |cluster| {
    let line_byte_offset = wrapped_line.cluster_to_byte_offset(cluster);
    let result = line_byte_offset + byte_offset_adjustment;
    // ...
});
```

#### Remaining TODOs and Workarounds
```rust
// MAJOR TODO: Handle styled text segments properly
// The extract_cell_positions_internal function needs to know about segment boundaries
// OR the cells need to maintain continuous cluster numbering across segments

// Workaround attempt (not implemented): Track cluster restarts
if *cluster < last_cluster {
    // Detected segment boundary, adjust byte offset
    segment_byte_offset += last_cluster + 1;
}

// Still using approximation fallback when WrappedLine missing
fn calculate_text_byte_length_from_cells(cells: &[ElementCell]) -> usize {
    // This is still an approximation and logs warnings
    log::warn!("⚠️ FALLBACK: Estimating text length from clusters");
}
```

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

## Next Steps for Resolution

### The Core Problem to Solve
**Styled text segments have independent cluster numbering**. When markdown contains bold, italic, or other styled text, each segment is shaped separately with clusters starting from 0. The position extraction code doesn't know about these segment boundaries.

### Solution Approaches

#### Option 1: Fix at Text Shaping Level (Recommended)
Modify how `StyledWrappedText` handles clusters to maintain continuous numbering:
- In `shape_line_with_styles()` in box_model.rs
- Track the byte position as segments are shaped
- Adjust cluster values to be continuous across segments
- This fixes the issue at its source

#### Option 2: Fix at Position Extraction Level
Teach position extraction about segment boundaries:
- Detect when clusters restart (cluster < previous_cluster)
- Track cumulative byte offset per segment
- More complex and error-prone

#### Option 3: Store Segment Information
Include segment boundary information with cells:
- Add segment_start_byte to ElementCell::GlyphWithCluster
- Use this to calculate correct byte offsets
- Requires changes throughout the rendering pipeline

### Debugging Next Session
1. **Add logging to text shaping**: Log in `shape_line_with_styles()` to see how segments are created
2. **Track cluster values**: Log cluster values as each segment is shaped
3. **Verify the fix**: Ensure clusters are continuous or segment info is preserved

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

## Deviation from Original Plan

### What Was Planned vs What Was Implemented

**Original Plan (Solution 1: Offset Mapping)**:
- Track mapping between rendered and original markdown positions during parsing
- Thread offset map through Element → WrappedLine → PositionTree pipeline
- Use mapping to translate cluster positions back to original markdown

**What Was Actually Implemented**:
- Extract plain text from markdown during selection operations
- No pipeline modifications needed
- WYSIWYG behavior - users copy rendered text, not markdown source

### Why the Deviation Was Chosen

1. **Architectural Complexity**: Markdown rendering creates multiple independent Elements that get wrapped separately - no clean way to maintain offset mapping through this process
2. **User Expectations**: WYSIWYG behavior (copying what you see) aligns better with modern UI patterns
3. **Simpler Solution**: Isolated change to selection extraction vs modifying entire rendering pipeline
4. **Maintainability**: Fewer moving parts = fewer bugs

### Key Learnings

1. **The 4-character offset mystery**: Caused by `**` markers on each side of bold text being stripped during rendering
2. **Position tracking works correctly**: The infrastructure accurately tracks rendered text positions
3. **Text extraction was the issue**: Not position calculation - just extracting from wrong text representation
4. **WYSIWYG is preferred**: Users expect to copy what they see, not underlying markup

## Primary Bug Fixes

### Fix 1: AI Message Selection Offset ✅ COMPLETED

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

## Implementation Order (ACTUAL)

### Session 16 Implementation

1. **✅ Root Cause Analysis** (COMPLETED)
   - Identified markdown stripping as cause of 4-character offset
   - Confirmed position tracking works correctly for rendered text
   - Issue was text extraction using wrong representation

2. **✅ Fix AI message selection** (COMPLETED)
   - Implemented `get_rendered_text_from_markdown()` for plain text extraction
   - Added `get_item_text_for_selection()` to handle AI vs user messages
   - Enhanced markdown parsing for links, lists, and paragraphs

3. **✅ Fix selection z-index** (COMPLETED)
   - Changed from z-index 14 to 12 (same layer as activity log content)
   - Selection now renders correctly behind text

4. **⏸ Extract selection module** (NOT STARTED)
   - Deferred - current implementation is working
   - Can be done in future refactoring session

5. **⏸ Remaining refactoring** (NOT STARTED)
   - Performance optimizations not critical with current solution
   - Can be addressed if performance issues arise

## Key Files Reference

- `wezterm-gui/src/sidebar/ai_sidebar.rs` - Main sidebar logic (needs refactoring)
- `wezterm-gui/src/sidebar/position_cache.rs` - Position data structures
- `wezterm-gui/src/termwindow/render/activity_log_positions.rs` - Position extraction
- `wezterm-gui/src/termwindow/render/sidebar_render.rs` - Selection rendering
- `wezterm-gui/src/termwindow/mouseevent.rs` - Mouse event routing
- `wezterm-gui/src/sidebar/sidebar_constants.rs` - Centralized constants

## Known Limitations

### Current Implementation Limitations

1. **Styled Text Segments**: Each bold/italic segment resets cluster numbering, causing selection offset
2. **No Markdown in Clipboard**: Users copy plain text, not markdown formatting (by design)
3. **Command Output Selection**: Still misaligned (not addressed)
4. **Code Block Selection**: Severely broken due to styled text segment issue
5. **Progressive Offset Accumulation**: Errors compound in long messages with many styled segments

### Future Work

1. **Complete Markdown Support**: Add nested lists, blockquotes
2. **Command/Code Block Selection**: Investigate and fix remaining selection issues
3. **Performance Optimization**: Cache rendered text if needed
4. **Code Organization**: Extract selection module as planned for better maintainability
5. **Testing**: Add unit tests for markdown extraction and selection logic

## Final Notes

The implementation successfully fixes the critical AI message selection bug using a pragmatic WYSIWYG approach. While this deviates from the original offset mapping plan, it provides a simpler, more maintainable solution that aligns with user expectations.

The position tracking infrastructure remains sound and correctly tracks rendered text positions. The fix was isolated to text extraction during selection operations, demonstrating that the underlying architecture is robust.

Remember: We're building a first-class Rust UI. The current solution is clean, works correctly, and can be enhanced incrementally as needed.