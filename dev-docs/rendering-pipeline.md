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

### Clipping Approaches That Don't Work Reliably

#### GPU Scissor Rects (BROKEN)
**Why it fails**: Scissor state applies per render pass, not per element. Each z-index creates a new render pass, resetting GPU state.
**Type**: Architectural limitation (would require complete rendering rewrite)

#### ClipBounds::Explicit (CAUSES CRASHES)  
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
- **Z-index 10**: Right sidebar activity log content
- **Z-index 12**: Right sidebar background
- **Z-index 14**: Right sidebar main content
- **Z-index 16**: Right sidebar scrollbars(s) and buttons
- **Z-index 20**: Right sidebar overlays (e.g. modals)
- **Z-index 22**: Right sidebar overlay content within overlays (such as sidebars within overlays)
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

### Current Limitations

The Element system lacks true overflow containers:
- No equivalent to CSS `overflow: hidden`
- Parent bounds don't constrain children
- Elements expand to contain all content

### Failed Clipping Approaches

Based on extensive testing (see HORIZONTAL_SCROLL_ATTEMPTS.md):

1. **GPU Scissor Rects** (Failed)
   - Timing mismatch: scissor set during element phase, used in draw phase
   - State doesn't persist across z-indices
   - Each render pass resets GPU state

2. **Manual Clipping** (Ineffective)
   - Can clip individual glyphs/sprites
   - Doesn't prevent element expansion
   - Other z-indices still render in "clipped" area

3. **Explicit ClipBounds** (Causes crashes)
   - Causes RefCell borrow conflicts
   - Nested borrows in quad allocator
   - Currently incompatible with architecture

### Recommended Approaches

1. **Cut-a-Hole Pattern** (Currently Used)
   - Render content at lower z-index
   - Render background at higher z-index with holes
   - Content shows through holes
   - Used in sidebar scrollable regions

2. **Render-to-Texture** (Future)
   - Render scrollable content to off-screen texture
   - Display texture with UV scrolling
   - True GPU-level clipping
   - Enables caching and effects

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