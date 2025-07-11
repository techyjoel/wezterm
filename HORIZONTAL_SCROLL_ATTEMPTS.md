# Horizontal Scrolling Implementation Attempts - Complete History

> **Note**: The critical findings from this document have been incorporated into `dev-docs/rendering-pipeline.md` under the "Critical Implementation Notes" section. See that file for the authoritative guidance on clipping approaches.

> **Note2** This file was made before we had implemented scissor rect improvements that enable it per-z-layer.

## Overview

This document provides a comprehensive history of all attempts to implement horizontal scrolling for code blocks in the CLiBuddy Terminal sidebar. The goal was to allow users to horizontally scroll through wide code blocks in the activity log and suggestion card modals, preventing content truncation while maintaining proper visual boundaries.

## Summary of Implementation Status

**Current Status As Of Commit 5d0cdcf458599243f3b9c8e73852d57770387f6d**: MECHANICALLY WORKING, VISUALLY BROKEN ⚠️

- ✅ Scrollbar mechanics function correctly (drag, shift+wheel)
- ✅ Scroll state management works per code block
- ❌ Content visually overflows boundaries when scrolled horizontally
- ❌ Text selection impossible in sidebar elements

## Attempted Solutions

### 1. GPU Scissor Rect Implementation ❌ (ATTEMPTED AND REMOVED)

**Approach**: Implement GPU-level scissor rectangles to clip content at hardware level.

**Implementation Details**:
- Added `scissor_stack: RefCell<Vec<RectF>>` to `RenderState`
- Implemented stack-based scissor rect management (push/pop operations)
- Added OpenGL scissor implementation: `glScissor()` calls
- Added WebGPU scissor implementation: `render_pass.set_scissor_rect()`
- Modified `render_element` to push scissor for elements with clip bounds

**Why It Failed**:
1. **Timing Mismatch**: WezTerm uses a batched rendering pipeline where:
   - Element processing phase: Elements processed recursively, quads allocated to buffers
   - GPU drawing phase: Entire layers drawn in single draw calls
   - Scissor state set during element processing but GPU draws happen later
   - By draw time, scissor stack is empty

2. **Per-Layer Rendering**: Each z-index creates a separate render pass:
   - One scissor rect per draw call, not per element
   - Different z-indices = different render passes
   - Scissor state doesn't persist across passes

**Outcome**: Complete removal of GPU scissor infrastructure after proving architecturally incompatible.

**Detailed Analysis**: The core issue is that scissor state is applied PER RENDER PASS, not globally:
- In WebGPU: Each layer creates a new render pass with `encoder.begin_render_pass()`
- In OpenGL: Each layer is drawn with a separate draw call
- The scissor rect is applied inside each render pass/draw call
- When a new render pass starts for the next layer, the GPU scissor state from the previous pass is lost
- Since each unique z-index creates a separate layer with its own render pass, scissor state cannot persist across z-indices

### 2. Manual Clipping with Partial Glyph Support ⚠️ (IMPLEMENTED but INEFFECTIVE)

**Approach**: Manually check bounds and clip content during element rendering before creating quads.

**Implementation Details**:
```rust
// In render_element() for sprites and glyphs:
if let Some(ref clip_bounds) = element.clip_bounds {
    // Check if content is within bounds
    if sprite_left >= clip_bounds.max_x() || sprite_right <= clip_bounds.min_x() {
        continue; // Skip entirely out of bounds
    }
    
    // Handle partial clipping at boundaries
    if sprite_left < clip_bounds.min_x() || sprite_right > clip_bounds.max_x() {
        // Calculate visible portion
        // Adjust texture coordinates
        // Render only visible part
    }
}
```

**Current State**:
- Code is active in `box_model.rs` (lines 1308-1409)
- Includes coordinate system fixes (applied `left` offset)
- Supports partial glyph rendering with texture coordinate adjustment
- Proper bounds checking before creating quads

**Why It Doesn't Work**:
1. **Element System Limitations**:
   - Viewport element expands to contain all content despite max_width
   - No true "overflow: hidden" concept in Element system
   - Parent elements don't constrain child rendering bounds
   - Background/borders render at full size regardless of clipping

