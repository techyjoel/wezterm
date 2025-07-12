# Multi-line Chat Input & Text Selection Implementation

## Project Goals
1. **Focus Management**: Focus defaults to terminal. Only moves to chat input when clicked. Modals steal focus and return it when closed.
2. **Text Selection**: Enable click-and-drag per-character text selection with visual feedback (blue background, white text) in activity log, suggestion card, suggestion "view more" modal, and goal text.
3. **Multi-line Chat Input**: Full editing capabilities with click-to-position cursor, scrolling, Enter to send, Shift+Enter for newline.

## Latest User Test Results (Verbatim)

*"I have tested, here is current status: 1. The cursor looks much nicer and doesn't shift the text. 2. Click to place the cursor is far worse. It appears that the character width in the click evaluation is much larger than used for rendering - when I click on leftmost characters the click is fairly accurate, but clicking in the middle of the line places the cursor to the far right. Then, if I try to click on the 2nd line of text it doesn't ever register, the cursor never can click to place on that 2nd line. 3. Scrolling still does not work in the chat input box at all. There is no scrollbar, and I cannot scroll with the scroll wheel. 4. The text selection functionality seems un-changed from before this rev of your work. Selection of text in the sidebar is different now, but still very broken. I can't select text at all in the suggestion card (nor the suggestion card modal). When I click-and-drag to select text in the goal card or the activity log, all of the the text suddenly jumps to half-width (the text only appears on the left side of the element). No text ever appears like it is selected, dragging around appears to do nothing."*

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

### ❌ Current Issues (After Latest Changes)

#### 1. Click-to-Position Cursor (Much Worse)
**Symptoms**: 
- Character width in click evaluation appears much larger than rendering width
- Clicks on leftmost characters are fairly accurate
- Clicks in middle of line place cursor far right
- Clicks on 2nd line never register at all

**Root Causes**:
- **Hardcoded character width mismatch**: Click handler uses `char_width = 8.5` but actual font `cell_width` varies based on font size
- **No access to real font metrics**: `handle_chat_input_click_simple` can't access `LoadedFont` to get real `cell_width`
- **Font size reduction not accounted for**: Sidebar body font uses `font_size - 1.0` point reduction
- **Coordinate calculation issues**: Multiple layers of padding not properly accounted for
  - Container padding: 8px left/right, 6px top/bottom
  - Per-line text element padding: 4px left/right, 2px top/bottom (found in rendering but not click handling)

**What We Tried**:
- ✅ Used consistent line height with 1.1x multiplier
- ✅ Updated character width to 8.5 (still not matching actual)
- ✅ Added more sophisticated character width estimation
- ❌ Still using hardcoded values instead of actual font metrics

#### 2. Scrolling in the chat input box (Still Non-Functional)
**Symptoms**:
- No scrollbar appears
- Mouse wheel doesn't scroll
- No visual indication of scrollability

**Root Cause (Suspected)**:
- **Possible timing issue**: Bounds might be set after they're needed for first render
- **Needs deeper investigation**

**What We Tried**:
- ✅ Added `chat_input` field to `SidebarScrollbars`
- ✅ Implemented scrollbar calculation in `get_scrollbars`
- ✅ Added scrollbar rendering in `render_sidebar_scrollbars`
- ✅ Fixed height calculations to use 1.1x multiplier consistently

#### 3. Text Selection (Text Jumps to Half-Width)
**Symptoms**:
- Text jumps to half-width when attempting to select
- No visual selection feedback
- Can't select in suggestion cards or modal
- Selection dragging appears to do nothing

**Root Causes (Confirmed for some, suspected for others)**:
- **Max width application timing**: For goals/suggestions, `max_width` is applied after element creation in builder chain, not during
- **Width calculation**: Goals and suggestions use `sidebar_width - 40.0` which may be incorrect
- **Suggestion modal uses markdown**: Modal content rendered with MarkdownRenderer which doesn't support selection
- **Selection spans might not be visible**: Colors may be incorrect or overridden

**What We Tried**:
- ✅ Added `max_width` to all `StyledWrappedText` elements
- ✅ Fixed color inheritance in selection spans
- ❌ Width constraint still not properly applied during text wrapping phase for goals/suggestions

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

## Next Steps (Priority Order)

### 1. Fix Click-to-Position Cursor
- [ ] Pass actual font reference to click handlers (requires architectural change)
- [ ] OR: Pre-calculate and store exact character positions during rendering
- [ ] Account for ALL padding layers in click position calculation
- [ ] Store line height with 1.1x multiplier in UIItemType
- [ ] Fix 2nd line click detection by properly handling viewport bounds

### 2. Fix Scrolling
- [ ] Debug why scrollbar doesn't appear - add logging to verify:
  - Is chat input focused?
  - How many lines of text are there?
  - What are the height calculations?
- [ ] Consider showing scrollbar even when not focused if content is scrollable
- [ ] Verify mouse wheel events are reaching the handler

### 3. Fix Text Selection Rendering
- [ ] Refactor element creation to apply max_width at creation time, not after
- [ ] Verify selection span colors are not being overridden
- [ ] Add selection support to suggestion modal (convert from markdown to selectable text)
- [ ] Debug why selection visual feedback doesn't appear
- [ ] Consider using different rendering approach that preserves width constraints

### 4. Architectural Improvements
- [ ] Consider storing font metrics in UIItemType for accurate click handling
- [ ] Unify rendering patterns - choose either Activity Log or Modal pattern
- [ ] Fix state synchronization - calculate bounds before rendering
- [ ] Standardize coordinate system usage

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