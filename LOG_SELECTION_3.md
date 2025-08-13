# Text Selection System - Implementation Guide v3

## Overview

This document provides the complete implementation guide for fixing and refactoring the text selection system in WezTerm's AI sidebar activity log. The system enables pixel-perfect text selection using glyph position tracking from HarfBuzz clusters.

## Implementation Status (Updated Session 21)

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

#### Session 19: Cluster Fix & Long Message Selection Investigation
- **Implemented cluster adjustment fix**: Added `shape_text_to_cells_with_offset` helper in box_model.rs
  - Tracks segment byte positions within line text
  - Adjusts clusters by segment start position to maintain continuous numbering
  - Result: Clusters now represent byte positions within full line, not segment-local
- **Testing revealed partial improvement**: 
  - Can now select inline markdown styles (e.g. italic text)
  - Code blocks still have offset issues
- **Root cause of long message selection failure discovered**:
  - Positions stored with viewport/window y-coordinates instead of item-relative
  - When scrolling or clicking far down, y-coordinates don't match stored positions
  - Hit test fails with "No positions found on line at y=784.0"
- **Added comprehensive debug logging**:
  - Fallback path detection
  - Cumulative byte offset tracking with corruption detection
  - Code block processing visualization
  - Hit test recursion tracking

#### Session 20: Deep Investigation of Y-Coordinate Bug
- **Identified the REAL root cause**: UIItem y-coordinates are clamped to 0 in `box_model.rs`
  - When items scroll above viewport (negative y), UIItem.y gets clamped to 0
  - This makes viewport_y calculation wrong: `ui_item.y - activity_bounds.origin.y`
  - Result: viewport_y stops updating at -577px even when scrolled further
- **Why -577px limit**: That's when the item's y-coordinate gets clamped to 0
- **Discovery through debug analysis**:
  - "Detailed Troubleshooting" is at item-relative y=1515
  - When clicking at y=740, we're ~775px above where it actually is
  - Visual rendering doesn't match stored positions
- **Attempted fix**: Modified viewport_y calculation to use virtual scrolling data
  - Added `calculate_item_top_position` and `get_activity_log_scroll_offset` methods
  - Tried to calculate correct viewport_y when UIItem.y is clamped
  - **Result**: Fix didn't work as expected, viewport_y still behaves the same

#### Session 21: Successful Fix for Long Message Selection
- **Root Cause Analysis**: Through debug logs, identified that the issue wasn't just UIItem clamping, but that clamped bounds were being used to transform selection rectangles from item to window coordinates
- **Solution Implemented**: "Solution A" - Direct unclamped bounds tracking
  - Created `extract_activity_item_positions_with_unclamped_bounds()` that walks ComputedElement tree directly
  - Captures unclamped bounds BEFORE UIItem creation (which clamps negative y to 0)
  - Stores unclamped bounds for both position extraction AND selection rectangle transformation
- **Key Discovery**: The bug had TWO parts:
  1. viewport_y calculation was using clamped UIItem.y (partially addressed in Session 20)
  2. Selection rectangle transformation was ALSO using clamped bounds (the critical missing piece)
- **Safety Improvements Added**:
  - Recursion depth limiting (MAX_TREE_DEPTH = 50) to prevent stack overflow
  - Bounds validation to prevent NaN/infinity issues
  - Removed unused code from failed virtual scrolling attempt
- **Result**: **SUCCESSFULLY FIXED** - Text selection now works at any scroll position in long messages

### What's Working ✅
- **Long message selection**: **NOW WORKING** - Can select text at any scroll position
- **User message selection**: Perfectly aligned
- **AI message first paragraph**: Selection works correctly
- **Position extraction infrastructure**: Successfully extracts 200+ positions per message
- **Deselection on single click**: Working
- **Selection scrolling alignment**: Working
- **3-coordinate system**: (Window → Viewport → Item) - stable and correct
- **Position extraction**: processes ALL paragraphs
- **Command item rendering**: Fixed display issues
- **Cluster information**: All sidebar text has GlyphWithCluster (no missing clusters)
- **Coordinate transformation**: Unclamped bounds properly used for selection rendering

### Partially Working 🚧
- **Multi-paragraph AI messages with styled text**: 
  - First plain paragraph: Works perfectly ✅
  - First headings with bold text: rendered rectangle is aligned, cmd-c selection is 1 character off to the right
  - Code blocks: 
    - Rectangle renders 1/2 line too high
    - Selection is 2 characters off to the left
  - Styled text segments now have continuous clusters (partial fix applied)

