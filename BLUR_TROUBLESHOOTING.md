# GPU Blur Troubleshooting

## Issue Description (RESOLVED)
The GPU-accelerated neon glow effect for sidebar button icons had the following issues:
1. **Horizontal banding/lines**: Visible at 1px and 3px blur radii - FIXED
2. **Asymmetric blur**: More glow weight at bottom than top - FIXED
3. **Missing glow on icon parts**: AI icon's down-arrow portion has no glow - FIXED
4. **Fixed blur dimensions**: Blur appears same size regardless of radius setting - PARTIALLY FIXED

### Visual Symptoms (User-Reported)
- 1px blur: Strong horizontal lines, blur wider than expected
- 3px blur: Still shows horizontal lines, asymmetric
- 6px blur: Looks more like blur but very bottom-heavy, shorter than expected
- All radii show same pixel dimensions for blur effect

## Technical Analysis

### Icon Rendering Pipeline
1. Icon font glyph is rasterized to texture (`icon_to_texture.rs`)
2. Texture passed to GPU blur renderer (`blur.rs`) 
3. Two-pass separable blur (horizontal then vertical) via WGSL shader (`blur.wgsl`)
4. Result composited via effects overlay system (`effects_overlay.rs`)

### Pre-Blur Icon Analysis
- Analyzed saved PPM texture for AI icon (52x52px)
- Content perfectly centered: (25.5, 25.0) vs image center (26.0, 26.0)
- Vertical distribution ratio: 1.01 (nearly perfect)
- Horizontal distribution ratio: 1.00 (perfect)
- **Conclusion**: Asymmetry is NOT in the icon texture

### Blur Parameters (3px radius)
- sigma = 0.9009009 (using GIMP formula: radius/3.33)
- kernel_size = 7
- kernel_radius = 3
- Sampling from -3 to +3 (7 samples total)

## Attempted Fixes (All Failed)

### 1. Icon Centering Improvements
- Fixed centering calculation to not subtract content bounds offset
- Increased blur padding from 5 to 2x blur radius
- Result: No change in appearance

### 2. Blur Algorithm Adjustments
- **Sigma calculations tried**:
  - radius / 3 (original)
  - radius (direct)
  - radius / 3.33 (GIMP formula)
- **Kernel size calculations tried**:
  - Standard 6σ approach
  - GIMP's threshold-based (capture to 1/255 intensity)
  - Minimum kernel size enforcement
- Result: No change in appearance

### 3. Texture Format Changes
- Changed render targets from `Rgba8UnormSrgb` to `Rgba8Unorm`
- Reasoning: Eliminate sRGB conversion between blur passes
- Result: No change in appearance

### 4. Shader Modifications
- Removed edge bounds checking
- Added radius-based sampling scaling
- Fixed potential coordinate system issues
- Result: No change in appearance

### 5. Caching Elimination
- Added blur cache clearing in debug mode
- Disabled caching entirely when WEZTERM_DEBUG_BLUR=1
- Result: Confirmed new code is running, but appearance unchanged

## GIMP Implementation Analysis

Analyzed GIMP's Gaussian blur (blur-gauss.c):

### Key Differences Found:
1. **Sigma Formula**: GIMP uses `sigma = sqrt(radius² / (2 * ln(255)))` ≈ radius/3.33
2. **Kernel Size**: Based on capturing values down to 1/255 intensity
3. **Edge Handling**: Pre-extends borders before blur
4. **Integer Math**: Uses integer weights scaled by 255

### What We Implemented:
- Matched GIMP's sigma formula exactly
- Matched kernel size calculation
- GPU's ClampToEdge should be equivalent to border extension
- Using floating-point math (should be more accurate)

## Critical Insights

### 1. Fixed Blur Dimensions Issue
User observation: "no matter what the blur radius is, the width and height of the blur pixels is the same"
- This suggests fundamental sampling issue in shader
- Attempted fix: Added radius-based step scaling
- Result: No effect, suggesting issue is elsewhere

### 2. Separable Blur Validity
- Separable Gaussian blur is mathematically correct
- Used by GIMP, Photoshop, all major software
- Issue likely not in the approach but implementation

### 3. Persistent Symptoms Pattern
- Horizontal lines → Discrete sampling artifacts
- Bottom-heavy → Accumulation bias in vertical pass
- Missing arrow → Clipping or sampling bounds issue

## Eliminated Causes

1. **Icon texture creation** - Verified perfectly centered
2. **Color space issues** - Fixed early, color now correct
3. **Sigma/kernel calculations** - Tried all variations
4. **Texture formats** - Linear vs sRGB made no difference
5. **Caching** - Disabled, confirmed changes applied
6. **Basic shader math** - Gaussian weight formula correct

## Remaining Possibilities

### 1. GPU/Driver Bug
- Shader compiler optimization causing issues
- Texture sampling precision problems
- Platform-specific WebGPU implementation bug

### 2. Coordinate System Mismatch
- Y-flip in vertex shader vs texture coordinates
- Accumulation of sub-pixel errors between passes
- Render target coordinate system differences

### 3. Fundamental Shader Bug
- Off-by-one error in sampling loop
- Incorrect texture coordinate calculations
- Issue with how step_size is applied

### 4. Pipeline Integration Issue
- Problem in how blurred texture is created/stored
- Issue in effects overlay compositing
- WebGPU-specific texture binding problem

## Root Cause (FOUND)

The horizontal banding was caused by incorrect glyph data interpretation. The font rasterizer returns RGBA data (4 bytes per pixel) but the code was treating it as single-channel alpha data (1 byte per pixel). This caused the code to read only every 4th pixel, creating the characteristic horizontal lines.

### Fix Applied
- Updated `icon_to_texture.rs` to correctly read RGBA glyph data
- Fixed array indexing to use `(y * width + x) * 4` instead of `y * width + x`
- Read alpha channel from index 3 of RGBA data

### Additional Fixes
- Fixed Y-coordinate mismatch in blur vertex shader
- Added proper Y-flip for texture coordinates in both blur and glow composite shaders
- Removed sRGB conversion to keep everything in linear color space
- Improved texture size calculations to account for blur radius

## Remaining Issues
1. **Blur radius scaling**: The visual size of the blur could be better scaled with radius
2. **Performance optimization**: Could benefit from compute shader implementation
3. **OpenGL support**: Currently WebGPU-only, no OpenGL fallback

## Code Locations
- Icon texture: `wezterm-gui/src/termwindow/render/icon_to_texture.rs`
- Blur implementation: `wezterm-gui/src/termwindow/render/blur.rs`
- Blur shader: `wezterm-gui/src/shaders/blur.wgsl`
- Neon rendering: `wezterm-gui/src/termwindow/render/neon.rs`
- Effects overlay: `wezterm-gui/src/termwindow/render/effects_overlay.rs`
- WebGPU setup: `wezterm-gui/src/termwindow/webgpu.rs`

## Environment
- Platform: macOS (Darwin 22.6.0)
- GPU: Unknown (check with user)
- Renderer: WebGPU
- Font: Material Design Icons (built-in)
- Tested blur radii: 1px, 3px, 6px