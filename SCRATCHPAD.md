# Multi-line Chat Input & Text Selection Implementation

## Project Goals
1. **Focus Management**: Focus defaults to terminal. Only moves to chat input when clicked. Modals steal focus and return it when closed.
2. **Text Selection**: Enable click-and-drag per-character text selection with visual feedback (blue background, white text) in activity log, suggestion card, suggestion "view more" modal, and goal text.
3. **Multi-line Chat Input**: Full editing capabilities with click-to-position cursor, scrolling, Enter to send, Shift+Enter for newline.

## Latest Status (After Session 7 - Successful Fix!)

### ✅ Working Features
1. **Click-to-Position**: FIXED! Clicking anywhere in wrapped text correctly positions cursor
2. **Cursor Display**: FIXED! Cursor now appears at correct position on all wrapped lines
3. **Text Entry**: Characters appear at cursor position on any line
4. **Line Wrapping**: Text properly wraps and cursor/clicks work across wrapped lines

### ❌ Remaining Issues
1. **Shift+Enter**: Text may disappear or scroll out of view when adding newlines
2. **Scrolling**: Not functional - no scrollbar appears, mouse wheel doesn't work
3. **Text Selection**: Not yet implemented

### What Fixed the Issue (Session 7)

1. **Completed ElementCell::GlyphWithCluster Implementation**:
   - Stores position-specific cluster data alongside glyphs
   - Solves the fundamental cache collision problem
   - Cluster values now correct: 0, 1, 2, 3... (not 2, 28, 7, 32...)

2. **Fixed Click Handler for Wrapped Lines**:
   - Maps visual line clicks to logical line positions
   - Properly converts document byte offsets to line/column
   - Works across wrapped text boundaries

3. **Fixed Cursor Position Calculation**:
   - Maps logical cursor position to visual line position
   - Uses exact glyph positions for pixel-perfect cursor placement
   - Handles wrapped lines correctly

### Root Cause Analysis

The fundamental issue is that **cluster information is position-specific, not glyph-specific**:
- HarfBuzz provides cluster values that represent byte positions in the input text
- We've been trying to cache these position-specific values with the glyphs
- But the same glyph (e.g., 'T') can appear at many positions with different clusters
- The glyph cache returns the cluster from wherever that glyph was first cached

### Critical Architecture Issues Discovered

1. **Glyph Cache Design Conflict**:
   - Glyph cache is designed to share visual representations (textures)
   - But cluster information is unique to each text position
   - These two concepts are fundamentally incompatible

2. **Performance vs Accuracy Trade-off**:
   - Original design cached glyphs for performance (share textures)
   - But accurate positioning requires position-specific data
   - Current approach tries to mix both, causing the bugs

3. **Alternative Approaches Needed**:
   - Option A: Track clusters separately from glyphs (current attempt)
   - Option B: Don't use glyph cache for sidebar text
   - Option C: Create a separate position tracking system

## Current Implementation Status

### ✅ Confirmed Working Features
1. **Basic Chat Input**
   - Enter sends messages
   - Shift+Enter inserts newlines (likely, need to test once scrolling is working)
   - Escape unfocuses back to terminal
   - Focus visual feedback (blue border when focused)

2. **Scissor Rect Infrastructure**
   - Activity log pattern implemented
   - Chat input uses filled rectangles for background/border
   - Text content rendered with scissor rect clipping at z-index 15

3. **Focus Management Infrastructure**
   - `has_keyboard_focus()` properly routes keyboard events
   - Chat input receives keyboard events when focused
   - Modal focus handling works correctly

4. **Cursor Rendering**
   - Cursor now renders as overlay element at z-index 16
   - No longer shifts text
   - 2px wide light gray filled rectangle

### ❌ Current Issues (Still Broken After Fixes)

#### 1. Click-to-Position Cursor (Much Worse)
**Symptoms**: 
- The placement of the cursor upon a click visually appears about correct, however functionally it is not
- Typed characters (after a click) appears significantly to they left of where the cursor visually appears
- This is consistent with the visual placement of the cursor during initial text entry: The cursor moves to the right, away from the leading character, over time as the user types. It's as if the estimation of where the cursor should appear uses widths that are too large.
- Clicks on 2nd line never register at all

**Suspected Root Causes**:
- **Hardcoded character width mismatch**: Click handler uses `char_width = 8.5` but actual font `cell_width` varies based on font size
- **No access to real font metrics**: `handle_chat_input_click_simple` can't access `LoadedFont` to get real `cell_width`
- **Font size reduction not accounted for**: Sidebar body font uses `font_size - 1.0` point reduction
- **Coordinate calculation issues**: Multiple layers of padding not properly accounted for
  - Container padding: 8px left/right, 6px top/bottom
  - Per-line text element padding: 4px left/right, 2px top/bottom (found in rendering but not click handling)