### Remaining Bugs 🔧
1. ~~**AI Message Selection**: 4-character LEFT offset~~ **FIXED in Session 16**
2. ~~**Selection Z-Index**~~ **FIXED in Session 16**
3. ~~**Styled Text Cluster Issue**: Each style segment gets shaped independently~~ **PARTIALLY FIXED in Session 19**
   - Cluster adjustment implemented but small offsets remain
4. ~~**Long Message Selection Failure**~~ **FIXED in Session 21**
5. **Heading Selection**: 1 character offset to the right (cmd-c copies 1 char off, visual rect is correct)
6. **Code Block Rendering**: Rectangle renders 1/2 line too high  
7. **Code Block Selection**: 2 characters offset to the left
8. **Command Items**: Selection and rendering misaligned with the content
9. ~~**0-width selections**~~ **FIXED in Session 17**

### Critical Implementation Details (Session 21)

#### The Complete Fix for Long Message Selection

**The Two-Part Bug**:
1. **Part 1**: UIItem.y gets clamped to 0 when items scroll above viewport (because UIItem uses `usize`)
2. **Part 2**: Selection rectangle transformation was using these clamped bounds to convert from item to window coordinates

**Why Previous Attempts Failed**:
- Session 20's virtual scrolling approach failed because it tried to reconstruct the unclamped y-coordinate after the fact
- The calculation used hardcoded defaults (line_height=20.0, width=400.0) that didn't match actual rendering
- Even if viewport_y was calculated correctly, selection rectangles would still be mispositioned due to using clamped bounds for transformation

**The Successful Solution**:
```rust
// In sidebar_render.rs - Walk the ComputedElement tree BEFORE UIItem creation
fn extract_activity_item_positions_with_unclamped_bounds() {
    // Captures unclamped bounds directly from ComputedElement
    let unclamped_bounds = computed.bounds; // Can be negative!
    
    // Store unclamped bounds for selection rectangle transformation
    ai_sidebar.set_activity_item_bounds(*index, unclamped_bounds);
    
    // Use unclamped bounds for viewport_y calculation
    let viewport_y = unclamped_bounds.min_y() - activity_bounds.origin.y;
}
```

**Safety Measures Added**:
- **Stack overflow prevention**: MAX_TREE_DEPTH = 50 with recursion limiting
- **Bounds validation**: Check for NaN/infinity before using floating point values
- **Code cleanup**: Removed failed virtual scrolling methods

**Performance Consideration**:
- Walking the ComputedElement tree adds minimal overhead as it happens during the same render pass
- The tree walk is bounded by MAX_TREE_DEPTH to prevent pathological cases

### Critical Implementation Details (Session 19)

#### The Styled Text Segment Problem (PARTIALLY RESOLVED)
**Original Issue**: Each style segment was shaped independently with clusters restarting from 0
- Example: "Hello **world**" became two segments with clusters [0,1,2,3,4,5] and [0,1,2,3,4]

**Fix Applied in Session 19**:
```rust
// In box_model.rs - Added helper to adjust clusters
fn shape_text_to_cells_with_offset(..., cluster_offset: u32) {
    // Adjusts clusters by segment start position
    cluster + cluster_offset
}

// In shape_line_with_styles - Track segment positions
let cluster_offset = start as u32; // Byte position within line_text
```

**Result**: Clusters now continuous, but small offsets remain (1-2 chars) suggesting additional issues

#### Long Message Selection Failure (NEW ROOT CAUSE)
**Discovery through debug logging**:
- Positions ARE extracted correctly (2673 positions for long message)
- "Detailed Troubleshooting" at byte offset 877-881
- "brew install openssl" at byte offset 402-406
- Hit test reports: "No positions found on line at y=784.0"

**Root Cause**: Y-coordinate mismatch
1. Positions stored with viewport/window y-coordinates at extraction time
2. When item scrolls or user clicks far down, stored y-coordinates invalid
3. Hit test checks y=784 but no positions have that y-value
4. System falls back to wrong position (earlier code block)

#### Debug Logging Added in Session 19
```rust
// In activity_log_positions.rs
log::warn!("⚠️ FALLBACK PATH: Failed to find activity item {} in computed element", index);
log::debug!("🔲 FOUND CODE BLOCK at child {} with cumulative offset {}", i, cumulative_byte_offset);
log::error!("⚠️ CORRUPTION: Byte offset went backwards! {} → {}", old_offset, cumulative_byte_offset);

// In ai_sidebar.rs - Hit test debugging
log::debug!("Hit testing in element with {} positions, byte range {} to {}", ...);
log::debug!("Checking {} children for hit test", position_tree.children.len());
log::debug!("No positions found on line at y={:.1}", point.y);
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

### Remaining Secondary Issues to Address

#### 1. Remaining Cluster Offset Issues
The cluster fix helped but didn't fully resolve offsets. Investigation needed:
- First Heading: 1 char right offset of cmd-c selection may be due to markdown processing
- Code blocks: 2 char left offset of cmd-c selection suggests over-correction or padding issue
- Consider: The cluster offset might need to account for styled text differently

### Critical Debugging Information

#### How to Debug the UIItem Clamping Issue:
```bash
# Watch viewport_y values to see clamping at -577:
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "Storing item positions|viewport_y"

