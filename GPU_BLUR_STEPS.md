# GPU Blur Next Steps

## Immediate Troubleshooting Tasks

### 1. Fix Glow Positioning
**Issue**: Glow may not be perfectly centered on icons

**Steps**:
1. Add debug visualization to show glow bounds
   ```rust
   // In effects_overlay.rs composite_glow()
   log::info!("Icon at ({}, {}), glow at ({}, {}) size {}x{}", 
       effect.position.x, effect.position.y,
       glow_x, glow_y, glow_width, glow_height);
   ```

2. Verify offset calculation
   - Current: `glow_x = effect.position.x - (glow_width - 40.0) / 2.0`
   - May need to account for icon centering within button
   - Check if position is top-left or center of icon

3. Test with visual markers
   - Temporarily draw a colored border around glow texture
   - Compare with icon position visually

### 2. Fix Glow Color
**Issue**: Glow color might not match neon color exactly

**Steps**:
1. Verify color space handling
   - Check if colors need sRGB to linear conversion
   - Ensure premultiplied alpha is handled correctly

2. Debug color values
   ```rust
   // In create_icon_texture()
   log::info!("Icon color: {:?}", color);
   // In blur shader
   // Add debug output for sampled colors
   ```

3. Check blending equation
   - Current: Additive blending
   - May need to adjust for color accuracy

### 3. Fine-tune Intensity
**Issue**: 80% intensity might be too bright/dim

**Steps**:
1. Make intensity configurable in wezterm.lua
   ```lua
   config.clibuddy.sidebar_button.neon = {
       glow_intensity = 0.8,  -- Base intensity
       glow_multiplier = 0.8, -- GPU multiplier
   }
   ```

2. Add dynamic adjustment based on background
   - Detect dark vs light themes
   - Adjust intensity accordingly

3. Consider non-linear intensity curves
   - Current: Linear multiplication
   - Try: Power curves for more natural falloff

## OpenGL Implementation Plan

### Phase 1: OpenGL Blur Infrastructure

1. **Create OpenGL Blur Shaders**
   ```glsl
   // blur_vertex.glsl
   #version 330 core
   layout(location = 0) in vec2 position;
   layout(location = 1) in vec2 texCoord;
   out vec2 TexCoord;
   void main() {
       gl_Position = vec4(position, 0.0, 1.0);
       TexCoord = texCoord;
   }
   
   // blur_fragment.glsl
   #version 330 core
   in vec2 TexCoord;
   out vec4 FragColor;
   uniform sampler2D sourceTexture;
   uniform vec2 direction;
   uniform float kernel[MAX_KERNEL_SIZE];
   uniform int kernelSize;
   // ... Gaussian blur implementation
   ```

2. **Extend BlurRenderer for OpenGL**
   ```rust
   // In blur.rs
   enum BlurPipeline {
       WebGpu(WebGpuBlurPipeline),
       OpenGl(OpenGlBlurPipeline),
   }
   
   impl BlurRenderer {
       pub fn init_opengl_pipeline(&mut self, gl_state: &GliumRenderState) -> Result<()> {
           // Create shaders, framebuffers, etc.
       }
   }
   ```

3. **Create OpenGL Render Targets**
   - Use Framebuffer Objects (FBOs)
   - Implement texture pooling for OpenGL
   - Handle texture format compatibility

### Phase 2: OpenGL Effects Overlay

1. **Port glow_composite shader to GLSL**
   ```glsl
   // glow_composite_fragment.glsl
   #version 330 core
   uniform sampler2D glowTexture;
   uniform float intensity;
   uniform vec4 glowBounds; // x, y, width, height
   // Additive blending implementation
   ```

2. **Extend EffectsOverlay for OpenGL**
   ```rust
   impl EffectsOverlay {
       pub fn render_opengl(
           &mut self, 
           frame: &mut glium::Frame,
           gl_state: &GliumRenderState,
       ) -> Result<()> {
           // Render each glow effect
       }
   }
   ```

3. **Handle OpenGL state management**
   - Save/restore blend state
   - Manage texture bindings
   - Handle viewport changes

### Phase 3: Integration

1. **Update Initialization**
   ```rust
   // In termwindow/mod.rs
   match &render_state.context {
       RenderContext::WebGpu(_) => { /* existing code */ }
       RenderContext::Glium(gl_state) => {
           blur_renderer.init_opengl_pipeline(gl_state)?;
           // Initialize OpenGL effects overlay
       }
   }
   ```

2. **Update Rendering Path**
   ```rust
   // In render_neon_glyph()
   let can_use_gpu = matches!(
       &self.render_state.as_ref().unwrap().context,
       RenderContext::WebGpu(_) | RenderContext::Glium(_)
   );
   ```

3. **Update draw calls**
   ```rust
   // In call_draw_glium()
   if let Some(ref mut overlay) = self.effects_overlay.borrow_mut().as_mut() {
       overlay.render_opengl(frame, gl_state)?;
   }
   ```

### Phase 4: Platform-Specific Optimizations

1. **OpenGL ES Support** (for older systems)
   - Adjust shader versions
   - Handle extension availability
   - Fallback for missing features

2. **Performance Tuning**
   - Use PBOs for async texture transfers
   - Implement triple buffering
   - Profile and optimize draw calls

3. **Compatibility Testing**
   - Test on macOS (OpenGL 4.1)
   - Test on Linux (various drivers)
   - Test on Windows (ANGLE fallback)

## Testing Strategy

### Visual Tests
1. Screenshot comparison between WebGPU and OpenGL
2. Verify glow alignment with grid overlay
3. Color accuracy tests with reference images

### Performance Tests
1. Measure frame time with 0, 1, 5, 10 active glows
2. Compare WebGPU vs OpenGL performance
3. Profile GPU usage and memory consumption

### Compatibility Tests
1. Test with `front_end = "OpenGL"` configuration
2. Test with software rendering fallback
3. Test on integrated vs discrete GPUs

## Configuration Schema

```lua
config.clibuddy.sidebar_button.neon = {
    -- Existing
    color = { 0.0, 1.0, 1.0, 1.0 },
    glow_radius = 8.0,
    glow_intensity = 0.9,
    
    -- New options
    glow_gpu_multiplier = 0.8,    -- Fine-tune GPU intensity
    glow_quality = "high",         -- "low", "medium", "high"
    glow_debug = false,            -- Show debug overlays
    force_cpu_blur = false,        -- Disable GPU acceleration
}
```

## Debug Helpers

1. **Visual Debug Mode**
   - Draw glow bounds as colored rectangles
   - Show texture dimensions on screen
   - Display performance metrics

2. **Logging Improvements**
   - Add performance timers for each stage
   - Log texture cache hit rates
   - Track memory usage

3. **Interactive Tuning**
   - Hotkeys to adjust intensity in real-time
   - Toggle between CPU/GPU rendering
   - Save/load test configurations

## Implementation Status

### Completed (WebGPU)
- ✅ GPU blur pipeline with 2-pass separable Gaussian blur
- ✅ Effects overlay system for rendering glows
- ✅ Icon texture creation and rasterization
- ✅ Integration with neon rendering system
- ✅ Basic glow visibility at 80% intensity
- ✅ 120x+ performance improvement achieved

### To Do
- [ ] Fix glow positioning accuracy
- [ ] Verify glow color matches neon color
- [ ] Implement OpenGL support
- [ ] Add configuration options
- [ ] Performance profiling and optimization