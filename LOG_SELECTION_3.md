# Text Selection System - Implementation Guide v3

## Overview

This document tracks the implementation and debugging of the text selection system in WezTerm's AI sidebar activity log. The system uses HarfBuzz cluster information for pixel-perfect text selection with a 3-tier coordinate system (Window → Viewport → Item).

## Current Status (Session 28)

### ✅ TEXT SELECTION FULLY FUNCTIONAL

**Session 28 Major Achievement**:
Successfully fixed ALL text selection offset issues. The system now works perfectly for user messages, AI messages, headings, and all markdown elements. Selection copies exactly what is visually selected.

**Key Fixes Implemented**:

1. **Implemented Fix 3 - Correct Position Tracking** 
   - **Root Cause Found**: Position extraction was using `wrapped_line.byte_offset` (position in original unwrapped text) plus `global_offset` (position in markdown), creating nonsense positions
   - **Solution**: Track actual position in `rendered_text` as it's built
   - `activity_log_positions.rs` line 460: Store `rendered_text.len()` before adding each line
   - Line 509: Use ONLY `line_start_in_rendered` without adding irrelevant offsets
   - Line 1042: Calculate position as `cluster + byte_offset_adjustment` only

2. **Fixed Markdown Element Offset Accumulation**
   - **Problem**: Each markdown element (paragraph, heading, code block) was getting its own position tracking starting from 0, but they all contribute to the same AI message text
   - **Solution**: Use cumulative position tracking across all elements
   - Removed incorrect addition of `global_offset` which was for markdown positions, not rendered text positions

3. **Production Code Cleanup**
   - Replaced all `eprintln!` debug statements with `log::debug!`
   - Added bounds checking to prevent panics on string slicing (line 172-180 in `ai_sidebar.rs`)
   - Removed unused variables and misleading comments

### Current Results (After Session 28)

#### All Text Selection ✅ FULLY WORKING
- **User Messages**: ✅ Perfect - selection and copy match exactly
- **AI First Paragraph**: ✅ Perfect - including inline bold/italic
- **AI Headings**: ✅ Perfect - exact selection and copy
- **Lines After Headings**: ✅ Perfect - including italic text
- **Multi-paragraph Selection**: ✅ Perfect - works across elements
- **Code Blocks**: 🟡 Text selection works well with some hit-testing and rendered rectangle mis-alignment

### Known Issues (Minor)

#### Code Block Selection Rectangle Rendering 🟡
- **Issue**: Hit testing and selection rectangle in code blocks is offset:
  - Horizontally: ~1 character too far left
  - Vertically: ~5/8 of a line too high
- **Cause**: positioning likely not accounting for code block margin/padding
- **Impact**: Hit testing and text selection are offset: user has to click and drag above and to the left of desired content, and rectangle renders offset.
- **Next Steps**: Adjust to account for code block's content offset

### Architecture Evolution (Session 28)

**The Coordinate System Journey**:
1. **Session 25-26**: Attempted "Universal Global Offsets" but created more bugs
2. **Session 27**: Pivoted to item-relative offsets, reduced but didn't eliminate issues
3. **Session 28**: Discovered the real problem - mixing coordinate systems:
   - `wrapped_line.byte_offset`: Position in original unwrapped text
   - `global_offset`: Position in original markdown text
   - `rendered_text`: The actual text being built for selection
   - **Solution**: Use ONLY positions relative to `rendered_text`

**Key Learning**: The position tree must use the same coordinate system as the text it's selecting from. Mixing original text positions with rendered text positions creates progressive offsets.

### Next Session Action Items

1. **Fix Code Block Selection Positioning**
   - Investigate how code blocks set padding/margin
   - Adjust selection hit testing and rectangle to account for content offset
   - Test with various code block sizes and positions

### Technical Details for Next Engineer

**Code Block Rectangle Issue Investigation Starting Points**:
1. Check `markdown.rs` for how code blocks set padding/margin
2. Look at `sidebar_render.rs` for how selection rectangles are calculated
3. The rectangle needs to account for the same offset that positions the code text
4. Compare how regular paragraphs vs code blocks handle content positioning

