# Clipping Investigation Report

## Problem Summary
Despite implementing GPU scissor rect infrastructure, manual clipping in `render_element`, and unique z-indices for code blocks, horizontal scrolling still shows content overflowing visually.

## Key Findings

### 1. Scissor State Handling Per Render Pass
The core issue appears to be that **scissor state is applied PER RENDER PASS**, not globally:

- In WebGPU (`draw.rs:99-163`): Each layer creates a **new render pass** with `encoder.begin_render_pass()` 
- In OpenGL (`draw.rs:319-356`): Each layer is drawn with a **separate draw call**
- The scissor rect is applied **inside each render pass/draw call**

This means:
- When `render_element` pushes a scissor rect for an element with `clip_bounds`
- The scissor is correctly applied to that element's layer
- BUT: The scissor state doesn't persist across different layers/render passes

### 2. Z-Index Creates Separate Layers
From `renderstate.rs:725-744`:
- Each unique z-index creates a separate `RenderLayer` 
- Layers are rendered in z-index order
- Each layer gets its own render pass

### 3. The Clipping Problem
When you have code blocks with unique z-indices (50-69), here's what happens:

1. `render_element` is called for a code block with z-index 50
2. It pushes a scissor rect onto the stack
3. It gets layer for z-index 50, renders content
4. It pops the scissor rect
5. **A new render pass starts for the next layer**
6. The GPU scissor state from the previous pass is lost

### 4. Why Manual Clipping Also Fails
The manual clipping in `render_element` (lines 1296-1383) only applies to:
- Sprites and glyphs rendered **within that specific element**
- It doesn't affect other elements that might overlap spatially but are on different z-indices

### 5. The Real Issue: Cross-Layer Clipping
The fundamental issue is that clipping needs to work **across layers**, but the current architecture:
- Applies scissor per render pass
- Each z-index is a separate layer/pass
- Scissor state doesn't carry between passes

## Potential Solutions

### Option 1: Same Z-Index for Clipped Content
Put all content that needs to be clipped together on the **same z-index/layer**. This ensures they're in the same render pass and scissor state applies.

### Option 2: Persistent Scissor State
Modify the rendering loop to maintain scissor state across render passes:
- Track "active clip regions" at a higher level
- Apply appropriate scissor for each render pass based on what's being rendered

### Option 3: Stencil Buffer
Use the stencil buffer for clipping instead of scissor rects:
- Write clip masks to stencil
- Test against stencil when rendering
- Works across render passes

### Option 4: Render to Texture
Render scrollable content to an off-screen texture:
- Apply clipping when rendering to texture
- Draw the texture with proper bounds in main pass

## Recommended Fix
The simplest fix is likely **Option 1**: Ensure all content within a scrollable area uses the same z-index. This keeps everything in the same render pass where scissor state works correctly.

The current approach of giving each code block a unique z-index (50-69) defeats the scissor clipping by spreading content across multiple render passes.