**What We Tried**:
- ✅ Pre-calculated character positions during rendering and stored in UIItemType::ChatInput
- ✅ Added handle_chat_input_click_with_positions() that uses pre-calculated positions
- ✅ Applied 0.85x adjustment factor to account for font size reduction
- ❌ Character width is still wrong: base=20px, adjusted=17px but actual appears to be ~8-10px
- ❌ The font metrics show cell_height=25.78 which seems too large for sidebar text

#### 2. Scrolling in the chat input box (Still Non-Functional)
**Symptoms**:
- No scrollbar appears
- Mouse wheel doesn't scroll
- No visual indication of scrollability

**Root Cause (Suspected)**:
- **Possible timing issue**: Bounds might be set after they're needed for first render
- **Needs deeper investigation**

**What We Tried**:
- ✅ Fixed scrollbar rendering to check both activity log AND chat input
- ✅ Changed to show scrollbar even when unfocused if content is scrollable
- ✅ Added debug logging throughout the scrollbar pipeline
- ❌ Logs show `chat_input=false` - scrollbar is never detected as needed
- ❌ Chat input bounds may not be set properly

#### 3. Text Selection (Text Jumps to Half-Width)
**Symptoms**:
- Text jumps to half-width when attempting to select (e.g. in the activity log)
- No visual selection feedback
- Can't select in suggestion cards or modal
- Selection dragging appears to do nothing

**Root Causes (Confirmed for some, suspected for others)**:
- **Max width application timing**: For goals/suggestions, `max_width` is applied after element creation in builder chain, not during
- **Width calculation**: Goals and suggestions use `sidebar_width - 40.0` which may be incorrect
- **Suggestion modal uses markdown**: Modal content rendered with MarkdownRenderer which doesn't support selection
- **Selection spans might not be visible**: Colors may be incorrect or overridden

**What We Tried**:
- ✅ Added debug logging for width calculations
- ✅ Verified selection spans are being created correctly
- ❌ Text still jumps to half-width when selection starts
- ❌ No visual selection feedback appears
- ❌ Selection in suggestion cards/modal still not working

### System-Level Issues Discovered

1. **Font Metrics Access Pattern**
   - Fonts only available during rendering phase
   - Mouse handlers must use estimates
   - Creates fundamental mismatch between click handling and rendering

2. **Mixed Rendering Patterns**
   - Sidebar uses Activity Log Pattern (filled rectangles + separate element renders)
   - Also tries to compose elements like Modal Pattern
   - Creates z-index conflicts and rendering issues

3. **State Synchronization**
   - Chat input bounds set during rendering but used for event handling
   - Creates one-frame delay where bounds might be stale
   - Height caches updated after rendering, not during

4. **Coordinate System Issues**
   - Mixing window coordinates, relative coordinates, and sidebar-relative coordinates
   - Translation happens at different points, sometimes double-translating
   - Padding/margin calculations inconsistent between click handling and rendering

## Attempted Fixes Summary

### Session 3:
1. **Click-to-Position**: Pre-calculated character positions during rendering, stored in UIItemType
2. **Scrollbar**: Fixed rendering logic to check both activity log and chat input
3. **Font Metrics**: Applied 0.85x adjustment for font size reduction

### Key Discoveries
1. **Font Size Mismatch**: Sidebar body font has `font_size_reduction` applied (typically 1.0pt)
   - Font metrics report cell_width=20px, cell_height=25.78px
   - Actual rendered characters appear much smaller (~8-10px wide)
   - 0.85x adjustment factor is still not enough

2. **Architectural Constraints**:
   - Fonts only available during rendering phase, not event handling
   - UIItemType is the correct pattern for passing data between phases
   - Scissor rect clipping requires dedicated z-index layers

3. **Scrollbar Detection Issue**:
   - Chat input scrollbar consistently shows as `false` in logs
   - Suggests the bounds or line counting logic is incorrect

### Session 4: Improved Understanding
1. **Proportional Font Issue Confirmed**:
   - Sidebar uses Roboto (proportional font), not monospace
   - `cell_width` metric is meaningless for proportional fonts
   - Actual character widths vary significantly (spaces ~4px, 'W' ~12px)
   - Changed from 0.85x to 0.5x adjustment based on empirical testing

2. **Y-Coordinate Bug Fixed**:
   - Container padding (8px) wasn't accounted for in click handling
   - This caused clicks to register on wrong lines

3. **Character-Specific Width Estimation**:
   - Implemented different widths for uppercase, lowercase, digits, punctuation
   - Much more accurate but still not perfect

## Root Cause Analysis

### 1. Font Metrics Problem (CONFIRMED)
The fundamental issue is multi-faceted:
- **Proportional Font**: Roboto is proportional, so `cell_width` (designed for monospace) is meaningless
- **Font Size Reduction**: The sidebar applies 1pt reduction, but metrics come from base font
- **No Glyph Access**: Event handlers can't access actual glyph advances from text shaping
- **Result**: Character width estimation will always be imperfect without actual glyph data