**Key Files Modified in Session 28**:
- `activity_log_positions.rs`: Lines 460-514 (Fix 3 implementation)
- `ai_sidebar.rs`: Lines 163-180 (bounds checking), debug logging cleanup
- Both files had extensive debug statement cleanup

## Previous Session History

### Session 27: Identified Root Cause of LEFT Offset

**Key Discovery**:
Found that byte positions in the position tree were ahead of actual character positions, causing a consistent LEFT offset when copying text (2-7 characters).

**Investigation and Findings**:
1. **Added comprehensive debug logging** to track exact byte offsets during selection
2. **Confirmed the offset pattern**:
   - User messages: 2 chars LEFT
   - AI first paragraph: 3 chars LEFT  
   - AI headings: 7 chars LEFT
   - Progressive offset that got worse deeper in document
3. **Identified mixing of coordinate systems** as root cause:
   - `wrapped_line.byte_offset` was position in original unwrapped text
   - But positions needed to be relative to rendered wrapped text with newlines

**Changes Made**:
1. Reverted Session 26's breaking change to restore markdown element offsets
2. Changed user messages to use item-relative offsets (`global_byte_offset(0)`)
3. Updated to use `get_item_text_for_selection_with_positions` for stored rendered text
4. Added extensive debug logging to diagnose the offset issue

**Status After Session 27**:
- User messages had 2-char LEFT offset (improved from 30-40 char RIGHT offset)
- AI messages had 3-7 char LEFT offset but all paragraphs were selectable
- Set the stage for Session 28's complete fix

### Session 26: Comprehensive Fix Implementation

### MAJOR IMPLEMENTATION: Comprehensive Fix for Text Selection System

**What Was Implemented**:
Building on Session 25's Option 3 (Universal Global Offsets), implemented comprehensive fixes for all identified bugs with proper architectural solutions.

**Changes Made**:
1. **Fixed Code Block Syntax Highlighting** (`markdown.rs` line 1091)
   - Removed unnecessary `.clone()` on `combined_text` 
   - Saved text length before move to fix borrow checker issue
   - ✅ Syntax colors now properly display in code blocks

2. **Fixed User Message Double-Offset** (Coordinated changes)
   - `box_model.rs` lines 1494, 2031: Changed `wrap_text_into_lines` and `wrap_text_with_estimates` to start at `byte_offset = 0` (local offsets)
   - Added `global_byte_offset` field to `ComputedElement` struct for proper offset tracking
   - Updated all ComputedElement construction sites to include global_byte_offset
   - `activity_log_positions.rs`: Updated to use global offset from ComputedElement when available

3. **Enhanced Text Selection Accuracy**
   - Modified position extraction to capture `shaped_text` from WrappedLines (the actual rendered text)
   - Added `rendered_text: String` field to `ItemPositionData` 
   - Updated `get_item_text_for_selection` to use stored rendered text when available
   - Created new extraction functions that return both PositionTree and rendered text

### Current Results (After Session 26 Implementation)

#### User Messages ⚠️ IMPROVED FROM SESSION 25 BUT OFFSET
- **Visual**: ✅ Selection rectangle renders correctly where dragged
- **Cmd-C**: ❌ Still copies text ~30 characters too far RIGHT from selection
- **Status**: Double-offset issue partially fixed but offset persists
- **Suspected Cause**: Global offset may not be properly propagated through all elements

#### AI Messages - Mostly Working with Consistent Left Offset
- **First paragraph**:
  - ✅ Visual selection correct
  - ✅ Inline bold renders properly
  - ❌ Cmd-C copies 3 chars too far LEFT
  
- **First heading** ("Quick Solution"):
  - ✅ Visual selection correct
  - ❌ Cmd-C copies 6 chars too far LEFT
  
- **Lines after heading**:
  - Line 1: ✅ Visual correct, ❌ Cmd-C 5 chars LEFT
  - Line 2: ✅ Visual correct, ❌ Cmd-C 6 chars LEFT
  - Line 3 ("First, check if..."): ❌ Can't select first 2 chars, ✅ Italics render, ❌ Cmd-C 5 chars LEFT
  
- **Later headings**: 
  - ✅ Visual correct
  - ❌ Cmd-C ~6 chars too far LEFT

#### Code Blocks ✅ 
- **Syntax highlighting FIXED**: Colors now visible and working properly like before session 25
- Selection behavior not quite correct (mostly un-tested), but out of scope for testing for now

