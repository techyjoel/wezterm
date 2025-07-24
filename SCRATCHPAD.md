# Multi-line Chat Input & Text Selection Implementation

## Project Goals
1. **Focus Management**: Focus defaults to terminal. Only moves to chat input when clicked. Modals steal focus and return it when closed.
2. **Text Selection**: Enable click-and-drag per-character text selection with visual feedback (blue background, white text) in activity log, suggestion card, suggestion "view more" modal, and goal text.
3. **Multi-line Chat Input**: Full editing capabilities with click-to-position cursor, scrolling, Enter to send, Shift+Enter for newline.

## Latest Status (After Session 13 - Goal Selection Working)

### ✅ Working Features
1. **Multi-line Chat Input**:
   - Click-to-position cursor works perfectly with wrapped text
   - Shift+Enter inserts newlines, Enter sends
   - Scrollbar appears and functions for long text
   - Focus management works (Escape returns to terminal)

2. **Mouse Event Routing for Goal Text**:
   - Events ARE reaching the goal text handler (fixed Card component issue)
   - Coordinate transformation from absolute to relative works correctly
   - Character position calculation works (byte offset calculated properly)

3. **Selection State Management**:
   - Selection state is properly tracked (prepare/activate/drag)
   - Selection rectangles DO appear at appropriate z-index

4. **Text selection within the goal**:
   - Click-to-position selection start works correctly
   - Drag to select text works (with real glyph positions)
   - Selection rectangles render properly behind text (z-index 14, sub-layer 0)
   - Visual feedback with blue background (needs to be connected to config, to use same bg as terminal selections)
   - Click-to-deselect implemented (need to test)

