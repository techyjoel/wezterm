# Colored Wrapped Text Implementation Plan

## Executive Summary

This document tracks our efforts to implement syntax highlighting and font variants (bold/italic) with proper line wrapping in WezTerm's sidebar. After extensive investigation, we've identified the architectural constraints and developed a pragmatic solution.

## Current Status (Jul 3 2025)

### Implementation Complete ✅

**Wrap-Before-Shape Approach Successfully Implemented:**
1. **Core Implementation**:
   - Added `WrappedLine` struct with byte tracking and space skipping
   - Implemented `wrap_text_with_estimates()` using actual font character widths
   - Implemented `shape_line_with_styles()` to shape each line with appropriate fonts
   - Modified `wrap_styled_text()` to use new approach

2. **Font Measurement**:
   - Added `calculate_average_char_width()` that measures actual font metrics
   - Uses representative sample text for accurate width estimation
   - Properly handles proportional vs monospace font differences
   - Includes `WIDTH_CORRECTION_FACTOR` for fine-tuning (currently 1.05)
   - Font width calculations are cached to avoid repeated measurements

3. **Critical Fixes Applied**:
   - ✅ Fixed invisible bold/italic text by setting explicit text color
   - ✅ Fixed `OutOfTextureSpace` error propagation for glyph cache resizing
   - ✅ Fixed progressive character loss with `track_wrapping_with_lines()`
   - ✅ Fixed text clipping by removing content boundary restrictions
   - ✅ Fixed space position tracking in wrap calculations
   - ✅ Removed performance-impacting debug logging

### Syntax Highlighting (Code Blocks) ✅
- **Status**: FULLY WORKING - Per-token highlighting
- **Approach**: StyledWrappedText with proper wrapping fixes
- **What works**: 
  - Per-token syntax highlighting with individual colors
  - Proper line wrapping without character loss
  - Monospace font optimization (bypasses width calculation)
  - Theme integration with WezTerm color palettes
  - Configurable syntax dimming factor

### Font Variants (Markdown Text) ✅
- **Status**: FULLY WORKING
- **What works**:
  - Bold/italic text is visible with proper colors
  - Fonts load and shape correctly
  - Style spans are created and tracked properly
  - Text wraps at appropriate positions
  - No character loss or clipping issues
  - Inline code uses proper code font with background color

### Recently Completed (Jul 1-3 2025)
1. ✅ **Segment Batching Bug** – fixed color-comparison logic; draw-call batching now triggers correctly.  
2. ✅ **Debug Logging** – added `WEZTERM_LOG=debug` hooks; grep “Code block language:” or “Syntax highlighting for line”.  
3. ✅ **WezTerm Theme Integration** – code blocks pull from active theme palette instead of a hard-coded one.  
4. ✅ **Improved Color Mappings** – expanded `create_syntect_theme_from_palette` scope list for full token coverage.  
5. ✅ **Wrap-Before-Shape Implementation** – Successfully implemented with proper font measurement
6. ✅ **Progressive Character Loss Fix** – Fixed grapheme-to-cell mapping for skipped spaces
7. ✅ **Font Width Calculation** – Implemented actual font measurement instead of using terminal cell width
8. ✅ **Text Clipping Fix** – Removed content boundary restrictions to allow proper rendering
9. ✅ **Performance Optimization** – Added font width caching (thread-local) and monospace optimization
10. ✅ **Syntax Dimming Factor** – Made configurable via `clibuddy.right_sidebar.fonts.syntax_dimming_factor`
11. ✅ **Character Loss at Wrap Points** – Fixed wrap position calculation bug
12. ✅ **Restored Markdown Styling** – Fixed regression that broke bold/italic support
13. ✅ **Missing Characters in Markdown** – Fixed color inheritance issue where unstyled text was transparent

### Key Technical Solutions

1. **Character Width Calculation**: The `calculate_average_char_width()` function measures actual font metrics using a representative text sample with thread-local caching
2. **Grapheme Tracking**: The `track_wrapping_with_lines()` method properly accounts for skipped leading spaces on wrapped lines
3. **Text Rendering**: Removed clipping restrictions to allow text to render into padding areas when needed (prevents character loss)
4. **Font Selection**: Style spans correctly specify fonts for bold/italic text rendering
5. **Monospace Optimization**: Code blocks bypass width calculation for monospace fonts
6. **Per-Token Highlighting**: Fixed wrap position tracking to maintain per-token syntax colors
7. **Color Inheritance**: Fixed by passing element colors to `wrap_styled_text` as default for unstyled segments

