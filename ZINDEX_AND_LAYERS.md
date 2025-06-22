# WezTerm Z-Index and Layer System Documentation

## Overview

WezTerm uses a two-level rendering system that can be confusing at first. This document clarifies how it works based on code analysis.

## The Two-Level System

### Level 1: Z-Index (RenderLayer)
- **Purpose**: Determines rendering order between different UI components
- **Range**: Any `i8` value (-128 to 127)
- **Created dynamically**: New `RenderLayer` objects are created as needed
- **Code**: `renderstate.rs:723` - `layer_for_zindex(zindex: i8)`

### Level 2: Sub-Layers (within each RenderLayer)
- **Purpose**: Separates content types within a single z-index
- **Count**: Exactly 3 sub-layers per z-index (hardcoded)
- **Indices**: 0, 1, 2 only
- **Code**: `renderstate.rs:556` - `pub vb: RefCell<[TripleVertexBuffer; 3]>`

## How It Works

### 1. Z-Index Creates RenderLayers
```rust
// renderstate.rs:723-742
pub fn layer_for_zindex(&self, zindex: i8) -> anyhow::Result<Rc<RenderLayer>> {
    // Checks if layer exists, creates if not
    // Keeps layers sorted by zindex for rendering order
}
```

### 2. Each RenderLayer Has Fixed Sub-Layers
```rust
// quad.rs:339-344 (HeapQuadAllocator::allocate)
match layer_num {
    0 => &mut self.layer0,
    1 => &mut self.layer1,
    2 => &mut self.layer2,
    _ => unreachable!(),  // PANICS if > 2
}

// renderstate.rs:664-671 (BorrowedLayers)
fn allocate(&mut self, layer_num: usize) -> anyhow::Result<QuadImpl> {
    self.layers[layer_num].allocate()  // Array access - panics if > 2
}
```

### 3. Sub-Layer Usage Convention
- **Sub-layer 0**: Backgrounds, underlines, block cursors
- **Sub-layer 1**: Text glyphs
- **Sub-layer 2**: Sprites, UI elements, bar cursors

## Code Examples

### Terminal Rendering (z-index 0)
```rust
// paint.rs:196-199
let layer = gl_state.layer_for_zindex(0)?;
let mut layers = layer.quad_allocator();

// paint.rs:254-265
self.filled_rectangle(&mut layers, 0, rect, background)?;  // Sub-layer 0 for background
```

### Element Rendering
```rust
// box_model.rs:905-951
let layer = gl_state.layer_for_zindex(element.zindex)?;
let mut layers = layer.quad_allocator();

// Different content types use different sub-layers:
let mut quad = layers.allocate(2)?;  // Sprites use sub-layer 2
let mut quad = layers.allocate(1)?;  // Glyphs use sub-layer 1
```

### Z-Index Inheritance
```rust
// box_model.rs:634
zindex: element.zindex + context.zindex,  // Elements inherit parent z-index
```

## Current Sidebar Implementation Issues

### Mixed Rendering Strategy
1. **Background** (sidebar_render.rs:344):
   ```rust
   self.filled_rectangle(layers, 2, sidebar_rect, sidebar_bg_color)?;
   ```
   - Uses passed-in `layers` from z-index 0
   - Renders on sub-layer 2

2. **Content** (sidebar_render.rs:405):
   ```rust
   zindex: 10,  // Element computation context
   ```
   - Elements start at z-index 10
   - Each element can add its own z-index offset

3. **Scrollbar** (scrollable.rs:468):
   ```rust
   .zindex(2);  // Final z-index = 10 + 2 = 12
   ```

This mixed approach causes issues because background and content are on different z-index levels.

## Constraints and Limitations

1. **Hard limit of 3 sub-layers**: Arrays and match statements enforce this
2. **Cannot use `layers.allocate(3)`**: Would panic with array out of bounds
3. **All content at same z-index shares 3 sub-layers**: Can cause ordering conflicts

## Recommendation: Z-Index Strategy

Instead of expanding to 9 sub-layers, use z-indices strategically:

### Proposed Z-Index Assignments
- **Z-index 0**: Terminal content (existing)
- **Z-index 1**: Tab bar (existing)
- **Z-index 2**: Left sidebar background
- **Z-index 3**: Left sidebar content
- **Z-index 4**: Right sidebar background
- **Z-index 5**: Right sidebar content
- **Z-index 6**: Scrollbars
- **Z-index 7**: Modal overlays
- **Z-index 8**: Tooltips
- **Z-index 9**: Drag previews

### Implementation Changes

1. **Fix sidebar background rendering**:
   ```rust
   // Instead of:
   self.filled_rectangle(layers, 2, sidebar_rect, color)?;
   
   // Use:
   let bg_layer = gl_state.layer_for_zindex(4)?;
   let mut bg_layers = bg_layer.quad_allocator();
   self.filled_rectangle(&mut bg_layers, 0, sidebar_rect, color)?;
   ```

2. **Adjust element base z-index**:
   ```rust
   // sidebar_render.rs - change from:
   zindex: 10,
   // to:
   zindex: 5,
   ```

3. **Adjust scrollbar z-index**:
   ```rust
   // scrollable.rs - change from:
   .zindex(2)
   // to:
   .zindex(1)  // Will become 5+1=6 after inheritance
   ```

## Benefits of This Approach

1. **No core changes needed**: Uses existing z-index system
2. **Clear separation**: Each UI layer gets its own RenderLayer
3. **Predictable ordering**: No conflicts within a z-index
4. **Performance**: No additional overhead
5. **Extensible**: Can add more z-indices as needed

## Long-Term Scrollbar Fix

1. **Immediate**: Implement the z-index strategy above
2. **Refactor**: Convert scrollbar from Element-based to direct rendering at z-index 6
3. **Reusable**: Create `ScrollbarRenderer` component that any scrollable area can use
4. **Performance**: Direct rendering avoids Element tree overhead

This approach solves the current issues without the complexity of expanding the sub-layer system.