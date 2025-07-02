# Colored Wrapped Text Implementation Plan

## Executive Summary

This document tracks our efforts to implement syntax highlighting and font variants (bold/italic) with proper line wrapping in WezTerm's sidebar. After extensive investigation, we've identified the architectural constraints and developed a pragmatic solution.

## Current Status (Jul 2 2025)

### Implementation Progress

**What's Been Implemented:**
1. **Wrap-Before-Shape Approach (Option 1)**: Preserve exact substrings from original text
   - Added `WrappedLine` struct with byte tracking and space skipping
   - Implemented `wrap_text_with_estimates()` to wrap using uniform character widths
   - Implemented `shape_line_with_styles()` to shape each line with appropriate fonts
   - Modified `wrap_styled_text()` to use new approach

2. **Font Loading Infrastructure**:
   - Font variants (bold/italic) are successfully loaded
   - `SidebarFonts` struct properly configured
   - Markdown component creates style spans with font references

### Critical Issues Blocking Font Variants

#### Issue 1: Texture Size Exceeded Error
```
ERROR Failed to shape segment "OpenSSL error" to cells: Texture Size exceeded, need Some(256)
```
- **What's happening**: When trying to render bold/italic font glyphs, the glyph cache texture atlas is full
- **Impact**: Even with fallback to default font, text becomes invisible
- **Root cause**: WezTerm's glyph cache has limited texture space (256x256?)
- **Why fallback fails**: Unknown - fallback code executes but text remains invisible

#### Issue 2: Progressive Character Loss on Wrapped Lines
- **Symptom**: Comment line `# This is a very long line that should...` shows:
  - Line 1: `# This is a very long` (correct)
  - Line 2: `ine that should` (missing 'l' - 1st char)
  - Line 3: `efinitely trigger` (missing 'd' - 2nd char)
  - Line 4: `rizontal scrolling in` (missing 'ho' - 3rd char)
- **Pattern**: Each wrapped continuation line loses N characters where N = line number - 1
- **Root cause**: Byte offset calculation error in wrapping logic

### Syntax Highlighting (Code Blocks) ✅
- **Status**: WORKING - ASCII-only implementation
- **Approach**: Option 5 (Virtual Multi-Element with ASCII-Only)
- **What works**: 
  - Syntax highlighting for ASCII characters
  - Proper line wrapping maintained
  - Non-ASCII/whitespace/ligatures get default color by design
  - Segment batching for performance
  - Theme integration with WezTerm color palettes

### Font Variants (Markdown Text) ❌
- **Status**: NOT WORKING - Two blocking issues prevent any text with font variants from displaying
- **Infrastructure**: Complete - fonts load, style spans created, shaping attempted
- **Blocking issues**: See Critical Issues above

### Recently Completed (Jul 1-2 2025)
1. ✅ **Segment Batching Bug** – fixed color-comparison logic; draw-call batching now triggers correctly.  
2. ✅ **Debug Logging** – added `WEZTERM_LOG=debug` hooks; grep “Code block language:” or “Syntax highlighting for line”.  
3. ✅ **WezTerm Theme Integration** – code blocks pull from active theme palette instead of a hard-coded one.  
4. ✅ **Improved Color Mappings** – expanded `create_syntect_theme_from_palette` scope list for full token coverage.  
5. ❌ **Punctuation-Based Wrapping** – attempted; rolled back (see Failed Attempts).
6. ✅ **Wrap-Before-Shape Core** – Implemented but blocked by texture size and offset bugs

### Debug Information

**Key Debug Commands:**
```bash
# Basic logging for wrapped lines and font usage
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "(shape_line_with_styles:|Using style span font|Failed to shape segment)"

# Check texture size errors and fallback
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "(Texture Size exceeded|Fallback also failed)"

# Verify specific wrapped line offsets
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "shape_line_with_styles: byte_offset=(22|39|58)"
```

**Current Behavior:**
1. Font variants load successfully (`bold=true, italic=true, bold_italic=true`)
2. Style spans are created correctly with font references
3. Text is shaped and cells are created (23 cells for "I see you're getting an")
4. But styled text is invisible due to texture size error
5. Fallback to default font doesn't make text visible

### Technical Analysis

**Texture Size Issue:**
- The glyph cache appears to have a 256x256 texture limit
- When bold/italic glyphs are cached, it exceeds this limit
- The error propagates even with fallback handling
- Possible solutions:
  1. Increase glyph cache texture size
  2. Clear/compact glyph cache before rendering sidebar
  3. Use a separate glyph cache for sidebar fonts

**Character Loss Issue:**
The byte offset calculation has a cumulative error. Current logic:
1. `line_start_pos` tracks position in line
2. When skipping spaces: `line_start_pos = current_pos` (after skip)
3. `byte_offset = line_byte_start + actual_line_start`
4. But `actual_line_start` calculation is incorrect

**Why Bold/Italic Text Is Invisible:**
Even when fallback to default font is triggered, the text doesn't appear. This suggests:
1. The error handling might be breaking the render pipeline
2. The computed element might have incorrect bounds
3. The cells might be created but not rendered

### Next Steps to Investigate

**For Texture Size Issue:**
1. Find where glyph cache size is configured (likely in `glyphcache.rs` or `renderstate.rs`)
2. Check if cache can be increased or if there's a config option
3. Investigate why fallback doesn't work - cells might be empty or have zero size
4. Consider pre-loading common glyphs to avoid runtime allocation

**For Character Loss Issue:**
1. Add detailed logging of byte positions at each step of wrapping
2. The issue appears to be cumulative - each line loses more characters
3. Focus on the relationship between:
   - `current_pos` (position in line being processed)
   - `line_start_pos` (where the wrapped line starts)
   - `skip_spaces` (spaces to skip at line start)
   - `actual_line_start` calculation

**For Rendering Investigation:**
1. Check if cells have valid bounds/positions
2. Verify the computed element has correct dimensions
3. Look for z-index or clipping issues
4. Check if the render pipeline skips elements with errors

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

## Backlog after this implementation is working

### Syntax Highlighting Improvements
- [ ] Configure dimming factor (currently hardcoded 0.85), expose `syntax_dimming_factor` (float 0.7-1.0) in `clibuddy.right_sidebar`; default 0.85. 
- [ ] Document multi-cell character limitations, clarify ASCII-only design; show example with tabs & emoji in READMEs. 
- [ ] Performance optimization for style lookups
- [ ] Background color support for selections, extend `StyleSpan` to carry bg; update `MultilineText` renderer; needed for selection highlight

### Font Variant Perfection
- [ ] Support font changes mid-word