### Technical Fixes Applied

#### Fix 1: Transparent Text Color (SOLVED)
- **Problem**: Style spans used `ElementColors::default()` with `text: Inherited`
- **Solution**: Set explicit text color `LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)`
- **File**: `markdown.rs` line ~450

#### Fix 2: Texture Size Error Propagation (SOLVED)
- **Problem**: `shape_line_with_styles()` caught and suppressed `OutOfTextureSpace` errors
- **Solution**: Check for `OutOfTextureSpace` and propagate instead of suppressing
- **File**: `box_model.rs` lines ~1620-1650

#### Fix 3: Color Inheritance for Unstyled Text (SOLVED)
- **Problem**: Unstyled text segments used `ElementColors::default()` which inherits color, resulting in transparent text when no parent exists
- **Solution**: Pass element's colors to `wrap_styled_text()` as default for unstyled segments
- **File**: `box_model.rs` - added `element_colors` parameter to `wrap_styled_text()`

### Key Code Locations

- **Wrap-Before-Shape**: `wezterm-gui/src/termwindow/box_model.rs`
  - `wrap_text_with_estimates()` - Line ~1358
  - `shape_line_with_styles()` - Line ~1475
  - `wrap_styled_text()` - Line ~1210
- **Font Loading**: `wezterm-gui/src/termwindow/render/sidebar_render.rs`
  - `load_sidebar_font_variants()` - Line ~946
- **Markdown Rendering**: `wezterm-gui/src/sidebar/components/markdown.rs`
  - `build_paragraph_element()` - Line ~145
- **Glyph Cache**: `wezterm-gui/src/glyphcache.rs` (needs investigation)

## Implementation Plan: Wrap-Before-Shape Approach

### Overview
Instead of the current flow (Shape → Wrap), we'll reverse it to (Wrap → Shape), allowing us to use different fonts after determining line breaks.

### Key Insight
- Use `cell_size.width` as uniform character width estimate
- Accept ~20% overflow for styled text (into padding)
- Break at word boundaries when possible, character boundaries for long words

### Implementation Steps

#### 1. Modify `wrap_styled_text()` in box_model.rs
```rust
pub fn wrap_styled_text(
    &self,
    text: &str,
    default_font: &Rc<LoadedFont>,
    style_spans: &[StyleSpan],
    max_width: f32,
    context: &LayoutContext,
    default_style: &config::TextStyle,
) -> anyhow::Result<...> {
    // Step 1: Wrap using uniform width estimates
    let char_width = context.metrics.cell_size.width;
    let wrapped_lines = self.wrap_text_with_estimates(
        text,
        char_width,
        max_width
    )?;
    
    // Step 2: Shape each wrapped line with appropriate fonts
    let mut shaped_lines = Vec::new();
    for line in wrapped_lines {
        let shaped = self.shape_line_with_styles(
            &line,
            style_spans,
            default_font,
            context,
            default_style
        )?;
        shaped_lines.push(shaped);
    }
    
    // Step 3: Build style mappings as before
    // ...existing code...
}
```

#### 2. Add `wrap_text_with_estimates()` helper
```rust
fn wrap_text_with_estimates(
    &self,
    text: &str,
    char_width: f32,
    max_width: f32,
) -> Vec<WrappedLine> {
    // Use existing wrap_text logic but with uniform width
    // Track byte offsets for each wrapped line
    // Return: Vec<WrappedLine { text: String, byte_offset: usize }>
}
```

#### 3. Add `shape_line_with_styles()` helper
```rust
fn shape_line_with_styles(
    &self,
    line: &WrappedLine,
    style_spans: &[StyleSpan],
    default_font: &Rc<LoadedFont>,
    context: &LayoutContext,
    style: &config::TextStyle,
) -> Vec<ElementCell> {
    let mut cells = Vec::new();
    
    // Find style spans that overlap this line
    for span in style_spans {
        if span.overlaps_line(line) {
            // Shape this segment with its font
            let font = span.font.as_ref().unwrap_or(default_font);
            let segment = extract_segment(line, span);
            let shaped = shape_text(segment, font, context, style)?;
            cells.extend(shaped);
        }
    }
    
    cells
}
```