### Known Bugs After Session 26

1. **User Message Right Offset Bug** 🔴 CRITICAL
   - **Symptom**: Cmd-C copies text ~30 characters too far RIGHT from visual selection
   - **Status**: Partially addressed but not fully fixed
   - **Root Cause Analysis Needed**: 
     - The wrapping functions now use local offsets (starting at 0)
     - ComputedElement has global_byte_offset field
     - But the offset is still wrong by ~30 chars (suspiciously close to command item (mock item 0) length)
   - **Investigation Required**: 
     - Check if first activity item (command) has text that affects subsequent items
     - Verify global_byte_offset is correctly set on user message Elements
     - Trace through position extraction to see where the 30-char offset originates

2. **AI Message Consistent Left Offset Bug** 🟡 PATTERN IDENTIFIED
   - **Symptom**: All AI message selections copy text 3-6 characters too far LEFT
   - **Pattern**: Consistent offset suggests systematic issue, not random
   - **Likely Cause**: 
     - Markdown rendering adds separators/formatting that affects byte positions
     - The stored `rendered_text` may not perfectly match what position extraction sees
     - Possible newline/separator counting mismatch
   - **Evidence**: First paragraph (3 chars), headings (6 chars), regular lines (5-6 chars)

3. **Line 3 Selection Dead Zone** 🟡
   - **Symptom**: Can't select first 2 characters in third line after heading
   - **Context**: This line contains italic text inline (*text*)
   - **Potential Cause**: Style span adjustment or cluster offset issue with italics

### Architectural Deviations from Session 25 Plan

1. **Added `global_byte_offset` to ComputedElement** ✅
   - **Why**: Needed to pass global offset from Element through to position extraction
   - **Impact**: Clean architectural solution, no workarounds
   - **Implementation**: Added field and updated all ComputedElement construction sites

2. **Stored Rendered Text in ItemPositionData** ✅
   - **Why**: Cannot accurately recreate rendered text from markdown
   - **Impact**: Ensures we use exact text that positions were calculated from
   - **Implementation**: Captures `shaped_text` from WrappedLines during extraction

### Key Learnings from Session 26

1. **WrappedLine.shaped_text is the source of truth** 
   - This contains the EXACT text that was shaped and rendered
   - Using this eliminates markdown-to-rendered text conversion errors

2. **Global offset propagation is complex**
   - Element → ComputedElement → Position extraction requires careful tracking
   - Any break in the chain causes offset errors

3. **Consistent offset patterns indicate systematic issues**
   - The 3-6 char LEFT offset in AI messages suggests a counting mismatch
   - Likely related to markdown element separators or newline handling

## Prior Session Reference (Session 25)

### What Was Attempted (Option 3 - Universal Global Offsets)
Converted entire system to use global (document-relative) byte offsets, eliminating the dual coordinate system that was causing progressive offset bugs.

### Session 25 Changes Made:
1. **Phase 1**: Fixed separator byte addition logic to not interfere with global offset elements (`activity_log_positions.rs` lines 539-546)
2. **Phase 2**: Added global byte offsets to all code block line elements (`markdown.rs` lines 1013-1180)
3. **Phase 3+4**: Tracked document-wide positions and added global offsets to all activity items (`ai_sidebar.rs` lines 2390-2437)
4. **Phase 5**: Removed detection logic and simplified position extraction (`activity_log_positions.rs` lines 401-470)

### Session 25 Results:

#### User Messages ❌ BROKEN (WORKED IN LAST COMMIT)
- **Visual**: Selection rectangle rendered correctly where dragged
- **Cmd-C**: Copied text 30-40 characters too far RIGHT from selection
- **Root Cause Identified**: `global_byte_offset` was being double-counted - once on the Element, and again added to internal WrappedLine byte_offsets
- **Log Evidence**: User message at index 1 had `global_byte_offset=47` and first line showed `byte_offset=47` (should be 0)

#### AI Messages - Partial Success
- **First paragraph**: ✅ Working correctly with inline bold
- **First heading**: ✅ Working correctly  
- **Lines after heading**: ✅ First two lines work
- **Third line issues**:
  - Line has inline italics
  - Can't select first 2 characters
  - Cmd-C copies 4 chars too far LEFT
