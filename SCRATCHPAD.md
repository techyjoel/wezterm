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
   - Selection rectangles DO appear at z-index 16
   - Green test rectangle confirms rendering pipeline works

### 🎯 Goal Text Selection - Now Working!

1. **What's Working**:
   - Click-to-position selection start works correctly
   - Drag to select text works (with real glyph positions)
   - Selection rectangles render properly behind text (z-index 14, sub-layer 0)
   - Visual feedback with blue background
   - Text width no longer jumps when selection active
   - Click-to-deselect implemented

2. **Known Issues with Goal Selection**:
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
   - Removed green test rectangle
   - Fixed z-index layering (now uses z-index 14 with sub-layer 0)
   - Adjusted selection rectangle height and vertical alignment
   - Text now renders on top of selection background

3. **Code Quality Improvements**:
   - Added constants for padding/sizing values
   - Implemented click-to-deselect functionality
   - Fixed misleading comments
   - Added documentation to dev-docs

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

2. **Attempted Drag Fix**:
   - Added Move event handling in sidebar's `handle_mouse_event`
   - But implementation has a flaw - it doesn't actually handle the drag
   - Just logs "will be handled by UIItem" and returns

### Why Drag Selection Still Doesn't Work

The drag handling has a fundamental issue:
1. Mouse down on GoalText → UIItem found → `mouse_event_goal_text` called ✓
2. Start dragging → Mouse moves outside text bounds
3. Move event → `resolve_ui_item` can't find GoalText UIItem (mouse outside bounds)
4. Sidebar's `handle_mouse_event` IS called with Move event
5. But it just logs and doesn't actually update selection
6. No `update_selection_drag` calls with new positions

