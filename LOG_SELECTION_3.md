# Text Selection System - Implementation Guide v3

## Overview

This document tracks the implementation and debugging of the text selection system in WezTerm's AI sidebar activity log. The system uses HarfBuzz cluster information for pixel-perfect text selection with a 3-tier coordinate system (Window → Viewport → Item).

**STATUS: ✅ FULLY FUNCTIONAL** - All known issues have been resolved as of Session 31. The text selection system now works perfectly across all element types, with proper visual feedback and clean text copying.

## Current Status (Session 31)

### Session 31 Major Achievements

1. **FIXED Multi-Element Selection Visual Bug** ✅ COMPLETE
   - **Root Cause**: Line indices were reset to 0 for each markdown element (paragraph, heading, code block)
   - **Solution**: Implemented global_line_index tracking across all elements
   - **Implementation**: Added `global_line_index` parameter to position extraction functions
   - **Result**: Selection rectangles now properly display across all selected lines
   - **Key Insight**: The issue wasn't child elements in the tree, but incorrect line indexing

2. **Improved Selection Rectangle Calculation** ✅
   - **Problem**: Only positions strictly within byte range were included
   - **Solution**: Enhanced `calculate_line_selection_rect` to handle partial line selections
   - **Result**: Proper selection rectangles for lines that partially overlap selection range

3. **Clean Architecture Implementation** ✅
   - Added `calculate_local_selection_rectangles` method for non-recursive selection
   - Kept existing `calculate_selection_rectangles` for backward compatibility
   - Clear separation of concerns with well-documented methods
   - Added `collect_all_text_positions` for future extensibility

4. **Code Quality Improvements** ✅
   - Removed all SELECTION_DEBUG logging
   - Cleaned up commented-out `builder.start_element()` calls
   - Added bounds checking to `get_selection_text()`
   - Removed verbose debug logging from hot paths

### Current Working Features

**Text Selection Functionality**:
- **Single Element Selection**: ✅ Perfect visual and copy
- **Multi-Element Selection Within Item**: ✅ FIXED - Works across paragraphs, headings, code blocks
- **Multi-Item Selection**: ✅ Works across different activity items
- **Wrapped Text**: ✅ No artificial newlines in copied text
- **Syntax Highlighting**: ✅ Consistent colors, no longer changes on click
- **Code Block Multi-Line Selection**: ✅ FIXED - Works across multiple lines
- **Partial Line Selection**: ✅ Handles selections starting/ending mid-line

### Known Issues

None! All previously identified issues have been resolved:
- ✅ Multi-element selection visual bug - FIXED with global line indexing
- ✅ Code block multi-line selection - FIXED as part of multi-element fix
- ✅ Artificial newlines in copied text - FIXED with wrap_newlines tracking
- ✅ Syntax highlighting color changes - FIXED with API consolidation

### Key Fixes Implemented in Session 29:

#### 1. Code Block Selection Fix ✅
- **Root Cause**: Code blocks have 12px padding and 8px top margin not accounted for in position extraction
- **Solution**: 
  - Added `apply_code_block_offset_adjustment()` helper function
  - Adjusts both horizontal (12px) and vertical (12px + 8px) offsets
  - Applied in both processing branches (mixed content and pure nested)
- **Files Modified**: 
  - `activity_log_positions.rs`: Lines 291-298 (helper function), 673-707 (Branch 1), 784-865 (Branch 2)
  - `sidebar_constants.rs`: Added `CODE_BLOCK_TOP_MARGIN` constant

#### 2. Processing Branch Architecture Clarified
- **Branch 1 (Mixed Content)**: Handles elements with both text AND nested children (e.g., code blocks with copy buttons)
- **Branch 2 (Pure Nested)**: Handles elements with only nested Children elements
- Added clear documentation explaining when each branch is used

#### 3. Comprehensive Logging Cleanup

### Current Results (After Session 31)

#### All Text Selection ✅ FULLY ALIGNED
- **User Messages**: ✅ Perfect
- **AI Messages**: ✅ Perfect - all markdown elements
- **Headings**: ✅ Perfect - all levels
- **Bold/Italic/Code**: ✅ Perfect inline formatting
- **Multi-paragraph**: ✅ Perfect across elements
- **Code Blocks**: ✅ FIXED alignment
- **Lists**: ✅ Perfect - ordered and unordered
- **Multi-line Selection**: ✅ Perfect - works across all wrapped lines and code blocks without artificial newlines

## How We Fixed Multi-Element Selection (Session 31)

### The Real Root Cause

Initially we thought the issue was child elements in the position tree, but the actual problem was much simpler: **line indices were being reset for each markdown element**.

When processing a document with multiple paragraphs, headings, and code blocks, each element would enumerate its lines starting from 0. This caused all text positions to be grouped into only a few line indices (0-8), making selection rectangles collapse incorrectly.