### **Known Issues with Goal Selection**:
   - Cannot select last 2 characters (position tracking stops short)
   - Sometimes deselection doesn't work properly (intermittent)
   - No clipboard integration (Command-C doesn't copy)

### ❌ Other Selection Areas - Not Yet Implemented

1. **Activity Log**: Selection broken (needs position tracking integration)
2. **Suggestion Card**: Selection not implemented
3. **Show More Modal**: Selection not implemented  
4. **Chat Input**: Selection partially implemented but needs fixes

## Session 13 - Connected Position Tracking & Fixed Goal Selection

### What We Accomplished

1. **Connected Position Tracking Infrastructure**:
   - Removed pre-calculated `calculate_char_positions()` for Goal text
   - Added `goal_char_positions` storage in AISidebar
   - Implemented `extract_goal_text_positions()` to extract real glyph positions
   - Updated both click and drag handlers to use real positions

2. **Fixed Selection Rendering**:
   - Fixed z-index layering (now uses z-index 14 with sub-layer 0)
   - Adjusted selection rectangle height and vertical alignment

### Technical Implementation

- **Position Extraction**: Happens after rendering in `sidebar_render.rs`
- **Multi-line Support**: Properly tracks byte offsets across lines
- **Drag Handling**: Sidebar handles drag events when mouse outside UIItem bounds
- **Rendering**: Uses same z-index as content with sub-layer ordering

### Next Steps

1. **Fix Last 2 Characters Issue**: Position tracking may be incomplete
2. **Implement Clipboard**: Hook up Command-C to copy selected text
3. **Apply to Other Areas**: Activity Log, Suggestions, Chat Input
4. **Improve Deselection**: Make click-to-deselect more reliable

## Session 12 - Implementation Progress & Findings

### What We Fixed
1. **UIItem Event Propagation**:
   - Added debug logging to track UIItem creation and mouse events
   - Fixed coordinate transformation using goal bounds
   - Added `get_goal_bounds()` getter method to AISidebar

### Why Drag Selection Still Doesn't Work

The drag handling has a fundamental issue:
1. Mouse down on GoalText → UIItem found → `mouse_event_goal_text` called ✓
2. Start dragging → Mouse moves outside text bounds
3. Move event → `resolve_ui_item` can't find GoalText UIItem (mouse outside bounds)
4. Sidebar's `handle_mouse_event` IS called with Move event
5. But it just logs and doesn't actually update selection
6. No `update_selection_drag` calls with new positions


### Architectural Issue
The system assumes UIItems remain under the mouse during drag, but text selection needs to work when dragging outside bounds. Two approaches:
1. **Fix in sidebar** (current attempt) - Handle drag directly when selection active
2. **Mouse capture** - Ensure all Move events go to original handler

## Important Implementation Context

### Working Infrastructure
1. **Character Positions**: Calculated and passed via UIItemType
2. **Coordinate Transformation**: Properly converts absolute to relative
3. **Selection Rendering**: Works at z-index 16

### Key Files
- `sidebar_render.rs`: `render_sidebar_selection_overlays()` - rendering works
- `ai_sidebar.rs`: `calculate_selection_rectangles()` - calculation works
- `mouseevent.rs`: Event routing and coordinate transformation

## Suggested Next Steps

1. **Fix the sidebar drag handler**:
   - Actually calculate byte offset from mouse position
   - Call `update_selection_drag` with new offset
   - Need access to character positions (from where?)

2. **Alternative: Implement mouse capture**:
   - When selection starts, capture mouse to ensure all events reach handler
   - More robust than relying on UIItem hit testing during drag

3. **For Activity Log**:
   - Will have same issue - need similar fix
   - Currently shows text width jump (rendering path issue)

## Session 9 - Selection Implementation Attempt

### What We Attempted
1. **Selection Rectangle Rendering**:
   - Added `render_sidebar_selection_overlays()` method
   - Renders blue filled rectangles at specific z-indices
   - Activity log: z-index 11, Suggestions/Goals: z-index 13, Chat: z-index 14

2. **Mouse Drag State**:
   - Added drag handling in mouse events
   - ChatInput selection variant with multi-line support
   - Prepared selection on mouse down, activate on drag

3. **Fixed Text Width Jump**:
   - Removed duplicate `.max_width()` in activity log user messages
   - Added missing `.max_width()` to non-selection case

4. **Bounds Tracking**:
   - Added methods to capture UI item bounds
   - Activity items bounds captured after rendering
   - Added suggestion/goal bounds capture

### Root Causes Found (But Fixes Didn't Work)

1. **Text Width Jump**: 
   - User messages had duplicate `.max_width()` when selection active
   - NO `.max_width()` when selection inactive
   - **Fix applied but behavior unchanged**

2. **No Selection Rectangles**:
   - `calculate_selection_rectangles()` returns placeholder rectangles
   - Bounds weren't being populated (activity_item_bounds was empty)
   - **Added bounds capture but still no visual selection**

3. **Selection State Issues**:
   - Selection state is being set (verified with logging)
   - `render_sidebar_selection_overlays()` is called
   - Rectangles are calculated and `filled_rectangle()` is called
   - **But nothing appears visually**

## Important Implementation Context

### Working Infrastructure
1. **ElementCell::GlyphWithCluster**: Stores position-specific cluster data, enables pixel-perfect positioning
2. **Exact Glyph Positions**: Available for chat input, used successfully for cursor positioning
3. **Multi-line Chat Input**: Fully functional with wrapped text support

### Critical Observations
1. **The same glyph position tracking that works perfectly for cursor positioning should work for selection**
3. **The text width jump suggests rendering mode changes when selection is active**

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

### Consistent Line Height (though should be a constant rather than magic number)
```rust
// Always use 1.1x multiplier for line spacing
let line_height_with_spacing = line_height * 1.1;
```

## Critical Implementation Constraints

- Font access only during rendering phase (architectural requirement)
- Scissor rect requires dedicated z-index layers (GPU requirement)
- Each unique z-index = separate GPU draw call (performance consideration)
- UIItemType is the only reliable way to pass data from render to event handling
- Element box model: `border_rect` includes padding and border but NOT margin
- **StyleSpan backgrounds are ignored in rendering** - only text color is applied
- **Two different text wrapping algorithms** cause width jumps between selection states

## Session 11 - Selection Implementation Progress & Findings

### Remaining Key notes from session

1. **Selection Rectangles NOW APPEAR!** ✅
   - Blue selection rectangles are visible
   - Even zero-width selections show a 2px cursor

2. **Extensive Debugging Added** ✅
   - Detailed logging throughout selection pipeline
   - Coordinate and bounds logging

### Critical Discovery: Mouse Events Were Not Reaching Goal Text

The main issue was that mouse events aren't reaching the `mouse_event_goal_text` handler because:

1. **UIItemType is buried in Card structure**:
   ```
   Goal Element with UIItemType::GoalText
   └── Card (wraps in its own elements)
       └── Another Element wrapper
   ```

## Session 11 - Terminal Selection Analysis & Revised Plan

### How Terminal Selection Actually Works

After deep investigation, the terminal selection pattern is:
1. **Selection is passed as a parameter** through the rendering pipeline
2. **Selection rectangles are rendered using `filled_rectangle()`** at sub-layer 0 during line rendering
3. **This happens during the element processing phase**, not as a separate overlay
4. **Key insight**: Selection is integrated into line rendering, not overlaid after

### Why Sidebar Selection Is Different

The sidebar has architectural constraints:
- **Two-phase rendering is mandatory** - compute elements first, then render
- **Selection must be rendered after elements are computed** to have bounds available
- **The overlay approach is architecturally correct** for the sidebar
- **The issue is in the selection rectangle calculation**, not the approach


