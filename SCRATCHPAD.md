# Text Selection Bug - Activity Log Items (Session 28)

## Current Status (CRITICAL - STILL BROKEN)
- Goal text selection works perfectly
- Activity item text selection is completely broken - selection rectangles don't appear at all after latest changes
- Previous issues: selection rectangles appeared 7+ lines below where user clicked/dragged

## Root Cause Analysis

### The Fundamental Problem
Activity items have a complex nested structure that we're not handling correctly:

1. **Structure Mismatch**:
   - Goal text: Single MultilineText element → simple Y coordinate mapping works
   - Activity items: 35+ nested child elements, each with their own MultilineText
   - Each child has lines starting at y=0 relative to that child
   - We're flattening this hierarchy and losing critical position information

2. **Coordinate System Issues**:
   - Line positions are extracted as item-relative (y=0, y=25.8, etc)
   - But child elements have bounds.y values (630, 878, 929, etc) that position them
   - Current code ignores these bounds, causing wrong line selection

3. **Failed Fixes**:
   - Tried not accumulating Y offsets → made selection appear far below click
   - Attempted to fix by keeping base_y_offset=0 → selections disappeared entirely
   - The encoding hack (line_index * 1000 + offset) is fragile and incomplete

## Steps Attempted

1. **Initial Investigation**:
   - Added extensive debug logging
   - Found selection rectangles had huge Y values (e.g., y=14642)
   - Discovered coordinate transformation issues

2. **Coordinate Fixes**:
   - Fixed mouse coordinate conversion from window → sidebar → activity-relative
   - Fixed byte offset calculation from line-relative to absolute
   - Result: X coordinates now work, but Y still broken

3. **Y Position Fixes** (ALL FAILED):
   - Attempt 1: Subtract first line Y from all positions → still wrong
   - Attempt 2: Don't accumulate child.bounds.y → selection 7 lines off
   - Attempt 3: Lead engineer's analysis suggested we need bounds.y → broke completely

## Critical Code Locations

### Position Extraction (sidebar_render.rs:~1810)
```rust
ComputedElementContent::Children(children) => {
    // THIS IS THE PROBLEM AREA
    // We need to use child.bounds.origin.y but previous attempts broke
    for (i, child) in children.iter().enumerate() {
        let child_y_offset = base_y_offset + child.bounds.origin.y; // ← This is needed
        self.extract_positions_recursive(child, line_positions, child_y_offset, line_height);
    }
}
```

### Selection Rectangle Calculation (ai_sidebar.rs:~2891)
```rust
// Currently: let y_in_viewport = bounds.origin.y + line_info.y_position;
// This assumes line_info.y_position is correct, but it's not due to extraction issues
```

## Key Insights

1. **Why Goal Text Works**:
   - Single text block, no nesting
   - Direct position extraction
   - No child bounds to worry about

2. **Why Activity Items Fail**:
   - Complex markdown rendering creates nested structure
   - Each paragraph, code block, etc. is a separate child
   - We're treating all lines as if they're in one text block

3. **The Bounds Problem**:
   - Child bounds (630, 878, etc) represent actual Y positions
   - Ignoring them causes wrong line selection
   - But using them incorrectly breaks selection entirely

## Suggested Next Steps

1. **Proper Structural Fix**:
   - Track which child element each line belongs to
   - Store child bounds with line positions
   - Use a structure like: `{ child_index, child_bounds, line_positions }`

2. **Alternative Approach**:
   - Instead of flattening, keep hierarchical structure
   - Find child by Y coordinate first, then find line within child
   - This matches how the rendering actually works

3. **Debug Strategy**:
   - Add logging to show which child is clicked
   - Compare clicked Y to child bounds
   - Verify position extraction matches visual layout

4. **Consider Reverting**:
   - The encoding hack and line-relative positions made things worse
   - May need to go back to a known working state and try again

## Important Context for Next Engineer

1. **Don't Trust Current Code**:
   - The encoding (line * 1000 + offset) is a hack
   - Line positions are wrong due to flattening
   - Selection only works for single lines within same child

2. **Test Pattern**:
   - Click and drag on different paragraphs
   - Note which visual line vs which line gets selected
   - Check if selection jumps between paragraphs

3. **Key Questions**:
   - Should we preserve element hierarchy during extraction?
   - Can we use the existing UIItem bounds instead?
   - Is there a simpler approach using the render tree?