### The Solution: Global Line Index Tracking

**Implementation**:
```rust
// Added global line counter that persists across all elements
let mut global_line_index = 0usize;

// Pass it through all extraction functions
extract_positions_recursively_with_text_and_wraps(
    computed, builder, offset, fonts,
    &mut cumulative_byte_offset,
    &mut rendered_text,
    &mut wrap_newlines,
    &mut global_line_index,  // NEW: Global counter
);

// Increment after each line, regardless of element boundaries
*global_line_index += 1;
```

This ensures every visual line gets a unique index, allowing proper selection rectangle generation.

### Additional Improvements

1. **Better Partial Line Selection**: Enhanced `calculate_line_selection_rect` to handle selections that start or end mid-line
2. **Clean Architecture**: Added `calculate_local_selection_rectangles` for non-recursive selection calculation
3. **Performance**: Removed debug logging from hot paths

## Code Changes Made in Session 31

### 1. Global Line Index Implementation

**Files Modified**:
- `activity_log_positions.rs`:
  - Added `global_line_index: &mut usize` parameter to `extract_positions_recursively_with_text_and_wraps`
  - Changed all `line_index` usages to `local_line_index` for element-local counting
  - Use `*global_line_index` for position extraction and increment after each line
  - Updated all recursive calls to pass the global index through

**Key Changes**:
```rust
// Before: Each element reset line_index to 0
for (line_index, line) in lines.iter().enumerate() {
    extract_positions(..., line_index, ...);
}

// After: Global counter across all elements
for (local_line_index, line) in lines.iter().enumerate() {
    extract_positions(..., *global_line_index, ...);
    *global_line_index += 1;
}
```

### 2. Selection Rectangle Calculation Improvements

**Files Modified**:
- `position_cache.rs`:
  - Added `calculate_local_selection_rectangles()` method for non-recursive selection
  - Enhanced `calculate_line_selection_rect()` to handle partial line selections
  - Added proper bounds checking to `get_selection_text()`
  - Added `collect_all_text_positions()` for future extensibility

**Key Improvements**:
- Handles lines that partially overlap selection range
- Checks if line has any overlap before processing
- Properly calculates selection start/end within each line

### 3. Clean Architecture Changes

**Files Modified**:
- `ai_sidebar.rs`:
  - Updated to use `calculate_local_selection_rectangles` instead of recursive version
  - Removed all SELECTION_DEBUG logging
  - Cleaned up empty else blocks

### 4. Code Quality Cleanup

**Files Modified**:
- Multiple files had debug logging removed or converted to trace level
- Removed commented-out code that was no longer needed
- Added bounds checking for safety

## Code Changes Made in Session 30 (Historical)

### 1. Artificial Newline Fix (Solution B)

**Files Modified**:
- `position_cache.rs`:
  - Added `wrap_newlines: HashSet<usize>` to `ItemPositionData` struct
  - Added `get_selection_text()` method to skip artificial newlines when extracting text
  
- `activity_log_positions.rs`:
  - Modified to track wrap newlines during position extraction (line 517)
  - Added `wrap_newlines` parameter to extraction functions
  - Updated `store_activity_item_positions_with_wraps` to accept wrap_newlines

- `ai_sidebar.rs`:
  - Updated text extraction to use `get_selection_text()` instead of raw substring
  - Modified both single-item and multi-item selection cases

### 2. Syntax Highlighting Fix

**Files Modified**:
- `markdown.rs`:
  - Consolidated `render_with_fonts`, `render_with_fonts_and_registry`, and `render_with_fonts_registry_and_palette` into single function
  - Single `render_with_fonts` now accepts optional registry, context, and palette parameters
  - Marked old functions as deprecated (they delegate to new function)

- `ai_sidebar.rs`:
  - Updated all 5 call sites to use new consolidated API
  - Always passes palette when available (lines 2082-2089, 2106-2112, 2153-2159)
  - Fixed issue where selection state caused palette to be omitted

- `suggestion_modal.rs`:
  - Updated to use new API (palette not available in modal context, passes None)

### 3. Multi-Element Selection Attempt (Incomplete)

**Files Modified**:
- `activity_log_positions.rs`:
  - Commented out `builder.start_element()` calls for headings (lines 790-803)
  - Commented out `builder.start_element()` calls for code blocks (lines 825-838)
  - Attempted to flatten position tree structure

- `ai_sidebar.rs`:
  - Implemented multi-item selection rendering (lines 3743-3802)
  - Fixed TODO for rendering selection across different activity items

**Issue**: Child elements still exist from recursive processing, causing only last element to show selection

## Deviations from Original Plan (Session 31)

1. **Simpler Solution Than Expected**: Instead of implementing the complex "hybrid approach" with flat mode flags, we discovered the real issue was just line index resetting. The simpler global_line_index solution was more elegant and less invasive.