# See the mismatch between click position and stored positions:
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "Detailed Troubleshooting|click y:|Position y-range"

# Track coordinate transformations:
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "Coordinate transform|Item.*viewport pos"
```

#### Key Observations:
1. **viewport_y stops at -577** even when scrolled much further
2. **"Detailed Troubleshooting" at y=1515** in item coordinates (correct)
3. **Click at y=740** looks for positions at that y-value (correct)
4. **But the visual position doesn't match** because viewport_y is wrong

### The Virtual Scrolling System (Working Correctly)

The virtual scrolling correctly tracks positions with:
- `margin_top = -self.activity_log_scroll_offset + y_offset_before_visible`
- `y_offset_before_visible` = cumulative height of items before first visible

This system works perfectly for rendering, but the position storage uses the clamped UIItem values.

### Why the Attempted Fix Failed

The Session 20 fix attempted to use `calculate_item_top_position` to compute the correct viewport_y when UIItem.y was clamped. However, the fix still resulted in the same behavior. Possible reasons:
1. The calculation might not match exactly how virtual scrolling positions items
2. The line height and width defaults (20.0, 400.0) might not match actual values
3. The logic to detect when clamping occurred (`if ui_item.y == 0`) might be insufficient

#### 2. Code Block Vertical Alignment
Rectangle renders 1/2 line too high:
- Check `CODE_LINE_HEIGHT` constant vs actual rendering
- May need to adjust y-position calculation in position extraction
- Verify line height matches between extraction and rendering
- Verify margin and padding are accounted for

#### 3. Selection Rectangle Scrolling
Once coordinate issue fixed, scrolling should work. If not:
- Ensure selection rectangles use viewport coordinates for rendering
- But position data uses item-relative for storage

### Implementation Guide for Next Engineer

#### Remaining Issues to Fix

1. **Heading Selection Offset (1 char right)**:
   - Visual rectangle is correct but cmd-c copies 1 character off
   - Likely issue in cluster-to-byte mapping for styled text
   - Check `shape_text_to_cells_with_offset` cluster adjustment logic
   - May need to account for markdown processing differences

2. **Code Block Alignment (1/2 line high)**:
   - Selection rectangle renders slightly above the text
   - Check `CODE_LINE_HEIGHT` constant vs actual rendered line height
   - Verify padding/margin calculations in position extraction
   - Compare with how code blocks are rendered in `markdown.rs`

3. **Code Block Selection Offset (2 chars left)**:
   - Selection extracts wrong text from code blocks
   - May be over-correcting cluster offsets
   - Check if code blocks have different cluster numbering pattern

4. **Command Item Selection**:
   - Both visual and text selection misaligned
   - Check if command items use different coordinate system
   - Verify UIItemType assignment for command output

#### Debugging Approach

```rust
// Add targeted logging for specific issues:

// For heading offset:
log::debug!("Heading cluster: orig={}, adjusted={}, byte={}", 
    original_cluster, adjusted_cluster, byte_offset);

// For code block alignment:
log::debug!("Code block y: line_y={}, offset.y={}, padding={}", 
    line_y, offset.y, computed.padding.origin.y);

// For command items:
log::debug!("Command item bounds: {:?}, UIType: {:?}", 
    computed.bounds, computed.item_type);
```

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

1. **Character offset mystery**: Caused by `**` markers on each side of bold text being stripped during rendering
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

1. ~~**Y-Coordinate Storage Bug**: Positions use viewport coordinates, breaking selection for scrolled content~~ **FIXED in Session 21**
2. **Partial Cluster Fix**: Styled segments now have continuous clusters but small offsets remain (1-2 chars)
3. **No Markdown in Clipboard**: Users copy plain text, not markdown formatting (by design - WYSIWYG approach)
4. **Command Output Selection**: Still misaligned (not addressed)
5. **Position Tree Structure**: All positions stored flat (0 children) instead of hierarchical
6. **Double Tree Walking**: Currently walks ComputedElement tree twice - once for UIItems, once for unclamped bounds (minor performance impact)

### Future Work

1. **Complete Markdown Support**: Add nested lists, blockquotes
2. **Hierarchical Position Tree**: Store positions in proper tree structure with children
3. **Performance Optimization**: Cache position data to avoid re-extraction
4. **Code Organization**: Extract selection module as planned for better maintainability
5. **Testing**: Add unit tests for coordinate transformations and position extraction