### 2. Click Position Reliability (PARTIALLY FIXED)
- Y-coordinate calculation was off due to missing container padding
- X-coordinate still imperfect due to character width estimation
- Some clicks may still be consumed by other UI elements

### 3. Text Selection Width Jump
The width constraint is likely being recalculated during selection state changes, causing a different layout calculation.

### 4. Proper Solution Identified
**Solution**: Modify CachedGlyph to preserve cluster information from text shaping
- See **POSITION_TRACKING.md** for detailed implementation plan
- Sidebar-only implementation to avoid terminal performance impact

## Next Steps

### Remaining High-Priority Issues

1. **Shift+Enter Newline Handling**:
   - Issue: Text disappears or scrolls out of view
   - Likely cause: Chat input only displays 2 lines, scroll offset not updated properly
   - Solution: Fix scroll offset calculation when newlines are added

2. **Scrolling Functionality**:
   - Issue: No scrollbar appears, mouse wheel doesn't work
   - Cause: Scrollbar detection needs to account for visual lines, not just logical lines
   - Solution: Calculate total height based on wrapped lines

3. **Text Selection**:
   - Infrastructure is ready (exact positions available)
   - Need to implement selection state management
   - Add visual feedback and copy functionality

### Technical Approach

The ElementCell::GlyphWithCluster solution is working well. The key insight was that cluster information must be stored per-instance, not in the shared glyph cache. This maintains the performance benefits of texture caching while enabling pixel-perfect text interaction.

### Integration Points

#### 1. Update Chat Input Click Handler
Replace the character width estimation in `handle_chat_input_click_with_positions()`:

```rust
// In compute_element for WrappedText:
let (lines, wrapped_lines) = self.wrap_text_with_info(text, &element.font, max_width, context, &style)?;

// Extract positions for each line
let mut line_positions = Vec::new();
for (cells, wrapped_line) in lines.iter().zip(wrapped_lines.iter()) {
    let position_map = GlyphPositionMap::from_cells(cells, wrapped_line);
    line_positions.push(position_map.positions);
}

// Store in UIItemType::ChatInput
if let Some(UIItemType::ChatInput { ref mut line_positions }) = element.item_type {
    *line_positions = line_positions;
}
```

Then in the click handler, use `GlyphPositionMap::hit_test()` instead of width estimates.

#### 2. Enable for Other Interactive Text
Apply the same pattern to:
- Activity log entries
- Suggestion cards
- Goal text

#### 3. Text Selection Implementation
With exact positions, implement selection by:
- Tracking selection start/end byte offsets
- Using `GlyphPositionMap` to convert mouse positions to byte offsets
- Rendering selection spans based on glyph positions

### Remaining Issues to Address:
1. **Scrolling**: Still need to debug why scrollbar doesn't appear
2. **Text Selection Width Jump**: Need to trace width recalculation  
3. **Selection in Modals**: MarkdownRenderer doesn't support selection

### Testing Strategy
1. Test with various Unicode text (emoji, RTL, ligatures)
2. Verify cursor positioning accuracy
3. Measure performance impact (should be <5% for sidebar)
4. Test text selection across line boundaries

### 5. Critical Context
- The sidebar uses **Roboto** (proportional font) with 1pt size reduction
- **Cell width metric is meaningless** for proportional fonts
- Current **0.5x adjustment** is empirically better but still imperfect
- **Exact glyph positions** infrastructure is now implemented (see POSITION_TRACKING.md)
- Ready to integrate pixel-perfect positioning

## Code Patterns That Work

### Cursor Overlay Rendering
```rust
// This pattern successfully renders cursor without shifting text
if let Some((cursor_x, cursor_y)) = ai_sidebar.get_cursor_position(&fonts.body) {
    let cursor_rect = euclid::rect(
        text_bounds.origin.x + cursor_x,
        text_bounds.origin.y + cursor_y,
        cursor_width,
        cursor_height
    );
    self.filled_rectangle(&mut layers, 0, cursor_rect, cursor_color)?;
}
```

### Consistent Line Height
```rust
// Always use 1.1x multiplier for line spacing
let line_height_with_spacing = line_height * 1.1;
```

## Testing Notes for Next Session

1. **For Click Position**: Log actual font metrics vs hardcoded values
2. **For Selection**: Check if selection state is being set but not rendered
3. **Check Console Logs**: Many debug statements were added

## Critical Implementation Constraints

- Font access only during rendering phase (architectural requirement)
- Scissor rect requires dedicated z-index layers (GPU requirement)
- Each unique z-index = separate GPU draw call (performance consideration)
- UIItemType is the only reliable way to pass data from render to event handling
- Element box model: `border_rect` includes padding and border but NOT margin