4. **Warning**:
   - Changes to position extraction affect all text selection
   - Test goal text after any changes to ensure it still works
   - The coordinate systems are confusing - document assumptions

## Recommended Reading
- `dev-docs/text-layout.md` - Text rendering architecture
- `dev-docs/rendering-pipeline.md` - How elements are positioned
- Compare goal text rendering vs activity item rendering in `sidebar_render.rs`

---

# Multi-line Chat Input & Text Selection Implementation

## Project Goals
1. **Focus Management**: Focus defaults to terminal. Only moves to chat input when clicked. Modals steal focus and return it when closed.
2. **Text Selection**: Enable click-and-drag per-character text selection with visual feedback (blue background, white text) in activity log, suggestion card, suggestion "view more" modal, and goal text.
3. **Multi-line Chat Input**: Full editing capabilities with click-to-position cursor, scrolling, Enter to send, Shift+Enter for newline.

## Session 27 - Attempted Cluster Tracking Implementation (SELECTION COMPLETELY BROKEN)

### Current Status:
**Text selection is now completely broken** - no selection rectangles visible, no text can be copied to clipboard.

### What Was Attempted:

1. **Implemented cluster tracking for StyledWrappedText** (box_model.rs):
   - Modified `shape_line_with_styles()` to adjust cluster values after shaping
   - Added calculation of segment offset in original text
   - Created adjusted GlyphInfo objects with corrected cluster values
   - Added `shape_text_to_cells_impl()` to handle original text for grapheme extraction

2. **Fixed activity item drag selection** (ai_sidebar.rs):
   - Changed from `update_selection_drag(0)` to proper byte offset calculation
   - Now uses `find_byte_offset_in_activity_item()` with both X and Y coordinates

3. **Enhanced position extraction for markdown** (sidebar_render.rs):
   - Added support for `Text` content type (single-line elements)
   - Added debug logging to understand nested structure
   - Note: This was a mistake - activity items are MultilineText, not Text

4. **Fixed MultilineText position extraction** (sidebar_render.rs):
   - Changed from cumulative byte offset calculation to direct cluster usage
   - Clusters in MultilineText are absolute positions in original text
   - Removed incorrect accumulation logic

5. **Replaced hardcoded character widths** (ai_sidebar.rs):
   - Changed from hardcoded 8.5px to `font.metrics().cell_width.get() * 0.5`
   - Applied to activity item, suggestion, and goal selection calculations

### Why Selection is Completely Broken:

1. **The cluster tracking implementation may be flawed**:
   - The adjusted clusters might not be correctly calculated
   - The validation against original text might be failing
   - The grapheme extraction from original text might not work as expected

2. **MultilineText position extraction was "fixed" incorrectly**:
   - The assumption that clusters are absolute might be wrong
   - Different text rendering paths may use clusters differently
   - The removal of cumulative offset tracking might have broken line boundaries

3. **Missing position data**:
   - If position extraction fails, no selection rectangles can be rendered
   - The debug logs would show if positions are being extracted
   - Without positions, mouse events can't calculate proper byte offsets

### Critical Issues to Debug:

1. **Check if positions are being extracted at all**:
   - Look for "POSITION_EXTRACT" debug logs
   - Verify that `GlyphWithCluster` cells are being created
   - Ensure the adjusted clusters are valid

2. **Verify selection state management**:
   - Check if selection is being prepared and activated
   - Ensure byte offsets are being calculated correctly
   - Verify that selection rectangles are being calculated

3. **Test the cluster tracking implementation**:
   - The segment offset calculation might be wrong
   - The adjusted clusters might exceed text bounds
   - The original text might not be passed correctly

### Recommended Next Steps:

1. **REVERT the cluster tracking changes** and get back to a working state
2. **Add extensive debug logging** to understand:
   - What type of ElementCells are being created (Glyph vs GlyphWithCluster)
   - What cluster values are being generated
   - Whether position extraction is working
   - What selection state is being maintained

3. **Fix one issue at a time**:
   - First, get selection working again with the old code
   - Then carefully implement cluster tracking with validation
   - Test each change thoroughly before moving on

4. **Consider a simpler approach**:
   - Instead of modifying core text shaping, could we post-process positions?
   - Can we use the existing WrappedText path for all activity items?
   - Is there a way to avoid StyledWrappedText entirely?

### Important Context for Next Engineer:

1. **The position tracking infrastructure exists and works** - but only for WrappedText
2. **StyledWrappedText is the problem** - it doesn't generate proper cluster data
3. **The attempted fix was too aggressive** - changing core text shaping is risky
4. **Debug logs are essential** - enable WEZTERM_LOG=debug to see what's happening
5. **Test incrementally** - make small changes and verify selection still works

## Session 26 - Fixing Selection Issues with Glyph Position Tracking

### Issues Found by User:
1. **User messages still go half-width when selected** - and selection appears on all lines
2. **AI messages don't go half-width anymore** (good!) but selection is misaligned by ~10 lines and repeats

### Root Cause Discovery:

Through investigation, we discovered the fundamental issue:
- **`StyledWrappedText` doesn't produce `ElementCell::GlyphWithCluster` cells**
- Only produces regular `ElementCell::Glyph` cells
- This means **no actual glyph position tracking** for StyledWrappedText content
- Both user messages (when selected) and markdown content use StyledWrappedText

The glyph position tracking framework (commits 8abb348912 and c16e44fe5) works great for:
- ✅ `WrappedText` (goal text, chat input)
- ❌ `StyledWrappedText` (user messages with selection, markdown paragraphs)

### What We Attempted:

1. **Fixed user messages to always use WrappedText** (lines 1685-1697 in ai_sidebar.rs)
   - Removed the switch to StyledWrappedText when selected
   - This should fix the half-width issue
   - Selection rendered as overlay (like goal text)

2. **Removed double padding for Y position** (lines 2860-2874 in ai_sidebar.rs)
   - Changed from adding padding to both X and Y
   - Now only adds padding to X position
   - This partially addresses the misalignment

3. **Identified the real issue**: StyledWrappedText shapes text in segments
   - Each style span is shaped separately
   - Cluster values are relative to each segment, not original text
   - This breaks position tracking completely

### Proposed Solution: Add Cluster Tracking to StyledWrappedText

This is the proper long-term fix that will enable accurate position tracking for all text types.

#### Technical Plan:

1. **Modify `shape_line_with_styles()` in box_model.rs**:
   ```rust
   // After shaping each segment:
   let infos = font.shape(segment_text, ...)?;
   
   // Calculate segment's byte offset in original text
   let segment_offset_in_original = line.byte_offset + 
       (if line.skip_leading_spaces && line.leading_space_bytes > 0 {
           line.leading_space_bytes + start
       } else {
           start
       });
   
   // Adjust cluster values to be relative to original text
   let adjusted_infos: Vec<GlyphInfo> = infos.into_iter()
       .map(|mut info| {
           info.cluster += segment_offset_in_original as u32;
           info
       })
       .collect();
   ```

2. **Key Challenges**:
   - Cluster values from shaping are relative to segment text
   - Need to track byte offsets through style spans
   - Must handle space-skipped lines correctly
   - Validation checks need updating for adjusted clusters

3. **Implementation Steps**:
   
   a. **Add cluster offset tracking**:
      - Track cumulative byte offset as we process style spans
      - Account for `skip_leading_spaces` in wrapped lines
      
   b. **Modify cluster validation**:
      - Current code validates `cluster < text.len()` against segment
      - Need to validate against original text length
      
   c. **Update grapheme extraction**:
      - Currently uses `&text[cluster..]` on segment text
      - Need access to original text for proper grapheme extraction
      
   d. **Ensure backward compatibility**:
      - Only affects sidebar rendering (track_cluster = true)
      - Terminal rendering unchanged

### Alternative Quick Fix (if cluster tracking proves too complex, but user prefers the proper approach):

For markdown specifically, we could:
1. Modify `extract_positions_recursive` to better handle markdown structure
2. Don't accumulate Y offsets for nested elements
3. Use element bounds more carefully
4. This won't give perfect positions but might be "good enough"

### Important Context for Next Engineer:

1. **The core issue is StyledWrappedText doesn't track positions**
   - It's not just a calculation error
   - Without cluster data, we're using approximations

2. **User's insight about wrapping reporting back positions**
   - This is exactly what cluster tracking provides
   - WrappedText already does this correctly
   - StyledWrappedText needs to be fixed to do the same

3. **Don't trust character width approximations**
   - `calculate_char_positions()` is a hack
   - Real solution requires actual glyph positions

