# Modal Conversion to Scissor Rect Clipping

## Overview

This document outlines the plan to convert the suggestion card modal from the current "cut a hole" scrolling approach to the new hardware-accelerated scissor rect clipping implementation.

## Current Implementation Analysis

### Z-Index Structure
- **Z-index 20**: Base dimmer layer over sidebar
- **Z-index 21**: Modal shadow and scrollable content area
- **Z-index 22**: Modal frame sections (top, bottom, left, right edges) that create the "hole"
- **Z-index 23**: Scrollbar (renders above everything)

### Problems with Current Approach
1. **Complex z-index management**: Requires careful coordination of multiple layers
2. **Visual artifacts**: Frame sections can sometimes be visible as seams
3. **Performance overhead**: Multiple overlapping elements at different z-indices
4. **Maintenance complexity**: Easy to break when adding new features
5. **Hacky scroll implementation**: Uses negative margins to offset content

## Proposed Scissor Rect Implementation

### New Z-Index Structure
- **Z-index 20**: Modal structure (dimmer, background, border, header, footer)
- **Z-index 21**: Scrollable content (with scissor rect clipping) - DEDICATED z-index
- **Z-index 23**: Scrollbar (unchanged - intentionally above clipped content)

**CRITICAL**: Z-index 21 must be used EXCLUSIVELY for modal scrollable content. Any other UI elements at this z-index will also be clipped by the scissor rect since clipping applies to the entire layer.

### Visual Structure
```
┌─────────────────────┐ <- Modal border (z-index 20)
│ Modal Header        │ <- Header (z-index 20)
├─────────────────────┤
│ ┌─────────────────┐ │ <- Scissor rect boundary
│ │ Scrollable      │ │ <- Content (z-index 21, clipped)
│ │ Content Area    │ │
│ └─────────────────┘ │
└─────────────────────┘ <- Modal background (z-index 20)
```

### Implementation Steps

#### 1. Remove Frame Blocking Elements
- Delete the 4 frame sections at z-index 22 (top, bottom, left, right)
- These were added in commit `495018f9e` to prevent overflow but won't be needed

#### 2. Consolidate Modal Structure
- Merge z-index 20 and 21 elements into a single layer (z-index 20)
- This includes: dimmer, modal background, border, header, footer
- These elements are NOT clipped and provide the visual structure

#### 3. Apply Scissor Rect to Content
```rust
// Calculate content viewport (modal bounds minus header/footer/padding)
let content_viewport = euclid::rect(
    modal_bounds.origin.x + padding,
    modal_bounds.origin.y + header_height + padding,
    modal_bounds.size.width - (padding * 2.0),
    modal_bounds.size.height - header_height - footer_height - (padding * 2.0)
);

// Apply scissor to scrollable content container
let content_container = Element::new(&fonts.body, ElementContent::Children(items))
    .zindex(21)  // Dedicated z-index for scrollable content
    .with_layer_scissor(content_viewport);
```

#### 4. Update Scroll Implementation
- Remove negative margin offset hack
- Use absolute positioning for content items:
```rust
// Position items based on scroll offset
for (idx, item) in visible_items.iter().enumerate() {
    let y_position = (idx as f32 * item_height) - scroll_offset;
    let item_element = render_item(item)
        .position(Position::Absolute)
        .top(Dimension::Pixels(y_position));
    content_container.add_child(item_element);
}
```

#### 5. Clean Up
- Remove any code related to the "cut a hole" approach
- Simplify the render_modal method
- Update comments and documentation

### Benefits

1. **Simpler implementation**: Fewer elements and z-indices to manage
2. **Better performance**: Hardware-accelerated GPU clipping
3. **No visual artifacts**: Clean clipping without frame seams
4. **Easier maintenance**: Straightforward clipping model
5. **Cleaner scroll handling**: No margin offset hacks

### Testing Plan

1. **Verify scissor rect is working**: Create a simple test element with scissor clipping
2. Verify modal appears correctly with background and border
3. Test scrolling works smoothly without visual glitches
4. Ensure content is properly clipped at viewport boundaries
5. Check that scrollbar remains visible and functional
6. Test with different modal sizes and content amounts
7. Verify text selection still works within the modal
8. Test on both WebGPU and OpenGL backends
9. **Verify no other UI elements use z-index 21**: Check that the modal's dedicated z-index doesn't conflict

### Migration Notes

- **CRITICAL**: The scissor rect applies to ALL elements at the specified z-index across the entire window
- Z-index 21 must be dedicated exclusively to modal scrollable content
- Ensure interactive elements (buttons, links) within the scrollable area use z-index 21 to be clipped
- Modal structure elements (background, border, header) must use z-index 20 to avoid clipping
- The scrollbar must remain at z-index 23 to avoid being clipped
- Virtual rendering (only creating elements for visible items) should still be used for performance

### Known Limitations

1. **Per-layer clipping**: Scissor rects apply to entire z-index layers, not individual elements
2. **Coordinate systems**: WebGPU uses top-left origin, OpenGL uses bottom-left (handled in implementation)
3. **No nested clipping**: Cannot have multiple scissor regions at the same z-index

### Implementation Verification Steps

Before starting the modal conversion:
1. Create a simple test case with a colored rectangle at a dedicated z-index
2. Apply scissor clipping to verify it works as expected
3. Test on both WebGPU and OpenGL backends
4. Verify clipping boundaries are pixel-perfect
5. Only proceed with modal conversion after basic functionality is confirmed