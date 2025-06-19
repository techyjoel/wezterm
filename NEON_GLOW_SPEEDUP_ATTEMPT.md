# Neon Glow Speedup Attempt - Pre-rendered Texture Approach

## Problem Statement
The original neon glow implementation was causing significant performance issues:
- Rendering slowdowns and keyboard input delays
- Multiple glow layers rendered in real-time for each frame
- Performance degraded with multiple active sidebar buttons

## Attempted Solution: Pre-rendered Texture Cache

### Approach Overview
We attempted to optimize the neon glow effect by pre-rendering glow textures and caching them in the main glyph atlas, then rendering them as textured quads instead of drawing multiple offset glyph layers in real-time.

### Implementation Details

#### 1. Glow Cache Integration
- **File**: `wezterm-gui/src/glyphcache.rs`
- Added `GlowKey` struct for cache keys:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq, Hash)]
  pub struct GlowKey {
      pub text: String,
      pub font_id: LoadedFontId,
      pub base_color: (u8, u8, u8, u8),
      pub glow_color: (u8, u8, u8, u8),
      pub glow_radius: u16,
      pub glow_layers: u8,
      pub glow_intensity: u8,
  }
  ```
- Added `glow_cache: HashMap<GlowKey, Sprite>` to `GlyphCache`
- Implemented `cached_glow()` method that generates and caches glow textures

#### 2. Glow Texture Generation
- **Method**: `render_glyph_with_glow()` 
- Uses font shaping to get glyph information
- Rasterizes the glyph at appropriate size
- Renders multiple offset layers with decreasing alpha for glow effect
- Creates final texture with main glyph rendered on top
- Uses proper alpha blending and Linear RGB to sRGB conversion

#### 3. Rendering Integration
- **File**: `wezterm-gui/src/termwindow/render/neon.rs`
- Modified `render_neon_glyph()` to use cached glow textures
- Allocates quad from layer allocator
- Sets texture coordinates from cached sprite
- Applies OpenGL coordinate transformation

#### 4. Removed Separate Cache
- Deleted `wezterm-gui/src/glowcache_simple.rs`
- Removed separate glow cache infrastructure from `TermWindow`
- Integrated everything into the main glyph cache for texture atlas consistency

### What Was Found

#### Positive Results
✅ **Texture Generation Works**: Glow textures are successfully generated
- Confirmed via debug logging: textures created with correct dimensions (e.g., 60x65, 64x65 pixels)
- Non-transparent pixel counts show actual glow content (1900+ pixels)
- Texture data verified with pixel sampling (e.g., center pixels: `0xfffe1493`, `0x57005757`)

✅ **Caching Works**: Textures are properly cached and retrieved
- First call generates texture, subsequent calls use cache
- Cache keys work correctly for different glyph/style combinations

✅ **Atlas Integration Works**: Sprites successfully allocated in main texture atlas
- No "out of texture space" errors
- Texture coordinates properly assigned

✅ **Quad Allocation Works**: Rendering quads successfully created
- Quads allocated on different layers (0, 1, 2 tested)
- Texture coordinates and colors properly set

#### The Problem
❌ **Glow Not Visible**: Despite all components working, the glow effect is not visible in the UI

#### Debugging Findings
1. **Coordinate System Issues**: 
   - Button positions are in screen coordinates (e.g., x=1302, y=10)
   - Glow positions calculated correctly (e.g., glow_y = 10 - 8 = 2)
   - OpenGL transformation puts glow at negative Y coordinates (top=-514)
   - This suggests the glow quad is positioned off-screen

2. **Rendering Pipeline Mismatch**:
   - Main icon rendered via Element/box model system
   - Glow rendered as raw quad with manual coordinate transformation
   - Different rendering paths may have different texture binding/atlas expectations

3. **Layer Issues**:
   - Tested glow on layers 0, 1, and 2
   - No visible difference regardless of layer choice

### Specific Technical Issues Discovered

#### Coordinate Transformation Problem
```
Window: 1724x1032 pixels
Button position: (1302, 10) in screen coordinates  
Glow position: (1294, 2) after padding calculation
OpenGL offsets: (862, 516) - half window dimensions
Final quad position: (432, -514) - glow extends above visible area
```

The button is positioned very close to the top of the window (y=10), and the glow extends 8 pixels above it (y=2). When transformed to OpenGL coordinates, this puts the glow quad partially or entirely off-screen.

#### Texture Atlas Binding
The glow texture is stored in the main glyph cache atlas, but the quad rendering may not be properly bound to this atlas. Other rendering (like filled rectangles) uses `util_sprites` which may use a different texture source.

### What Should Be Attempted Next

#### Option 1: Fix Coordinate System
1. **Investigate Element System Coordinates**: Understand how the Element/box model system handles coordinate transformation
2. **Match Coordinate Systems**: Ensure glow quad uses the same coordinate transformation as the main icon
3. **Test with Different Button Positions**: Move buttons away from screen edges to see if coordinate issues are the root cause

#### Option 2: Use Element System for Glow
1. **Render Glow as Element**: Instead of raw quads, create an Element that renders the glow texture
2. **Layer Behind Icon**: Use z-index to ensure glow renders behind the main icon
3. **Consistent Rendering Pipeline**: This would use the same rendering path as the main icon

#### Option 3: Debug Rendering Pipeline
1. **Add Visible Test Quad**: Render a solid colored quad at the same position to verify quad positioning works
2. **Check Texture Binding**: Verify that the glyph atlas texture is properly bound during quad rendering
3. **Shader Debugging**: Check if the `IS_COLOR_EMOJI` flag is being handled correctly by the shader

#### Option 4: Position Adjustment
1. **Add Margin to Button Position**: Move buttons further from screen edges
2. **Clamp Glow Position**: Ensure glow quads don't extend beyond screen boundaries
3. **Dynamic Glow Size**: Reduce glow radius when buttons are near screen edges

### Code Quality Issues Found
- Mixed coordinate systems between Element and quad rendering
- Complex coordinate transformation logic that's hard to debug
- No fallback when glow extends beyond screen boundaries

## Alternative Methods

### 1. GPU-Based Blur Shader
**Approach**: Use compute shaders or fragment shaders to generate blur effects in real-time on the GPU.

**Pros**:
- Leverages GPU parallel processing
- No CPU texture generation overhead
- Can be very fast for simple blur kernels
- Automatic handling of coordinate systems

**Cons**:
- Requires shader programming
- May not be available on all GPU backends (OpenGL vs WebGPU)
- More complex to implement variable blur radius/intensity

**Implementation Strategy**:
- Render icon to offscreen texture
- Apply Gaussian blur shader passes
- Composite blurred result behind main icon

### 2. Distance Field-Based Glow
**Approach**: Use signed distance fields (SDF) to generate smooth glow effects mathematically.

**Pros**:
- Extremely fast - just math in fragment shader
- Perfect scaling at any resolution
- Easy to animate glow intensity/size
- No texture memory overhead

**Cons**:
- Requires converting glyphs to distance fields
- May not work well with complex glyph shapes
- Learning curve for SDF techniques

**Implementation Strategy**:
- Generate SDF texture for each glyph once
- Use SDF in fragment shader to calculate glow falloff
- Render glow effect procedurally

### 3. Multi-Pass Rendering with Stencil Buffer
**Approach**: Use multiple rendering passes with stencil buffer to create glow effect.

**Pros**:
- Works with existing glyph rendering
- Can reuse current quad/element system
- Good control over glow shape

**Cons**:
- Still requires multiple draw calls
- Stencil buffer operations can be slow
- More complex state management

**Implementation Strategy**:
- First pass: render glyph to stencil buffer
- Second pass: render enlarged/blurred version as background
- Third pass: render main glyph on top

### 4. Instanced Rendering with Offsets
**Approach**: Use GPU instancing to render multiple offset copies of the glyph in a single draw call.

**Pros**:
- Single draw call for all glow layers
- GPU handles offset calculations
- Can be very fast with proper batching

**Cons**:
- Still CPU overhead for calculating instances
- Limited control over per-layer alpha
- Requires instancing support

**Implementation Strategy**:
- Create instance buffer with offset/alpha data
- Modify vertex shader to handle instanced data
- Render all glow layers + main glyph in one call

### 5. Texture-Based Lookup Tables
**Approach**: Pre-generate lookup textures for different glow patterns and sample them.

**Pros**:
- Very fast at runtime
- Consistent glow appearance
- Easy to tweak glow patterns

**Cons**:
- Fixed glow patterns (less dynamic)
- Memory overhead for lookup textures
- May not scale well to different glyph sizes

**Implementation Strategy**:
- Create 2D textures with glow falloff patterns
- Sample texture in fragment shader based on distance from glyph center
- Combine with main glyph rendering

### Recommended Next Approach
Based on performance requirements and implementation complexity, I recommend trying **GPU-Based Blur Shader** first:

1. It provides the best performance/complexity tradeoff
2. Leverages GPU capabilities effectively  
3. Integrates well with existing rendering pipeline
4. Provides high-quality visual results

The implementation would involve:
1. Render the icon to an offscreen framebuffer
2. Apply a two-pass Gaussian blur (horizontal then vertical)
3. Composite the blurred result behind the main icon
4. All operations happen on GPU with minimal CPU overhead

This approach avoids the coordinate system issues we encountered and leverages the GPU's parallel processing capabilities for optimal performance.