4. **Test with various scenarios**:
   - Multi-line user messages
   - Markdown with code blocks
   - Mixed fonts and styles
   - Unicode text (emoji, etc.)

5. **Coordinate system complexities**:
   - Activity items use viewport-relative coordinates
   - Markdown has nested element bounds
   - Selection overlays need absolute window coordinates

### Next Steps Priority:

1. **High Priority**: Implement cluster tracking for StyledWrappedText
   - This fixes the root cause
   - Enables accurate selection for all text types
   
2. **Medium Priority**: Clean up position extraction
   - Remove character width approximations
   - Use only real glyph positions
   
3. **Low Priority**: Implement remaining selection features
   - Chat input selection
   - Suggestion card selection
   - Modal selection

### Testing the Current Changes:

After the changes in this session:
- User messages should no longer go half-width (always use WrappedText)
- AI message selection Y position should be less wrong (no double padding)
- But AI messages still won't have accurate positions until cluster tracking is added

## Session 25 - Deep Investigation of Half-Width Issue

### Critical Finding:
- AI message with 2980 chars wraps to 265 lines at width=342px
- This means ~11 chars per line average
- At ~9px per char, that's ~99px per line, not 342px
- **Text is wrapping at approximately 1/3 of the intended width**

### Root Cause:
The issue ONLY occurs when:
- Text has an active selection (uses StyledWrappedText)
- Affects both user messages and AI messages when selected
- The wrapping algorithm behaves differently when style spans are present

## Session 24 - Root Cause Found

### Key Discovery:
- User messages were changed to always use `StyledWrappedText` instead of `WrappedText`
- `WrappedText` measures actual text, `StyledWrappedText` uses character width estimation
- This causes the half-width rendering issue

## Session 23 - Width Investigation

### Key Findings:
1. **Width is consistent**: All items show `sidebar_width=400, content_width=342`
2. **Positions are being extracted**: Each item shows correct line positions being extracted
3. **First activity log item renders at half-width BEFORE any selection occurs**

## Session 22 - Fixed Major Issues

### What Was Completed:
1. **Fixed Activity Log Bounds** ✅ - Converted to viewport-relative coordinates
2. **Fixed Single-Click Deselection** ✅ - Added explicit invalidation
3. **Implemented `extract_activity_item_positions`** ✅ - Full multi-line infrastructure
4. **Updated Activity Log Mouse Handling** ✅ - Uses Y coordinate for line selection
5. **Fixed Goal Selection Visibility** ✅ - Fixed coordinate calculations

## Session 21 - Recovery from Lost Work

### What Was Fixed:
1. ✅ **Goal card selection** - Restored actual glyph position usage
2. ✅ **Focus tracking** - Proper focus area management
3. ✅ **Coordinate conversion** - Activity log uses relative coordinates

### Lost Work (Session 20 Git Accident):
- Multi-line activity log selection infrastructure
- Position extraction for markdown elements
- Several hours of implementation lost via `git checkout HEAD~1`

## Session 19 - Selection Infrastructure

### What Was Fixed:
1. **Y-Position Alignment** - Removed 1.1x multiplier mismatch
2. **Multi-Paragraph Selection** - Collects positions from ALL elements
3. **Font Metrics Integration** - Proper font parameter passing

### Still Needed:
1. Markdown code block selection
2. Heading alignment fixes
3. Chat input, suggestion card, modal selection

## Key Technical Context

### 📁 Important Files:
- `sidebar/ai_sidebar.rs` - Selection state, position storage, rectangle calculation
- `termwindow/mouseevent.rs` - Mouse event handling, focus tracking
- `termwindow/render/sidebar_render.rs` - Position extraction, selection overlay rendering
- `termwindow/box_model.rs` - Text wrapping and shaping, cluster tracking

### 🔑 Core Concepts:
1. **Position Tracking**: Glyph positions extracted AFTER rendering
2. **Coordinate Systems**: Window, sidebar-relative, viewport-relative, item-relative
3. **Rendering Paths**: Different for WrappedText vs StyledWrappedText vs Markdown
4. **Selection State**: prepare_selection → activate_prepared_selection → update_selection_drag

### ⚠️ Critical Warnings:
1. **Never trust character width approximations** - Use real glyph positions
2. **Rendering order matters** - Correct z-index and sub-layers required
3. **Test all paths** - User messages, AI messages, with/without selection
4. **Handle coordinate transforms** - Multiple coordinate systems in play