2. **No PositionTreeBuilder Changes Needed**: We initially planned to add a `flat_mode` flag to PositionTreeBuilder, but this wasn't necessary once we fixed the line indexing issue.

3. **Better Architecture Without Structural Changes**: Rather than modifying the tree structure, we added clean separation with `calculate_local_selection_rectangles` vs `calculate_selection_rectangles` methods.

## Deviations from Original Plan (Session 30 - Historical)

1. **Solution B Instead of A**: Chose to track wrap newlines separately rather than modifying position extraction logic extensively
2. **API Consolidation**: Went beyond fixing the bug to clean up poor API design with 3 similar functions
3. **Multi-Element Selection**: Initial attempt was incomplete - fixed properly in Session 31

## Key Learnings from Session 31

1. **Debug the Right Thing**: We spent time thinking about tree structure when the real issue was much simpler - line indices resetting. Always verify assumptions with logging.

2. **Global State Can Be Simple**: The global_line_index solution is straightforward and effective - sometimes a simple counter is all you need.

3. **Line-Based Selection Is Fundamental**: The selection system fundamentally works on lines, so getting line indices right is critical for proper rectangle generation.

4. **Clean Architecture Pays Off**: Adding separate methods (`calculate_local_selection_rectangles`) rather than boolean flags makes the code more maintainable and clear.

## Key Learnings from Session 30 (Historical)

1. **Wrap Newlines Are Predictable**: They occur between wrapped lines within the same paragraph/element, making them easy to track and filter
2. **API Design Matters**: Having 3 functions doing similar things with slight variations led to the palette bug
3. **Position Tree Structure Is Complex**: Even when not explicitly creating elements, recursive processing still creates child elements
4. **Selection vs Rendering Needs Differ**: Selection wants flat positions, rendering wants structure - these should be separated

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

## Future Optimization Opportunities

The text selection system is now fully functional and production-ready. These are optional improvements for future consideration:

### Performance Enhancements
1. **Position Tree Caching**: Cache position trees for unchanged content to avoid recalculation
2. **Memory Management**: Implement position tree cleanup for off-screen items
3. **Lazy Evaluation**: Only calculate positions for visible items until selection starts

### Code Organization
1. **Selection Module**: Extract selection logic from `ai_sidebar.rs` into dedicated module (see refactoring plan in previous sessions)
2. **Constants Consolidation**: Remove remaining magic numbers for line heights and spacing
3. **Type Safety**: Consider stronger typing for byte offsets vs character indices

### Testing & Documentation
1. **Unit Tests**: Add comprehensive tests for:
   - Line selection edge cases
   - Partial line selection
   - Multi-element selection
   - Artificial newline filtering
2. **Integration Tests**: Test selection across different markdown element combinations
3. **Performance Benchmarks**: Measure selection performance with large documents

## Previous Session History

### Session 29: Final Selection Alignment & Cleanup

**Major Achievements**:
1. **Fixed code block selection offset issues** - Added proper padding/margin handling
2. **Cleaned up extensive logging** - Removed ~40 debug logs from hot paths
3. **Improved code structure** - Added helper functions and clear documentation

**Key Technical Details**:
- Added `apply_code_block_offset_adjustment()` helper for consistent offset handling
- Code blocks have 12px padding + 8px top margin that needed accounting
- Clarified two processing branches: mixed content vs pure nested elements

### Session 28: Fixed Coordinate System Issues

**Major Achievement**: Fixed ALL text selection offset issues by correcting coordinate system mixing.

**Key Technical Fixes**:
1. **Fix 3 Implementation** (`activity_log_positions.rs` lines 460-514):
   - Tracked actual position in `rendered_text` as it's built
   - Line 460: Store `rendered_text.len()` before adding each line
   - Line 509: Use ONLY `line_start_in_rendered` without adding irrelevant offsets
   - Line 1042: Calculate position as `cluster + byte_offset_adjustment` only

2. **Fixed Markdown Element Accumulation**:
   - Problem: Each element (paragraph, heading, code block) started position tracking from 0
   - Solution: Use cumulative position tracking across all elements in same message
   - Removed incorrect addition of `global_offset` (for markdown positions, not rendered text)

3. **Production Code Cleanup**:
   - Replaced all `eprintln!` debug statements with `log::debug!`
   - Added bounds checking to prevent panics (`ai_sidebar.rs` lines 172-180)
   - Removed unused variables and misleading comments

**Key Learning**: Position extraction was mixing three different coordinate systems:
- `wrapped_line.byte_offset`: Position in original unwrapped text
- `global_offset`: Position in original markdown text  
- `rendered_text`: The actual displayed text for selection
The solution was to use ONLY positions relative to `rendered_text`.

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