# WezTerm Rendering Pipeline

## Critical Implementation Notes

### Sub-Layer Constraints (WILL HIT UNREACHABLE)
```rust
// ONLY valid sub-layers: 0, 1, 2
// Sub-layer 0: Backgrounds, underlines, filled rectangles  
// Sub-layer 1: Text glyphs
// Sub-layer 2: Sprites, UI elements, cursors
// Using sub-layer > 2 hits unreachable!() - hardcoded in HeapQuadAllocator::allocate()
```
**Type**: Current implementation constraint (could be changed but would require allocator rewrite)

### Two-Phase Rendering (ARCHITECTURAL REQUIREMENT)
WezTerm's rendering is fundamentally two-phase:
1. **Element Processing**: Build vertex buffers, no GPU operations
2. **GPU Drawing**: Draw entire layers in single calls

**Never attempt to draw during element processing** - GPU state doesn't persist between phases.
**Type**: Architectural requirement (fundamental to batched rendering design)

### Per-Layer Scissor Rect Constraints (ARCHITECTURAL REQUIREMENT)
Scissor rect clipping applies to ENTIRE z-index layers, not individual elements:
- **Dedicate z-indices**: Reserve specific z-indices exclusively for scissor-clipped content
- **No sharing**: All elements at a scissor-clipped z-index will be clipped
- **One rect per layer**: Only one scissor rect can apply to each z-index
**Type**: Architectural requirement (GPU scissor state applies per render pass)

### ClipBounds::Explicit (CAUSES CRASHES)  
**Why it fails**: Causes RefCell BorrowMutError due to nested borrows in quad allocator.
**Type**: Current implementation bug (could potentially be fixed)

### Performance Rules
- **Each unique z-index = separate GPU draw call**
- Too many z-indices causes: performance issues, RefCell borrow conflicts
- **Rule**: Group elements at same z-index, use sub-layers for separation
**Type**: Architectural design (intentional for performance)

## Overview

WezTerm uses a sophisticated GPU-accelerated rendering pipeline that batches draw calls for performance. Understanding this pipeline is crucial for implementing UI features correctly.

## Rendering Architecture

### Two-Phase Process

1. **Element Processing Phase**
   - `render_element()` called recursively on Element tree
   - Elements allocate quads to vertex buffers
   - Buffers organized by z-index and sub-layer
   - No actual GPU drawing occurs yet

2. **GPU Drawing Phase**
   - Iterate through z-indices in order
   - Each z-index layer drawn with single GPU call
   - State (like scissor rects) applies per draw call, not per element

### Z-Index System

#### Two-Level Hierarchy
1. **Z-Index** (`i8` values, -128 to 127)
   - Creates separate `RenderLayer` objects
   - Determines draw order between UI components
   - Each unique z-index = separate GPU draw call

2. **Sub-Layers** (exactly 3 per z-index, hardcoded)
   - 0: Backgrounds, underlines, filled rectangles
   - 1: Text glyphs  
   - 2: Sprites, UI elements, cursors

See `renderstate.rs:layer_for_zindex()` and `quad.rs:HeapQuadAllocator::allocate()`.

#### Z-Index Assignments
- **Z-index 0**: Terminal content
- **Z-index 1**: Tab bar
- **Z-index 10**: Right sidebar background
- **Z-index 12**: Right sidebar activity log content (with scissor rect)
- **Z-index 14**: Right sidebar main content
- **Z-index 16**: Right sidebar scrollbars(s) and buttons
- **Z-index 20**: Right sidebar overlays (e.g. modals)
- **Z-index 21**: Right sidebar modal scrollable content (with scissor rect)
- **Z-index 23**: Right sidebar modal scrollbars
- **Z-index 30**: Left sidebar content for scrolling
- **Z-index 32**: Left sidebar background and main content (shared layer)
- **Z-index 36**: Left sidebar toggle button and scrollbars
- **Z-index 38**: Left sidebar overlays  (e.g. modals)
- **Z-index 40**: Left sidebar overlay content within overlays (such as sidebars within overlays)


### Element Box Model

The Element system (`termwindow/box_model.rs`) provides CSS-like layout:

```rust
struct Element {
    item_type: Option<UIItemType>,
    vertical_align: VerticalAlign,
    zindex: i8,
    display: DisplayType,
    float: Float,
    padding: BoxDimension,
    margin: BoxDimension,
    border: BoxDimension,
    border_corners: Option<Corners>,
    colors: ElementColors,
    hover_colors: Option<ElementColors>,
    font: Rc<LoadedFont>,
    content: ElementContent,
    presentation: Option<Presentation>,
    line_height: Option<f64>,
    max_width: Option<Dimension>,
    min_width: Option<Dimension>,
    min_height: Option<Dimension>,
    clip_bounds: Option<ClipBounds>,
}
```

Key types:
- `ElementContent::Text` - Single-style text
- `ElementContent::WrappedText` - Multi-line wrapped text
- `ElementContent::StyledWrappedText` - Text with style spans
- `ElementContent::Children` - Container for nested elements