### Critical Code Locations
- `mouseevent.rs:1982-2026` - Goal text Move handler (never called during drag)
- `ai_sidebar.rs:3631-3636` - Broken drag handler (just logs, doesn't update)
- `mouseevent.rs:344-380` - Sidebar event forwarding (working correctly)

## What Needs to Be Fixed

### Immediate Fix: Proper Drag Handling in Sidebar
The sidebar's Move event handler needs to:
1. Calculate the relative position from mouse coordinates
2. Convert to byte offset using character positions
3. Call `update_selection_drag` with new position

Current broken code:
```rust
SelectionTarget::Goal { anchor_byte, .. } => {
    // Just logs and returns - doesn't update!
    log::debug!("Goal drag detected in sidebar, but will be handled by UIItem");
}
```

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
- `ai_sidebar.rs:3578-3650`: Broken drag handler that needs fixing

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

## Why Selection Still Doesn't Work - Deeper Issues

### Possible Architectural Problems

1. **Z-Index/Scissor Rect Conflicts**:
   - Selection rectangles at z-index 11/13/14 might be clipped by scissor rects
   - Activity log has scissor rect at z-index 12
   - Selection at z-index 11 might be outside the scissor bounds

2. **Coordinate System Mismatches**:
   - UI item bounds are in window coordinates
   - Selection rectangles might need different coordinate space
   - Sidebar translation might be applied twice or not at all

3. **Rendering Pipeline Issues**:
   - `filled_rectangle()` might not work as expected at those z-indices
   - Two-phase rendering might require different approach
   - Selection overlays might need to be Elements, not direct quads

4. **State Synchronization**:
   - Selection state updated in mouse events
   - Rendering happens later
   - Bounds might be stale or in wrong coordinate space

## Next Steps - Critical Debugging Needed

### 1. Verify Selection Rendering
- Add bright, obvious test rectangle at known coordinates
- Test if `filled_rectangle()` works at different z-indices
- Check if scissor rect is clipping selection
- Try rendering selection as Elements instead of direct quads

### 2. Debug Coordinate Systems
- Log exact coordinates at each step
- Verify bounds capture matches rendering
- Check if sidebar_x offset is needed
- Test with fixed position rectangle first

### 3. Alternative Approaches
- Try rendering selection within the text elements (inline)
- Use the existing create_selection_spans approach differently
- Consider if StyleSpan backgrounds could be fixed
- Look at how terminal selection works for reference

### 4. Simplify to Find Root Issue
- Start with hardcoded selection rectangle
- Remove all coordinate transforms
- Test at highest z-index without scissor
- Add visual debugging overlays

## Important Implementation Context

### Working Infrastructure
1. **ElementCell::GlyphWithCluster**: Stores position-specific cluster data, enables pixel-perfect positioning
2. **Exact Glyph Positions**: Available for chat input, used successfully for cursor positioning
3. **Multi-line Chat Input**: Fully functional with wrapped text support

### Selection Implementation Status
- **SelectionState** structure exists with proper variants
- **Mouse event routing** implemented for drag selection
- **Bounds tracking** added but may have issues
- **Selection rendering** implemented but not visible

### Critical Observations
1. **The same glyph position tracking that works perfectly for cursor positioning should work for selection**
2. **Something fundamental is preventing selection rectangles from appearing**
3. **The text width jump suggests rendering mode changes when selection is active**

### Key Files for Next Session
- `sidebar_render.rs`: render_sidebar_selection_overlays()
- `ai_sidebar.rs`: calculate_selection_rectangles(), selection state
- `mouseevent.rs`: drag handling for selection
- `box_model.rs`: how StyleSpan backgrounds are (not) rendered

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

## Critical Implementation Constraints

- Font access only during rendering phase (architectural requirement)
- Scissor rect requires dedicated z-index layers (GPU requirement)
- Each unique z-index = separate GPU draw call (performance consideration)
- UIItemType is the only reliable way to pass data from render to event handling
- Element box model: `border_rect` includes padding and border but NOT margin
- **StyleSpan backgrounds are ignored in rendering** - only text color is applied
- **Two different text wrapping algorithms** cause width jumps between selection states

## Fundamental Issues to Investigate

1. **Why Selection Rectangles Don't Appear**:
   - Red test rect works → rendering pipeline is fine
   - Selection state is set → event handling works
   - But selection rects don't appear → calculation or coordinate issue
   
2. **Text Width Jump Root Cause**:
   - Selection state changes text rendering path
   - Even with "unified" StyledWrappedText approach
   - Need to trace where the divergence happens

3. **Possible Architectural Mismatch**:
   - Selection might need to be rendered differently
   - Current approach might be fundamentally incompatible
   - Consider how terminal selection works for reference

## Session 11 - Selection Implementation Progress & Findings

### What We've Accomplished

1. **Selection Rectangles NOW APPEAR!** ✅
   - Changed to z-index 16 (same as cursor)
   - Blue selection rectangles are visible
   - Even zero-width selections show a 2px cursor

2. **Text Width Jump PARTIALLY FIXED** ✅
   - No longer jumps for zero-width selections
   - Only uses StyledWrappedText when there's actual selection (start != end)

3. **Extensive Debugging Added** ✅
   - Detailed logging throughout selection pipeline
   - Green test rectangle confirms rendering works
   - Coordinate and bounds logging

### Critical Discovery: Mouse Events Not Reaching Goal Text

The main issue is that mouse events aren't reaching the `mouse_event_goal_text` handler because:

1. **UIItemType is buried in Card structure**:
   ```
   Goal Element with UIItemType::GoalText
   └── Card (wraps in its own elements)
       └── Another Element wrapper
   ```

2. **Evidence from logs**:
   - Character positions ARE calculated: "Goal text has 35 char positions"
   - Selection rectangles DO render at correct position
   - But NO mouse event logs appear ("Goal text clicked", "Goal drag")
   - Selection stuck at byte 35 (always end of text)

### Why Selection Doesn't Work

1. **Mouse Events Blocked**: The Card component structure prevents UIItemType from being detected
2. **Selection Always at End**: Without mouse events, byte offset defaults to text length
3. **No Drag Updates**: Can't update selection during drag without receiving events

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

### Revised Implementation Plan

#### Phase 1: Fix Immediate Issues (2 hours)

1. **Use z-index 16 for selection** (proven to work with cursor and test rectangle)
   ```rust
   // In render_sidebar_selection_overlays()
   let mut layers = gl_state.layer_for_zindex(16)?; // Same as cursor
   ```

2. **Add diagnostic logging inside selection calculation**
   ```rust
   fn calculate_selection_rectangles(&self, selection: SelectionState) -> Vec<Rect> {
       log::debug!("Selection target: {:?}", selection.target);
       log::debug!("Available bounds: activity={}, chat={}", 
           self.activity_item_bounds.len(), 
           !self.chat_input_bounds.is_empty());
       // Log each calculated rectangle with dimensions
   }
   ```

3. **Add hardcoded test rectangle inside selection rendering**
   ```rust
   // Right before rendering actual selection rectangles
   let test_rect = euclid::rect(sidebar_x + 50.0, 300.0, 200.0, 30.0);
   self.filled_rectangle(&mut layers, 0, test_rect, 
       LinearRgba::with_components(0.0, 1.0, 0.0, 0.8))?; // Green
   ```

#### Phase 2: Fix Selection Rectangle Calculation (3 hours)

1. **For Chat Input - Use Exact Glyph Positions**
   ```rust
   fn calculate_chat_selection_rectangles(&self, selection: &SelectionState) -> Vec<Rect> {
       // Use self.chat_input.exact_glyph_positions if available
       // These contain (x_start, x_end, byte_offset) for each glyph
       // Calculate selection bounds using these exact positions
   }
   ```

2. **For Activity Log - Improve Character Width Estimation**
   ```rust
   fn calculate_activity_selection_rectangles(&self, bounds: &Rect, text: &str, 
                                             selection: &SelectionState) -> Vec<Rect> {
       // Use actual font metrics instead of hardcoded 8.5px
       let font = &self.fonts.body;
       let metrics = font.metrics();
       let avg_char_width = metrics.average_advance;
       
       // Handle wrapped text properly
       let wrapped_lines = wrap_text(text, font, bounds.size.width - 16.0);
       // Calculate selection per wrapped line
   }
   ```

3. **Store Glyph Positions During Rendering**
   - Capture positions when elements are computed
   - Store in accessible location for selection calculation
   - Similar to how chat input stores exact_glyph_positions

#### Phase 3: Fix Text Width Jump (2 hours)

1. **Always Use StyledWrappedText**
   ```rust
   // For AI messages, pre-process markdown to style spans
   fn markdown_to_style_spans(markdown: &str, fonts: &SidebarFonts) -> Vec<StyleSpan> {
       // Convert markdown to spans that produce same layout as MarkdownRenderer
       // This ensures consistent width calculation
   }
   ```

2. **Unify Rendering Path**
   - Remove conditional rendering based on selection state
   - Always use same element type and max_width
   - Pre-calculate style spans for markdown

#### Phase 4: Implement Position Tracking for All Selectable Text (3 hours)

1. **Extend UIItemType for Position Tracking**
   ```rust
   UIItemType::ActivityItem {
       item_id: String,
       track_positions: bool, // New flag
   }
   ```

2. **Capture Positions During compute_element**
   - When track_positions is true, extract glyph positions
   - Store for use during selection rendering
   - Follow chat input pattern

#### Phase 5: Testing Strategy (1 hour)

1. **Incremental Testing**
   - First: Get any selection rectangle to appear at z-index 16
   - Second: Fix coordinate calculations to match text position
   - Third: Handle multi-line selection correctly

2. **Debug Tools**
   - Extensive logging of all coordinates
   - Hardcoded test rectangles at each stage
   - Visual markers for debugging

### Key Implementation Details

1. **Z-index 16 is critical** - proven to work with cursor
2. **Coordinate space is window coordinates** - no transformation needed
3. **Sub-layer 0 for backgrounds** - consistent with terminal
4. **Must handle wrapped text** - selection per visual line
5. **Font metrics matter** - use actual metrics, not hardcoded values

### Why This Will Work

1. **Respects sidebar architecture** - overlay approach is correct
2. **Uses proven z-index** - 16 works for cursor and test rectangle
3. **Fixes actual issues** - rectangle calculation and text width jump
4. **Incremental approach** - can verify each step works

### Estimated Timeline
- Phase 1: 2 hours (immediate fixes and debugging) ✅ COMPLETE
- Phase 2: 3 hours (selection calculation) ✅ COMPLETE
- Phase 3: 2 hours (text width jump) ✅ PARTIAL
- Phase 4: 3 hours (position tracking) ❌ BLOCKED
- Phase 5: 1 hour (testing)
Total: 11 hours

## Next Steps to Fix Selection

### Immediate Fix: Restructure Goal Rendering

The Card component is blocking mouse events. Options:

1. **Move UIItemType to Card level** (Quick fix)
   - Modify Card to accept and propagate UIItemType
   - Ensure Card's render() method preserves the UIItemType

2. **Flatten Goal Structure** (Better long-term)
   - Remove Card wrapper for selectable text
   - Render goal text directly with proper styling
   - Keep buttons separate from selectable area

3. **Debug Event Routing** (Diagnostic)
   - Add logging to see which UIItems are being hit
   - Trace why Goal UIItemType isn't detected
   - Check if Card creates conflicting UIItems

### Other Issues to Address

1. **Byte Offset Calculation**
   - Currently always returns end of text (35)
   - Need to fix character position mapping
   - Ensure x-coordinate properly maps to byte offset

2. **Activity Log Selection**
   - Same Card structure issue likely affects activity items
   - Height of 7950px suggests bounds calculation error

3. **Chat Input Selection**
   - Has exact glyph positions but may have similar issues
   - Test after fixing Goal to see if same solution applies

### Key Insights

1. **Rendering works** - Selection rectangles appear correctly
2. **State management works** - Selection state is tracked
3. **Event routing is broken** - UIItemType not accessible through Card
4. **Position calculation needs work** - Always selects at end

The architecture is sound, but the Card component abstraction is interfering with the event system.