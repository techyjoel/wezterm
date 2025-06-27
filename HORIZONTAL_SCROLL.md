# Horizontal Scrolling for Code Blocks - Implementation Status 🔄

## Current Status: PARTIALLY FUNCTIONAL WITH CRITICAL ISSUES

Horizontal scrolling infrastructure is in place but core functionality is broken:

### Working Features ✅
- **Scrollbars appear when needed** (mostly - some borderline cases missed)
- **UIItems properly extracted** - Mouse events fire in activity log
- **Copy button always visible** above code blocks
- **Thumb moves** when dragging (but content doesn't scroll)

### Critical Issues 🔴

1. **Scrolling Implementation Partially Working**:
   - Content scrolls in activity log with thumb drag and shift+wheel ✅
   - Suggestion modal shows NO horizontal scrollbars at all ❌
   - Vertical scrolling broken when mouse over code blocks ❌

2. **Clipping/Overflow Will Be Issue**:
   - Once scrolling works, content will overflow onto terminal area due to z-layer architecture
   - Need proper viewport clipping to prevent content appearing over terminal
   - **Root cause**: "Cut-a-hole" pattern doesn't work for horizontal scrolling unless z-index of scrolled content is below the terminal area background.

3. **Modal Issues**:
   - NO horizontal scrollbars appear in suggestion modal at all ❌
   - Vertical scrolling only works outside markdown area (in padding) ❌
   - **Root cause**: Modal not using code block registry

4. **Scrollbar Styling Still Wrong** ❌:
   - Still "chunky" and styled incorrectly
   - Thumb is not positioned within the scrollbar track
   - Does NOT match vertical scrollbar appearance

5. **Code Block Width Calculation Still Broken** ❌:
   - Borderline content does NOT show horizontal scrollbars
   - 5px buffer not sufficient or not working properly

6. **Mouse Wheel Behavior**:
   - Currently captures ALL scroll events when mouse is over a code block, though scroll bar only moves when shift is pressed
   - Should only capture scroll events when Shift is pressed
   - Non-shift scroll should pass through for vertical scrolling

7. **Text Selection Disabled**:
   - Text cursor appears but can't select text
   - Affects all sidebar content (goals, suggestions, activity log, code blocks)
   - Likely due to UIItem event handling blocking selection

## Implemented Features (In Code)

- ✅ Horizontal scrolling with auto-hide scrollbars
- ✅ Full mouse interaction (drag, wheel, click-to-focus)
- ✅ Keyboard navigation (arrow keys, Home/End, Escape)
- ✅ Copy button with visual feedback
- ✅ Automatic memory management
- ✅ Reusable scrolling component
- ✅ Visual polish with focus indicators
- ✅ Compilation errors fixed

## Overview
Implement horizontal scrolling for code blocks in the markdown renderer to handle long lines without wrapping, preserving code formatting and readability.

## Architecture Concepts that were used during implementation

### 1. Code Block Container Component
Create a new `CodeBlockContainer` struct that manages:
- Viewport width (from parent constraints)
- Content width (longest line in the code block)
- Horizontal scroll offset
- Mouse interaction state
- Scrollbar visibility and hover state
- Focus state for keyboard navigation

### 2. Rendering Strategy
Use **relative z-index layering** within the current rendering context:

```
Layer Stack (relative to parent z-index):
- Sub-layer 0: Code block background and border
- Sub-layer 1: Scrollable code content (clipped to viewport)
- Sub-layer 2: Horizontal scrollbar (when needed)
- Z-index +1: Copy button (floats above code block)
```

### 3. Content Measurement
Before rendering, measure all code lines to find the maximum width

### 4. Scrollbar Implementation

#### Shared Hover/Activity Behavior
Create a shared trait for auto-hiding scrollbars

#### Horizontal Scrollbar Rendering
Leverage the existing `ScrollbarRenderer` in horizontal mode

### 5. Copy Button Implementation

Add a copy button that appears just above the code block

### 6. Rendering Flow

1. **Measure Phase**:
   - Calculate max line width from syntax-highlighted segments
   - Determine if scrollbar is needed (content_width > viewport_width)
   - Reserve 8px height for scrollbar if needed

2. **Layout Phase**:
   - Create container element with fixed width (viewport)
   - Apply negative left margin based on scroll_offset
   - Render each line as non-wrapping text (ElementContent::Text)

3. **Scrollbar Phase**:
   - Position below code content
   - Apply opacity based on hover/activity state
   - Render using sidebar styling
   - Register UIItems for mouse interaction

4. **Copy Button Phase**:
   - Position at top-right, just above the code block, with slight margin
   - Handle click to copy code to clipboard

### 7. Mouse Event Handling

### 8. Keyboard Support

When a code block has focus:
- **Left/Right arrows**: Scroll horizontally by ~50 pixels
- **Home/End**: Jump to start/end of longest line
- **Shift+Wheel**: Convert vertical wheel to horizontal scroll
- **Escape**: Clear focus

### 9. Integration Points

1. **Markdown Renderer Changes**:
   - Replace direct code block rendering with CodeBlockContainer
   - Pass viewport width from parent's max_width constraint
   - Generate unique IDs for each code block
   - Track containers in a registry

2. **Sidebar State**:
   - Add `code_block_registry: HashMap<String, CodeBlockContainer>`
   - Clean up on content change
   - Update visibility states in render loop

3. **UIItem Registration**:
   - Add new UIItemType variants:
     ```rust
     UIItemType::CodeBlockScrollbar(String),  // ID
     UIItemType::CodeBlockContent(String),    // ID  
     UIItemType::CodeBlockCopyButton(String), // ID
     ```

### 10. Visual Design

- **Scrollbar**: Match sidebar style (thin, semi-transparent)
- **Auto-hide**: Fade in on hover/activity, fade out after 1.5s
- **Copy button**: Subtle, appears on hover
- **Focus indicator**: Subtle border highlight when focused
- **Overflow indicator**: Gradient fade on right edge when scrollable

### 11. Technical Considerations

1. **Performance**: Cache measured widths to avoid recalculation
2. **Memory**: Clean up containers when content changes
3. **Coordination**: Ensure only one code block has focus at a time
4. **Accessibility**: Ensure keyboard navigation is discoverable
5. **Edge cases**: Handle empty code blocks, very long lines

### 12. Success Criteria

1. Long code lines don't overflow the sidebar
2. Horizontal scrolling is smooth and responsive
3. Scrollbar appears only when needed and on activity
4. Copy button works reliably
5. Keyboard navigation is intuitive
6. No performance impact on markdown rendering
7. Visual style matches existing sidebar components

## Implementation Status

### Phase 1: Basic Structure ✅ COMPLETED
- ✅ Created `CodeBlockContainer` struct with all required fields
- ✅ Implemented width measurement using `unicode_column_width` (from termwiz)
- ✅ Added `CodeBlockRegistry` type alias (Arc<Mutex<HashMap<String, CodeBlockContainer>>>)
- ✅ Updated `MarkdownRenderer` to:
  - Track code block counter for unique IDs
  - Accept optional `max_width` parameter
  - Generate unique IDs for each code block (format: "code_block_{counter}")
  - Pass viewport width to code block rendering
- ✅ Added new public method `render_with_width()` for width-aware rendering
- ✅ Collected lines for measurement in `highlight_code_block`

### Phase 2: Scrollbar Rendering ✅ COMPLETED
- ✅ Added UIItemType variants:
  - `CodeBlockScrollbar(String)`
  - `CodeBlockContent(String)`
  - `CodeBlockCopyButton(String)`
- ✅ Updated mouse event handling in `mouseevent.rs`:
  - Added match arms for new UIItemType variants
  - Created full implementations for each interaction type
  - Set appropriate cursors (Arrow for scrollbar/button, Text for content)
- ✅ Verified `ScrollbarRenderer` supports horizontal mode via `new_horizontal()`
- ✅ Code blocks now tagged with `UIItemType::CodeBlockContent` for interaction
- ✅ Added NaN protection in width measurement
- ✅ Creating CodeBlockContainer instances with proper state management
- ✅ Actually render the scrollbar in code blocks using `horizontal_scroll` helper
- ✅ Implemented shared auto-hide behavior with opacity animation
- ✅ Position scrollbar below code content with proper spacing
- ✅ Implemented viewport clipping for scrolled content using negative margin

### Phase 3: Mouse Interaction ✅ COMPLETED
- ✅ Horizontal wheel scrolling (both native horizontal and Shift+vertical)
- ✅ Scrollbar dragging with proper drag offset calculation
- ✅ Click to focus (clears focus from other code blocks)
- ✅ Hover state tracking for both content and scrollbar

### Phase 4: Copy Button ✅ COMPLETED
- ✅ Render copy button above code block on hover
- ✅ Implemented clipboard integration with actual code extraction
- ✅ Extract actual code content from markdown
- ✅ Visual feedback on copy (checkmark for 2 seconds)

### Phase 5: Keyboard Support ✅ COMPLETED
- ✅ Focus management (click to focus, maintains focus state)
- ✅ Arrow key scrolling when focused
- ✅ Home/End navigation when focused
- ✅ Shift+wheel for horizontal scroll (already implemented)
- ✅ Escape to clear focus

### Phase 6: Polish ✅ COMPLETED
- ✅ Focus indicators (blue border when focused)
- ✅ Copy success feedback (checkmark animation)
- ✅ Auto-cleanup of code block registry when content changes

## Implementation Differences/Notes

1. **Width Measurement**: Using `unicode_column_width` from termwiz instead of a custom implementation, which is more accurate for terminal rendering. Added NaN protection in the fold operation.

2. **Renderer Structure**: Made `MarkdownRenderer` methods require `&mut self` to support the code block counter. This allows generating unique IDs without external state.

3. **UIItemType Integration**: Following the existing pattern where UIItemType variants store the ID string directly, not wrapped in a struct. Code blocks are now tagged with `UIItemType::CodeBlockContent`.

4. **Horizontal Scrollbar Implementation**: Created a `horizontal_scroll` module instead of using `ScrollbarRenderer` directly. Note that despite the generic API, this module is currently specific to code blocks due to hardcoded UIItemType usage.

5. **State Management**: Integrated `CodeBlockRegistry` into `AiSidebar` using `Arc<Mutex>` for thread safety (required by the `Sidebar` trait). The registry is passed to `MarkdownRenderer` when rendering to maintain scroll state across renders.

6. **Mouse Event Handling**: Implemented full mouse interaction including:
   - Scrollbar dragging handled through sidebar's existing drag detection
   - Horizontal scrolling with mouse wheel (native and Shift+vertical) with configurable speed constant
   - Click to focus with proper focus management
   - Hover state tracking for auto-hide behavior
   - Note: Hit testing for scrollbar clicks currently assumes UIItem bounds, needs proper coordinate transformation

7. **Auto-hide Behavior**: Implemented opacity animation based on hover state and activity. Scrollbars fade in/out smoothly with configurable timing.

8. **Viewport Clipping**: Used negative left margin on content to implement horizontal scrolling, with a fixed-width viewport container that clips overflow.

9. **Memory Management**: Added `clear_code_block_registry()` method and integrated automatic cleanup when activity log content changes to prevent unbounded memory growth.

## Debug Actions Taken

1. Added `update_code_block_opacity(0.016)` call in sidebar render
2. Added debug logging for code block measurements and opacity
3. Added a very long line to mock data to ensure scrolling is needed
5. Adjusted viewport width calculation to account for code block padding (24px)
6. Temporarily forcing scrollbar opacity to 1.0 when needed (for testing)
7. Discovered scrollbar was being rendered inside the padded code block container
   - Scrollbar was potentially hidden by padding constraints or positioned outside visible area
   - Restructured rendering to place scrollbar outside the padded container (which user later disliked)
8. Added extensive debug logging to track element creation and structure

## Latest Changes

- Reverted to rendering scrollbar inside code block container
- Fixed element stacking with track and thumb
- Discovered scrollbar sometimes renders but with major issues

## Root Cause Analysis of some issues

### **Width Calculation Issues**
- Viewport width may be calculated incorrectly
- Content width measurement might not account for all factors
- The padding adjustment may be applied incorrectly
- Max_width propagation through the element tree could be broken

### **Element Structure Problems**  
- Track and thumb layering isn't working correctly
- The negative margin approach for thumb positioning fails

### **Event Handling Breakdown**
- UIItemType registration for scrollbar might not have correct bounds
- Mouse events aren't reaching the code block handlers
- Registry lookup might be failing (container not found)
- Event propagation blocked by parent elements

### **Context-Specific Problems**
- Activity log vs suggestion modal have different event handling
- Different max_width values in different contexts
- Parent container differences affecting child rendering

## Fixes Applied (What Actually Works)

1. **UIItem extraction fixed** - Added `self.ui_items.extend(activity_log_computed.ui_items())` ✅
2. **Copy button always visible** - Changed from hover-only to always visible ✅
3. **Scrolling works in activity log** - Thumb drag and shift+wheel functional ✅

## Failed Fixes (Need Rework)

1. **Scrollbar styling** - Still wrong despite changes ❌
2. **Width calculation** - Buffer not working for borderline cases ❌
3. **Modal integration** - No horizontal scrollbars at all ❌
4. **Vertical scroll capture** - Still broken when over code blocks ❌


## Priority Fix Order (Remaining Issues)

1. **Test Scrolling Implementation** ✅ FIXED
   - Verify that content now scrolls with thumb movement
   - Check that scroll offset is properly applied
   - Ensure smooth scrolling experience

2. **Fix Clipping/Overflow** (CRITICAL - Next Priority)
   - Content will likely overflow onto terminal area
   - Need proper viewport clipping at render level
   - May require implementing scissor rect or modifying z-layer approach

3. **Fix Modal Scrolling** (CRITICAL)
   - Modal needs to use render_with_registry
   - Fix vertical scrolling in modal
   - Ensure proper event capture

4. **Fix Mouse Wheel Behavior** (Medium Priority)
   - Currently captures all scroll events over code blocks
   - Should only capture horizontal scroll when Shift is pressed
   - Requires architectural changes to UI item event handling

5. **Enable Text Selection** (Complex - Lower Priority)
   - Currently blocked by UI item event handling
   - Affects all sidebar content, not just code blocks
   - Investigate UIItem event handling
   - May need to selectively disable event capture