- **Later headings**: Visual correct, cmd-c ~15 chars LEFT offset
- **Root Cause Suspected**: Mismatch between markdown text (used for offset calculation) and rendered text (used for display)

#### Code Blocks ❌ 
- **Syntax highlighting BROKEN**: Colors missing from code
- **Root Cause Identified**: Line 1091 in `markdown.rs` incorrectly used `combined_text.clone()` inside loop, giving each line only partial text

### Session 25 Key Problems Identified:

1. **User Message Double-Offset Bug** 
   - Setting `global_byte_offset` on user message Elements caused double-counting
   - The offset got added both to the Element AND to internal WrappedLine offsets
   - Needed to ensure global_byte_offset is only used for document position, not internal offsets

2. **Code Block Syntax Highlighting Broken**
   - Line 1091: `text: combined_text.clone()` happened inside loop while building `combined_text`
   - Each line element got incomplete text
   - Needed to restructure to build complete text before creating element

3. **AI Message Offset Calculation Mismatch**
   - `get_item_text()` returned raw markdown text
   - Position extraction used rendered text (without markdown syntax)
   - Byte offset calculations didn't match displayed content
   - Needed to use rendered text for offset calculations

### Session 25 Debug Log Evidence
```
13:07:03.811  DEBUG  ai_sidebar > Rendering item at filtered index 1 = original index 1, global_byte_offset=47
13:07:03.812  DEBUG  box_model::WrappedText > wrapped line 0: byte_offset=47, text='I'm trying to resolve an SSL issue with'
```
This showed the double-counting issue - wrapped line byte_offset should have been 0, not 47.

### What Didn't Work in Session 25
1. **Simply setting global_byte_offset on Elements** - Led to double-counting
2. **Using markdown text for selection** - Didn't match rendered text byte positions
3. **Relying on cumulative offset tracking** - Complex and error-prone

## Session 23-24 Implementation Details

### What Was Implemented

1. **Global Byte Offsets (Session 23)** ✅
   - Added `global_byte_offset: Option<usize>` to Element struct
   - Modified markdown renderer to track and set global offsets on all elements
   - Updated `wrap_text_with_info` and `wrap_styled_text` to accept global offsets
   - Extended to code blocks via `highlight_code_block`

2. **Text Wrapping Fixes (Session 24)** ✅
   - Fixed runtime crash in `wrap_text_with_estimates` (line 2122)
   - Restored original text extraction for style spans (fixed italic rendering)
   - Re-enabled cluster offset adjustment for styled segments
   - Fixed list marker byte counting (dynamic instead of hard-coded 3)

3. **Position Extraction Updates (Session 24)** ⚠️ Partially Working
   - Attempted to detect global vs local offsets
   - Current detection logic is fundamentally flawed
   - Mixed global/local offset handling causes progressive drift

### Key Code Changes

**box_model.rs**:
- Lines 2197-2238: Global offset detection and text extraction
- Lines 1900-1902: Re-enabled cluster adjustment
- Line 684: Added `global_byte_offset` field to Element

**markdown.rs**:
- Lines 399-415: Track global offsets for paragraphs
- Lines 424-484: Set global offsets for headings
- Lines 506-512: Set global offsets for code blocks
- Lines 600-607: Dynamic list marker length calculation
- Lines 947-958: Added global_byte_offset parameter to highlight_code_block

**activity_log_positions.rs**:
- Lines 415-424: Flawed global offset detection logic
- Lines 456-460: Conditional cumulative offset updates
- Lines 966-972: Byte offset calculation with adjustments

## Resolution Plan for Remaining Bugs (For Session 28)

### Fix 1: LEFT Offset Bug (2-7 characters)

**Problem**: All selections copy text slightly too far LEFT with consistent pattern
- User messages: 2 chars LEFT
- AI first paragraph: 3 chars LEFT
- AI first heading: 7 chars LEFT
- Initial AI lines after headings: 7 chars LEFT
- Heading later in the item: 15 chars LEFT

**Status After Session 27**: 
- ⚠️ LEFT offset
- ✅ Using correct stored `rendered_text` 
- ✅ Positions are available and being used
- ❌ Small to medium sized offset remains