### GPU Backends

#### WebGPU (Primary)
- Modern GPU API
- Better performance
- Explicit render passes

#### OpenGL (Fallback)
- Legacy support
- Simpler API
- Implicit state management

Backend selection in `config.front_end` setting.

## Clipping and Overflow

### Per-Layer Scissor Rect (IMPLEMENTED)

WezTerm now supports hardware-accelerated scissor rect clipping at the z-index layer level:

```rust
// Apply scissor rect to all elements at a specific z-index
element.with_layer_scissor(viewport_rect).zindex(21)
```

**Key characteristics:**
- Scissor rect applies to ALL elements at the same z-index
- Hardware-accelerated GPU clipping (zero performance cost)
- Proper coordinate handling for WebGPU (top-left) and OpenGL (bottom-left)
- Must dedicate entire z-index to scissor-clipped content

**Implementation details:**
- `RenderLayer` has `scissor_rect: RefCell<Option<Rect>>` field
- Elements mark scissor contribution via `element.with_layer_scissor(viewport_rect)`
- During render_element(), scissor bounds propagate to the layer
- GPU drawing phase applies scissor rect per render pass:
  - WebGPU: `render_pass.set_scissor_rect()` with top-left origin
  - OpenGL: `glium::DrawParameters { scissor: Some(rect) }` with Y-flip for bottom-left
- Frame state cleared via `render_state.clear_frame_state()` each frame

**Usage for scrollable content:**
1. Reserve a dedicated z-index for scrollable content (e.g., z-index 21)
2. Apply scissor rect with viewport bounds
3. Use negative margin for scroll offset - GPU clips overflow
4. Scrollbar must use higher z-index to avoid being clipped

**Example:**
```rust
let viewport = euclid::rect(x, y, width, height);
let content = Element::new(&fonts.body, ElementContent::Children(items))
    .zindex(21)  // Dedicated z-index for this scrollable region
    .with_layer_scissor(viewport)
    .margin(BoxDimension {
        top: Dimension::Pixels(-scroll_offset),
        ..Default::default()
    });
```

### Legacy Approaches (Avoid)

1. **Cut-a-Hole Pattern** (Deprecated)
   - Complex: Requires frame elements at higher z-index
   - Visual artifacts: Seams between frame sections
   - Performance: Multiple overlapping elements
   - Still used in some older components

2. **Manual Clipping** (Ineffective)
   - Only clips individual glyphs/sprites
   - Doesn't prevent element expansion
   - Other z-indices still render in "clipped" area

3. **Explicit ClipBounds** (Broken)
   - Causes RefCell borrow conflicts
   - Incompatible with current architecture

## Special Effects

### GPU Blur System

For neon glow effects on UI elements:

1. Rasterize glyph/icon to texture with padding
2. Two-pass Gaussian blur (horizontal, vertical)
3. Additive blend at original position

Files:
- `termwindow/render/blur.rs` - Blur renderer
- `termwindow/render/effects_overlay.rs` - Effects layer
- `shaders/blur.wgsl`, `blur-*.glsl` - GPU shaders

Configuration:
- Blur radius: up to 15-16 pixels
- LRU cache: 50MB limit
- Automatic WebGPU/OpenGL selection

### Animation System

Professional animations via `ColorEase`:
- Multiple easing functions
- In/out durations
- Frame rate control (`animation_fps`)
- GPU uniform integration

See `colorease.rs` for implementation.

## Performance Considerations

### Batching is Critical
- Minimize z-index count (each = draw call)
- Group related elements at same z-index
- Use sub-layers for content separation

### Vertex Buffer Management
- Pre-allocated buffers per layer
- Dynamic growth when needed
- RefCell borrows must be carefully managed

### Texture Atlas
- Glyphs cached after first render
- LRU eviction at size limits
- Automatic growth up to GPU limits

## Common Pitfalls

1. **Too Many Z-Indices**
   - Each creates render pass
   - Increases draw calls
   - Can cause borrow conflicts

2. **Assuming Immediate Rendering**
   - Elements don't draw when processed
   - State must persist to draw phase
   - Can't modify GPU state per-element

3. **Ignoring Sub-Layer Rules**
   - Only 0, 1, 2 valid
   - Hardcoded in allocator
   - Will hit unreachable!() if > 2

4. **RefCell Borrow Conflicts**
   - Nested element processing
   - Simultaneous buffer access
   - More layers = more risk

## Debugging Tips

- `WEZTERM_LOG=trace` - Verbose rendering logs
- `RUST_BACKTRACE=1` - For panic traces
- Check z-index assignments for overlaps
- Monitor draw call count in profiler
- Verify sub-layer usage (0-2 only)

## Height Extraction for Virtual Scrolling

After elements are computed, their actual rendered heights are available:
```rust
// In update callback after render_element()
let rendered_height = computed_element.border_rect.size.height;
// Includes padding + border, but NOT margin
```

Key points:
- `border_rect` provides full height even for clipped elements
- Heights available in viewport-relative coordinates
- Cache immediately for any visible item (partial or full)