### Expected Behavior
- Text wraps at approximately correct positions
- Bold/italic text may overflow max_width by ~20%. If we can account for this with some multipler in the estimated width when we see bold or italic that could be a solution.
- Font variants are visually applied
- Word boundaries are respected

### Testing Checklist
- [ ] Test markdown with **bold**, *italic*, and ***bold italic***
- [ ] Verify inline `code` uses monospace font
- [ ] Test very long emphasized words that need character breaking
- [ ] Verify style changes mid-word (e.g., "**bo**ld")
- [ ] Check overflow is acceptable (not extending past sidebar bounds)
- [ ] Test performance with many style changes
- [ ] Verify non-ASCII text still wraps correctly

## Failed Attempts

### 1. Shape-then-wrap approaches
All these failed because fonts must be selected BEFORE shaping:

- **Enhanced StyledWrappedText** (January 2025)
  - Added font loading, style span creation
  - Problem: `wrap_styled_text()` ignores font field, uses default_font for all text (line 1219 in box_model.rs)
  - Has TODO comment confirming this limitation

- **Using Children elements** (commits c2d77f014, 322f5b8a0)
  - Problem: Children layout doesn't implement text wrapping (verified in box_model.rs lines 1259-1315)
  - Text extends horizontally forever

- **Font variants in syntax highlighting**
  - Problem: Conflicts with color span system
  - Can't mix fonts and colors in same text

### 2. Post-shaping font swapping
- **Architecturally impossible with attempted approach**: Glyphs are immutable after shaping
- Each glyph is cached with specific font ID
- Can't change font without re-shaping

## Architectural Constraints

### Why This Is Hard
1. **Shaping requires font selection**: Can't know glyph widths without font
2. **Wrapping requires glyph widths**: Can't wrap without knowing sizes
3. **Chicken-and-egg problem**: Need font to wrap, need wrapping to apply fonts

### Terminal vs Element System
- **Terminal**: Cell-based → per-cell font selection → no wrapping needed
- **Elements**: Text-based → single font for wrapping → styles applied after

### Children Layout Lacks Wrapping
Inline children accumulate width (`block_pixel_width += kid.bounds.width()`) and never reposition.  Commits attempting inline `Text` or `WrappedText` verify overflow.

## Alternative Approaches (Not Recommended)

### Word-Level Elements
Create separate Element per word with manual wrapping logic.
- **Pros**: Works with current architecture
- **Cons**: Complex spacing, manual wrapping, not sure how this would work without breaking wrapping due to children issue

### External Text Layout Library
Integrate Pango or DirectWrite.
- **Pros**: Professional text layout
- **Cons**: Major dependency, platform-specific

### Accept Current Limitation
Keep implementation without font variants.
- **Pros**: No work, wrapping perfect
- **Cons**: No visual emphasis

## Code References

### Key Files
- `wezterm-gui/src/termwindow/box_model.rs` - Text wrapping and shaping
- `wezterm-gui/src/sidebar/components/markdown.rs` - Markdown rendering
- `wezterm-gui/src/termwindow/render/sidebar_render.rs` - Font loading

### Important Functions
- `wrap_text()` - Current single-font wrapping
- `wrap_styled_text()` - Attempts multi-font but uses single font
- `shape()` - Font-specific text shaping (in LoadedFont)

## Remaining Tasks and Improvements

### Minor Enhancements
- [ ] All font colors should come from theme instead of any hardcoded values (may currently be using 0.9, 0.9, 0.9)
- [ ] Support font changes mid-word if can be done without complexity (e.g., "**bo**ld") - currently changes apply to whole words. If too compelex or risky, skip
- [ ] Makie WIDTH_CORRECTION_FACTOR configurable in our LUA file (currently 1.02) (and put a comment describing what it does and what all it affects)
- [ ] Calculate code block chrome dynamically instead of hardcoded 26px

### Documentation
- [ ] Document the wrap-before-shape approach for future contributors
- [ ] Add architecture diagram showing the text rendering pipeline