**Root Cause Hypothesis**:
The byte positions in the position tree are slightly ahead of actual character positions. This consistent LEFT offset suggests positions are being calculated with an offset.

**Investigation Steps**:
1. **Add Hit Test Logging** (partially done in Session 27)
   - Log exact byte offsets returned during selection drag
   - Compare with actual character positions in rendered_text
   - Look for systematic offset pattern

2. **Trace Position Extraction**
   - Check if cluster-to-byte mapping has off-by-one error
   - Verify wrapped line byte_offset calculations
   - Look for separator/newline counting differences

3. **Check Text Assembly**
   - Verify rendered_text assembly matches position calculation
   - Check if newlines between elements are counted consistently

**Solution Approaches**:
- Fix the root cause in position extraction or hit test calculation
- May need to adjust how `wrapped_line.byte_offset` is used

### Fix 2: Italic Line Dead Zone (First 2 chars)

**Problem**: Can't select first 2 characters in the line containing italic text shortly after the first heading ("First, *check* if OpenSSL...")

**Status After Session 27**:
- ⚠️ Still present but possibly related to general LEFT offset issue
- Most of the line is selectable

**Root Cause Hypothesis**:
Related to the general LEFT offset - if positions are off by 2-3 chars, the first positions might be missing or invalid.

**Solution**:
- Should be fixed when Fix 1 is resolved
- If not, investigate cluster numbering for styled text segments

### Testing Approach

Run with debug logging:
```bash
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "(global_byte_offset|byte_offset=|🌍)"
```

Verify:
- User messages: First line should have `byte_offset=0` internally
- Code blocks: Should show syntax colors
- AI messages: Selection should match visual exactly

### Architecture Evolution

**Session 25-26: Attempted "Universal Global Offsets" (Option 3)**
- Tried to use document-relative positions (user message at 47, AI at 267, etc.)
- Goal was to eliminate dual coordinate systems
- Result: Created MORE bugs - user messages had 30-40 char RIGHT offset

**Session 27: Pivoted to Item-Relative Offsets**
- Changed to item-relative positions (each item starts at 0)
- Within each item, markdown elements use offsets relative to item start
- Result: improvement back to status before session 25 - 2-7 char LEFT offset remains

**Current Architecture (Item-Relative)**:
- Each activity item's positions start at 0 ✅
- Markdown elements within items use item-relative offsets ✅
- No complex detection logic needed ✅
- Simpler and more maintainable than document-global ✅

The term "global_byte_offset" in the code is misleading - it means "global within the item" not "global to document". This should probably be fixed.

## Known Issues & Current State

1. **Debug logging still active**: Multiple log statements added for debugging remain in code
2. **No cleanup done**: Session ended before removing debug statements
3. **Partial implementation**: Option 3 architecture is correct but has implementation bugs
4. **Breaking change**: User message selection is now broken (was working before Option 3)

## Historical Context (Condensed)

### Key Learnings
- **Session 16-18**: Discovered markdown strips formatting, positions need plain text extraction
- **Session 19-20**: Fixed long message selection via unclamped bounds tracking
- **Session 21**: Two-part fix for clamped bounds affecting both position extraction and selection rectangles
- **Session 22**: Identified each markdown element creates separate Element with local WrappedLines
- **Session 23**: Implemented global byte offsets for markdown elements
- **Session 24**: Fixed rendering issues but progressive offset bug remains due to flawed detection
- **Session 25**: Implemented Option 3 (universal global offsets) but introduced new bugs in the process

### Architecture Decisions
- **3-tier coordinate system**: Window → Viewport → Item (stable and working)
- **Item-relative byte offsets**: Each item's positions start at 0 (not document-relative)
- **Cluster tracking**: HarfBuzz clusters provide character-level position data
- **Position extraction**: Happens during rendering, stored in PositionTree with rendered_text

## Refactoring Plan (after bugs are fixed)

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

### Future Work

1. **Complete Markdown Support**: Add nested lists, blockquotes
2. **Hierarchical Position Tree**: Store positions in proper tree structure with children
3. **Performance Optimization**: Cache position data to avoid re-extraction
4. **Code Organization**: Extract selection module as planned for better maintainability
5. **Testing**: Add unit tests for coordinate transformations and position extraction