2. **Fundamental Architecture Issue**:
   - Manual clipping only affects individual glyphs/sprites
   - Doesn't prevent element itself from expanding
   - Other elements at different z-indices still render over "clipped" area

### 3. Explicit ClipBounds on Elements ❌ (CAUSES CRASHES)

**Approach**: Use WezTerm's built-in `ClipBounds::Explicit` mechanism.

**Implementation Details**:
- Set `clip_bounds: Some(ClipBounds::Explicit(rect))` on viewport element
- Let WezTerm's rendering system handle clipping

**Why It Failed**:
- **RefCell BorrowMutError**: Nested borrows in quad allocator
- `quad_allocator()` creates immutable borrow via `self.vb.borrow()`
- Allocating quads needs mutable borrow via `self.bufs.borrow_mut()`
- Unsafe lifetime extensions prevent proper cleanup
- More layers (z-indices) = more potential for nested borrows

**Outcome**: Cannot be used without major refactoring of WezTerm's core rendering.

### 4. Different Z-Index Strategies ⚠️ (ATTEMPTED)

**Approach**: Assign unique z-indices to code blocks to control rendering order.

**Implementation Details**:
- Assigned z-indices 50-69 to individual code blocks
- Hoped to isolate clipping per block

**Why It Made Things Worse**:
- Each z-index creates separate render layer
- More layers = more render passes
- Scissor state can't work across passes
- Increased RefCell borrow conflicts
- Defeated any chance of GPU-level clipping

## Fundamental Issues Discovered

### 1. WezTerm's Batched Rendering Architecture

```
Element Processing Phase:
├── render_element() called recursively
├── Each element allocates quads to vertex buffers
├── Buffers organized by z-index and sub-layer (0-2)
└── Immutable borrows held during entire tree processing

GPU Drawing Phase:
├── Iterate through layers by z-index
├── Each layer draws entire vertex buffer
├── One draw call per layer
└── Draw parameters (like scissor) apply to entire call
```

### 2. The Sub-Layer System

Each z-index has exactly 3 sub-layers (hardcoded):
- Sub-layer 0: Backgrounds, underlines, filled rectangles
- Sub-layer 1: Text glyphs
- Sub-layer 2: Sprites, UI elements, cursors

This separation prevents coherent clipping across content types.

### 3. No True Overflow Container

The Element system lacks a proper overflow container concept:
- No equivalent to CSS `overflow: hidden`
- Parent bounds don't constrain children
- Elements expand to contain all content
- Manual clipping affects rendering but not layout

## What Actually Works

### Successfully Implemented Features ✅

1. **Horizontal Scrollbar Mechanics**:
   - Thumb drag works correctly
   - Shift+wheel scrolling functions
   - Proper thumb sizing based on content ratio
   - Fade in/out animations (100ms in, 75ms out)

2. **Scroll State Management**:
   - Unique IDs per code block prevent state sharing
   - Scroll position persists during sidebar lifetime
   - Separate states for activity log vs modals

3. **Content Measurement**:
   - Accurate width calculation for code blocks
   - Proper detection of when scrolling needed
   - 5px buffer for borderline cases

4. **Mouse Event Routing**:
   - Vertical scroll pass-through works
   - Events properly forwarded to sidebar
   - Scrollbar interaction doesn't interfere with other UI

## Future Implementation Options

### 1. Render-to-Texture Approach (Most Promising) 🎯

**Technical Overview**:
- Allocate dedicated texture for each scrollable element
- Texture size = full content dimensions (uncropped)
- Use `TextureUsages::RENDER_ATTACHMENT | TEXTURE_BINDING`
- Render element into texture via off-screen RenderPass
- GPU automatically clips to texture boundaries
- At composite time, treat as textured quad with UV scrolling

**Implementation Steps**:
1. Create texture large enough for full content
2. Set up off-screen render pass targeting texture
3. Render element normally (no clipping needed)
4. In main pass, draw textured quad with:
   - Position: visible viewport bounds
   - UVs: scrolled portion of texture
