# Activity Log Text Selection - Implementation Progress and Status

## Overview

This document tracks the implementation of text selection in WezTerm's activity log sidebar. The goal is to enable pixel-perfect text selection with proper copy functionality for all content types (user messages, AI messages with markdown, command output, and code blocks).

## Current Status (January 2025 - Session 5)

### What's Working
- **Goal card selection** - Works perfectly with accurate positioning and copy
- **Basic selection infrastructure** - Mouse events, drag handling, and selection state management
- **Multi-line selection support** - Creates rectangles for each line in selection
- **Copy functionality** - Works correctly when selection aligns properly (which doesn't always happen)
- **Mouse hit detection** - Correctly converts mouse coordinates to line positions

### What's Not Working
- **Selection rectangle rendering** - Rectangles appear in wrong viewport positions:
  - Incorrect Y calculations cause rectangles to be offset from text
  - When scrolled, rectangles can have negative Y coordinates (off-screen)
  - The offset gets progressively worse deeper into complex markdown
- **Mouse hit detection in long markdown** - Incorrectly converts mouse coordinates to line positions deeper into markdown content
- **Coordinate system confusion** - Using generic "bounds" terminology makes code hard to understand
- **X-offset issue** - Cannot select first ~2 characters (CHAT_ITEM_PADDING not accounted for?)
- **Command output** - Cannot be selected (position extraction may not reach nested elements)
- **Code blocks** - Cannot be selected within markdown

## Implementation Journey

### Completed Work

1. **Core Infrastructure** - Implemented `GlyphWithCluster` tracking for pixel-perfect positioning
2. **Bug Fixes** - Fixed Y-offset accumulation, line detection, copy functionality
3. **Coordinate Investigation** - Discovered that different content types use different coordinate spaces
4. **Failed Complex Solutions**:
   - Plan D: Added `CoordinateSystem` enum and `ActivityItemPositionData` struct
   - Added complex coordinate transformations that didn't fix the issue
   - Made code more complex without solving the root problem

### Session 5 Troubleshooting

Extensive debugging revealed the true nature of the coordinate system issues:

1. **Mouse coordinates are often correct**: The calculation `relative_y = mouse_y - bounds.origin.y` properly converts to item-relative coordinates except when deep in markdown
2. **Line positions are usually correct**: Stored as pixels from activity item top (e.g., line.y_position=672)
3. **There is a bug is in selection rectangle calculation**: Using `content_y - scroll_offset + line.y` instead of the simpler `bounds.origin.y + line.y`

### Key Discoveries

1. **Variable line heights**: Markdown has headings, paragraphs, code blocks with different spacing - we can't assume uniform line heights
2. **Virtual scrolling with negative margins**: Items above/below viewport are rendered with a buffer, entire content shifts with negative margin, items are added/removed from rendering (adjusting the margin) during scrolling
3. **Coordinate space confusion**: The term "bounds" is overloaded - sometimes viewport-relative, sometimes content-relative
4. **Activity item bounds ARE viewport-relative**: When scrolled, bounds.origin.y goes negative as items move above viewport

## Root Cause Analysis

The selection rectangle misalignment is likely caused by:

1. **Mixing coordinate systems**: Using `content_y` (content-space) with `scroll_offset` when `bounds.origin.y` is already viewport-relative
2. **Double transformation**: The bounds already include scroll transformation, but we're subtracting scroll_offset again
3. **Confusing terminology**: "bounds" doesn't clearly indicate which coordinate space we're in

## New Implementation Plan: Coordinate System Refactor

### Core Principles

1. **Two coordinate spaces only**:
   - **Item coordinates**: (0,0) at top-left of activity item, never changes with scrolling
   - **Viewport coordinates**: (0,0) at top-left of viewport, changes as content scrolls

2. **Clear terminology**: Replace generic "bounds" with specific "item_viewport_position"

3. **Simple transformations**: 
   - Item → Viewport: `viewport_pos = item_viewport_position + item_pos`
   - Viewport → Item: `item_pos = viewport_pos - item_viewport_position`

### Implementation Steps

#### Refactor and Fix Together

1. **Update data structures and naming**:
   ```rust
   // Change field from:
   activity_item_bounds: HashMap<usize, Rect<f32, PixelUnit>>
   
   // To (storing just position since we don't need the full rect):
   item_viewport_positions: HashMap<usize, Point<f32, PixelUnit>>
   ```

2. **Update all methods and usages**:
   ```rust
   // Methods:
   set_item_viewport_position(index: usize, position: Point<f32, PixelUnit>)
   get_item_viewport_position(index: usize) -> Option<&Point<f32, PixelUnit>>
   
   // Mouse coordinate conversion:
   let item_viewport_pos = self.get_item_viewport_position(index)?;
   let item_x = window_x - sidebar_x - item_viewport_pos.x;
   let item_y = window_y - item_viewport_pos.y;
   
   // Selection rectangle calculation:
   let item_viewport_pos = self.get_item_viewport_position(index)?;
   let selection_viewport_y = item_viewport_pos.y + line_info.y_position;
   ```

3. **Remove Plan D artifacts**:
   - Delete `CoordinateSystem` enum
   - Delete `ActivityItemPositionData` struct
   - Simplify back to `HashMap<usize, Vec<LinePositionInfo>>`

4. **Remove redundant tracking**:
   - Remove `activity_item_content_y` if viewport positions are sufficient
   - Clean up any double transformation logic

5. **Fix additional issues while refactoring**:
   - Add CHAT_ITEM_PADDING to selection rectangle X calculations
   - Ensure position extraction traverses all nested elements for command output
   - Verify syntax highlighting preserves position data for code blocks

### Testing Strategy

1. Test selection in user messages (simple structure)
2. Test selection in AI messages with complex markdown
3. Test selection while scrolled to various positions
4. Test selection across multiple lines
5. Verify rectangles stay aligned during scrolling

## Coordinate System Documentation

### Overview

The activity log uses two coordinate systems that work together to enable scrolling and interaction:

1. **Item Coordinates** - Position relative to an activity item's top-left corner (0,0)
2. **Viewport Coordinates** - Position relative to the viewport's top-left corner (0,0)

### How They Work Together

```
Window
├─ Sidebar (window coordinates)
│  ├─ Activity Viewport (fixed rectangle where content is visible)
│  │  ┌─────────────────────┐ ← Viewport (0,0)
│  │  │                      │
│  │  │  Activity Item #3    │ ← item_viewport_position = (10, 50)
│  │  │  ┌────────────────┐  │
│  │  │  │ Line 0: y=12   │  │ ← Item coord y=12
│  │  │  │ Line 1: y=40   │  │ ← Item coord y=40
│  │  │  └────────────────┘  │
│  │  │                      │
│  │  └─────────────────────┘
```

### Coordinate Transformations

```rust
// Item → Viewport (for rendering selection rectangles)
let selection_viewport_y = item_viewport_position.y + line_item_y;
let selection_viewport_x = item_viewport_position.x + line_item_x;

// Viewport → Item (for mouse hit testing)
let item_x = viewport_x - item_viewport_position.x;
let item_y = viewport_y - item_viewport_position.y;
```

### Virtual Scrolling and Coordinates

When scrolling occurs:
- **Item coordinates remain constant** - Line positions within an item never change
- **Viewport positions change** - Items move up/down, can have negative Y when above viewport
- **Negative margin** shifts all content up by scroll amount

Example with scrolling:
```
Before scroll:                    After scrolling down 100px:
item_viewport_position = (10, 50) item_viewport_position = (10, -50)
Line 0 viewport Y = 50 + 12 = 62  Line 0 viewport Y = -50 + 12 = -38 (above viewport)
```

### Key Rules

1. **Line positions** (`line.y_position`) are always in item coordinates
2. **Mouse events** arrive in window coordinates, convert to viewport then to item
3. **Selection rectangles** must be in viewport coordinates for rendering
4. **Item viewport positions** can be negative when scrolled above viewport
5. **Never mix coordinate systems** - always be explicit about which space you're in

### Common Patterns

```rust
// Getting item position for mouse handling
if let Some(item_viewport_pos) = self.get_item_viewport_position(index) {
    let item_x = mouse_x - sidebar_x - item_viewport_pos.x;
    let item_y = mouse_y - item_viewport_pos.y;
    // Now item_x, item_y are in item coordinate space
}

// Calculating selection rectangle position
if let Some(item_viewport_pos) = self.get_item_viewport_position(index) {
    let rect_viewport_x = item_viewport_pos.x + line_info.x_position + CHAT_ITEM_PADDING;
    let rect_viewport_y = item_viewport_pos.y + line_info.y_position;
    // Now rect is in viewport coordinate space, ready for rendering
}

// Checking if something is visible
let is_visible = rect_viewport_y >= 0.0 && rect_viewport_y < viewport_height;
```

## Next Steps

1. **Implement Phase 1** - Quick fix for selection rectangles using viewport-relative bounds
2. **Test the fix** - Have the user verify it resolves the misalignment for all content types
3. **Refactor terminology** - Make coordinate systems crystal clear
4. **Clean up code** - Remove complex transformations and redundant tracking

## Lessons Learned

1. **Clear terminology matters** - "bounds" was too generic and caused confusion
2. **Potentially trust (but verify) existing calculations** - The viewport-relative bounds may be correct
3. **Virtual scrolling is complex** - But the complexity may already be handled in the rendering layer
4. **Line heights vary** - Can't assume uniform spacing in rich content
5. **Coordinate spaces must be explicit** - Always be clear about which space you're in