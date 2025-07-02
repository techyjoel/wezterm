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

3. **Critical Fixes Applied**:
   - ✅ Fixed invisible bold/italic text by setting explicit text color instead of inherited
   - ✅ Fixed `OutOfTextureSpace` error propagation to allow glyph cache resizing
   - ❌ Progressive character loss issue remains unsolved

### Critical Issue: Progressive Character Loss Pattern

#### The Pattern
The issue affects BOTH styled text (bold/italic) AND code blocks. On wrapped lines:
- **Line 1**: Displays correctly
- **Line 2**: Missing 1st character of EACH WORD (but space is still occupied)
- **Line 3**: Missing 2nd character of EACH WORD (but space is still occupied)
- **Line 4**: Missing 3rd character of EACH WORD (but space is still occupied)
- Pattern continues...

Example: "line that should" displays as " ine  hat  hould" (missing 'l', 't', 's', has blank spaces)

#### Key Observations
1. The pattern is too specific to be accidental - Nth line missing Nth char of each word
2. Affects `StyledWrappedText` but NOT plain `WrappedText`
3. Characters are not deleted - the space is occupied but character is invisible
4. Both markdown styled text AND syntax-highlighted code blocks affected

#### What We've Tried
1. **Fixed byte offset tracking** - Multiple attempts to fix `line_start_pos` calculation
2. **Fixed space skipping logic** - Ensured `skip_spaces` is calculated correctly
3. **Fixed style span position mapping** - Adjusted for space-skipped text
4. **Added extensive debug logging** - Confirmed text extraction is correct
5. **Fixed cluster position handling** - Verified glyph shaping is working

#### Current Understanding
- Text extraction is CORRECT: "line that should" is properly extracted
- Glyph shaping is CORRECT: 16 glyphs are created for 16 characters
- Cell creation is CORRECT: 16 cells are created
- But rendering shows systematic character invisibility

This suggests the issue is either:
1. In how cells are positioned/rendered
2. In how the glyph cache handles certain glyphs
3. In some interaction between line number and character rendering

### Syntax Highlighting (Code Blocks) ✅
- **Status**: WORKING - ASCII-only implementation
- **Approach**: Option 5 (Virtual Multi-Element with ASCII-Only)
- **What works**: 
  - Syntax highlighting for ASCII characters
  - Proper line wrapping maintained
  - Non-ASCII/whitespace/ligatures get default color by design
  - Segment batching for performance
  - Theme integration with WezTerm color palettes
- **Issue**: Also affected by the progressive character loss pattern

### Font Variants (Markdown Text) ✅/❌
- **Status**: PARTIALLY WORKING
- **What works**:
  - Bold/italic text is now VISIBLE (fixed transparent color issue)
  - Fonts load correctly
  - Style spans are created properly
  - Text shaping works
- **What doesn't work**:
  - Progressive character loss pattern affects all styled text

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
# Check wrapped lines and font usage
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "(shape_line_with_styles:|Using style span font|Wrapped line)"

# Look for the specific problem pattern
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep "line that should"

# Check for any warnings or errors
WEZTERM_LOG=debug ./target/release/wezterm 2>&1 | grep -E "(WARN|ERROR|Large leading_space_bytes)"
```

### Technical Fixes Applied

#### Fix 1: Transparent Text Color (SOLVED)
- **Problem**: Style spans used `ElementColors::default()` with `text: Inherited`
- **Solution**: Set explicit text color `LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)`
- **File**: `markdown.rs` line ~450

#### Fix 2: Texture Size Error Propagation (SOLVED)
- **Problem**: `shape_line_with_styles()` caught and suppressed `OutOfTextureSpace` errors
- **Solution**: Check for `OutOfTextureSpace` and propagate instead of suppressing
- **File**: `box_model.rs` lines ~1620-1650

#### Fix 3: Progressive Character Loss (UNSOLVED)
Multiple attempts made:
1. **Byte offset tracking** - Tried various approaches to track `line_start_pos`
2. **Space adjustment** - Fixed style span position mapping for space-skipped text
3. **Wrap position handling** - Tried both skipping and not skipping wrap space
4. **Debug verification** - Confirmed text extraction and shaping are correct

The pattern persists: Nth wrapped line missing Nth character of each word.

### Next Steps to Investigate Progressive Character Loss

**Critical Pattern Analysis:**
The Nth wrapped line is missing the Nth character of EACH WORD. This is too specific to be a simple offset error. Since:
- Text extraction is correct
- Glyph shaping is correct (16 glyphs for "line that should")
- Cell creation is correct (16 cells created)
- But rendering shows blank spaces for specific characters

**Hypotheses to Test:**
1. **Per-word offset accumulation**: Something is applying an offset based on line number to each word
2. **Glyph cache key collision**: The Nth character might be getting a bad cache entry
3. **Rendering position calculation**: Cell positions might be offset by line number
4. **Style span interaction**: The pattern only affects `StyledWrappedText`, not plain text

**Next Investigation Steps:**
1. Check if glyph cache keys include any line number information
2. Trace cell positioning calculations during rendering
3. Look for any place where line number affects character rendering
4. Test if the issue occurs without style spans (plain text with uniform font)

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

### Font color fix
- [ ] Some text is hard-coded with LinearRgba::with_components(0.9, 0.9, 0.9, 1.0), should come from theme.

### Font Variant Perfection
- [ ] Support font changes mid-word