5. Single Sampler (nearest-neighbor) preserves glyph sharpness

**Advantages**:
- Automatic GPU clipping at texture boundaries
- Reusable snapshots (only re-render on content change)
- Works with existing rendering pipeline
- No RefCell borrow issues
- Perfect for text selection (can map texture coords back)

**Resource Management**:
- Purge textures when scrolled off-screen
- Drop and regenerate on resize/content change
- Share shader modules with main renderer
- LRU cache for texture reuse

### 2. CSS-Style Overflow Container Element

**Concept**: New `ElementContent::OverflowContainer` variant

**Features**:
- True bounds enforcement at container level
- Render children to intermediate buffer first
- Blit only visible portion to screen
- Built-in scroll state management

**Challenges**:
- Significant Element system refactoring
- Performance impact of intermediate rendering
- Complex integration with existing layout

### 3. Immediate Mode Rendering for Scrollables

**Concept**: Bypass batched rendering for scrollable content

**Implementation**:
- Extract scrollable elements from normal tree
- Render in separate pass with immediate scissor
- Draw directly without batching

**Trade-offs**:
- Avoids architectural conflicts
- More draw calls impact performance
- Breaks element tree abstraction

### 4. Virtual Scrolling (Current Workaround)

**Concept**: Only render visible lines

**Implementation**:
- Calculate visible line range from scroll offset
- Create elements only for visible content
- Skip rendering of off-screen lines

**Limitations**:
- Can't show partial lines at boundaries
- Jarring scroll experience
- Not true horizontal scrolling

## Text Selection Challenge

### Why Selection Doesn't Work

The Element system is purely visual:
1. Text → Glyphs → Quads (one-way transformation)
2. No retained text data after rendering
3. No mapping from screen coordinates to text
4. Glyphs are just textured rectangles

### Selection Solutions (Synergistic with Clipping)

**Best Option: Parallel Text Mapping**
```rust
// During clipping pass, build selection map
for glyph in glyphs {
    if glyph_visible_after_clipping {
        text_map.add_glyph(
            char_index,
            screen_bounds,  // Only visible area
            original_text
        );
    }
}
```

This approach:
- Builds map during same iteration as clipping
- Only tracks selectable (visible) text
- Natural synergy with manual clipping
- Single pass through content

## Lessons Learned

1. **Architecture Matters**: GPU-level solutions must align with rendering pipeline
2. **Timing is Critical**: State set during processing != state during drawing  
3. **Borrowing Complexity**: RefCell patterns in performance-critical code are fragile
4. **Abstraction Limits**: Element system wasn't designed for clipping
5. **Z-Index Separation**: More layers make problems worse, not better

## Recommendation

Based on extensive testing and analysis:

1. **Short Term**: Abandon horizontal scrolling, implement line wrapping
   - Proven to work (already implemented for other text)
   - No clipping issues
   - Enables focus on text selection

2. **Long Term**: If horizontal scrolling needed, use render-to-texture
   - Most architecturally sound approach
   - Avoids all discovered issues
   - Enables additional features (caching, effects)

## Files Modified During Attempts

- `wezterm-gui/src/termwindow/renderstate.rs` - GPU scissor stack (removed)
- `wezterm-gui/src/termwindow/render/draw.rs` - GPU scissor application (removed)
- `wezterm-gui/src/termwindow/box_model.rs` - Manual clipping (active but ineffective)
- `wezterm-gui/src/sidebar/components/horizontal_scroll.rs` - Core scrolling container
- `wezterm-gui/src/sidebar/components/markdown.rs` - Code block integration
- `wezterm-gui/src/sidebar/components/activity_log.rs` - Scroll state management
- `wezterm-gui/src/sidebar/components/modal/suggestion_modal.rs` - Modal integration

## Conclusion

While horizontal scrolling mechanics work perfectly, the visual clipping problem stems from fundamental architectural mismatches in WezTerm's rendering system. The Element system's lack of true overflow containers and the batched rendering pipeline make traditional clipping approaches ineffective. Future implementation should either use render-to-texture for proper GPU-accelerated clipping or abandon horizontal scrolling in favor of text wrapping.