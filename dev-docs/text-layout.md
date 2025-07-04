# Text Layout and Rendering

## Critical Implementation Notes

### Style Span Color Bug (CAUSES INVISIBLE TEXT)
```rust
// WRONG - Results in transparent/invisible text
StyleSpan { colors: ElementColors::Inherited, ... }
ElementColors::default()  // Uses Inherited internally

// CORRECT - Always set explicit colors
StyleSpan { colors: ElementColors::new(Some(fg_color)), ... }
```
**Why**: `Inherited` without a parent context results in transparent text.
**Type**: Current implementation requirement (Element system expects explicit colors)

### Text Layout Architecture (FUNDAMENTAL CONSTRAINT)
- **Must use "wrap-before-shape"** approach
- Shape-then-wrap is impossible because fonts must be selected before shaping
- This is a HarfBuzz/font shaping limitation, not WezTerm specific
**Type**: Architectural requirement (font shaping API constraint)

### Grapheme Cluster Tracking
- Style spans MUST track byte offsets, not character offsets
- Incorrect tracking causes progressive character loss at line wraps
- Always use proper Unicode grapheme iteration
**Type**: Correctness requirement (Unicode handling)

### OutOfTextureSpace Error Handling
- **NEVER** catch and suppress `OutOfTextureSpace` errors
- These errors trigger glyph cache resizing
- Suppressing them breaks text rendering permanently
**Type**: Current implementation requirement (cache management design)

### Font Width Calculation
- **DO NOT** use terminal cell width for proportional fonts
- Must use `calculate_average_char_width()` with actual font
- Character loss occurs without proper width calculation
**Type**: Correctness requirement for proportional fonts

## Overview

Text rendering in WezTerm involves complex interactions between font selection, text shaping, line wrapping, and GPU rendering. This document explains the text layout pipeline and implementation patterns.

## Text Rendering Pipeline

### Standard Flow

1. **Font Selection** - Choose appropriate font (regular, bold, italic, monospace)
2. **Text Shaping** - Convert text to positioned glyphs via HarfBuzz
3. **Line Wrapping** - Break shaped text at line boundaries
4. **GPU Rendering** - Draw glyphs from texture atlas

### The Fundamental Challenge

Traditional flow creates a circular dependency:
- Need font to determine glyph widths
- Need widths to determine line breaks
- Need line breaks to apply style changes
- Style changes may require different fonts

## Text Layout Approaches

### Approach 1: Shape-Then-Wrap (Architecturally Impossible)

The original approach that shapes text with a single font, then wraps:

```rust
// Simplified flow in box_model.rs
let shaped = font.shape(text, params)?;
// Then wrap the shaped text into lines
```

**Why it's impossible:**
- Cannot change fonts after shaping
- No mid-line style changes
- Bold/italic text impossible with wrapping
- This is a fundamental limitation of font shaping APIs

### Approach 2: Wrap-Before-Shape (Current Implementation)

Current implementation that estimates widths, wraps, then shapes each line:

```rust
// See box_model.rs:wrap_styled_text()
let wrapped_lines = wrap_text_with_estimates(text, char_width, max_width);
for line in wrapped_lines {
    let shaped = shape_line_with_styles(line, style_spans, fonts);
}
```

**Key Components:**

1. **Width Estimation** (`calculate_average_char_width()`)
   - Measures actual font metrics
   - Uses representative sample text
   - Applies correction factor (1.05 default)
   - Caches results for performance

2. **Line Wrapping** (`wrap_text_with_estimates()`)
   - Tracks byte offsets for style spans
   - Handles leading space skipping
   - Preserves word boundaries

3. **Styled Shaping** (`shape_line_with_styles()`)
   - Applies correct font per style span
   - Handles span boundaries
   - Maintains color information

### Implementation Details

#### Width Calculation

The system measures actual font character widths:
```rust
// Monospace optimization
if font.is_monospace() {
    return cell_size.width; // Use terminal cell width
}

// Proportional fonts use a representative English text sample
let shaped = font.shape(SAMPLE_TEXT, params)?;
let total_width = shaped.iter().sum();
let raw_avg_width = total_width / SAMPLE_TEXT.len() as f32;
let avg_width = raw_avg_width * WIDTH_CORRECTION_FACTOR;
```

#### Style Span Tracking

Style spans map byte ranges to visual styles:
```rust
pub struct StyleSpan {
    pub start: usize,              // Start byte offset
    pub end: usize,                // End byte offset
    pub colors: ElementColors,     // Colors for this span
    pub font: Option<Rc<LoadedFont>>, // Optional font override
    pub font_style: Option<FontStyleFlags>, // Bold/italic flags
}
```

#### Grapheme Cluster Handling

Proper Unicode support requires careful grapheme tracking:
- Track byte offsets for span boundaries
- Map grapheme clusters to cells
- Handle combining characters
- Account for wide characters

## Syntax Highlighting

### Integration with Syntect

Code blocks use syntect for tokenization:

1. **Language Detection** - Markdown parser identifies language
2. **Theme Creation** - WezTerm palette converted to syntect theme
3. **Tokenization** - Code parsed into syntax tokens
4. **Style Mapping** - Tokens mapped to color spans

See `markdown.rs:highlight_code_block()` for implementation.

### Theme Integration

Syntax colors derived from active WezTerm theme:
```rust
fn create_syntect_theme_from_palette(palette: &ColorPalette) -> syntect::highlighting::Theme {
    let mut theme = Theme::default();
    theme.settings.foreground = Some(palette.foreground);
    theme.settings.background = Some(palette.background);
    // Map ANSI colors to syntax scopes
}
```

## Markdown Rendering

### Architecture

The markdown system uses pulldown-cmark with custom rendering:

1. **Parsing** - Markdown to events via pulldown-cmark
2. **Event Processing** - Build Element tree from events
3. **Style Application** - Font variants and colors
4. **Code Highlighting** - Syntax highlighting for code blocks

Key files:
- `sidebar/components/markdown.rs` - Main implementation
- `MarkdownRenderer` - Stateless renderer methods
- `MarkdownContext` - Rendering state and configuration

### Style Patterns

Markdown elements map to font variants:
- **Bold** → `heading` font (Roboto Bold)
- *Italic* → `body` font with italic variant
- `Code` → `code` font with background

Inline styles can combine (bold italic).

## Common Issues and Solutions

### Issue: Invisible Styled Text

**Problem:** Bold/italic text renders transparent
**Cause:** Style spans using `Inherited` color without parent
**Solution:** Set explicit colors in style spans

### Issue: Progressive Character Loss

**Problem:** Characters disappear at line wrap points
**Cause:** Incorrect grapheme-to-cell mapping for skipped spaces
**Solution:** Track wrapped lines properly in `track_wrapping_with_lines()`

### Issue: Text Clipping

**Problem:** Text cut off at element boundaries
**Cause:** Content bounds smaller than rendered text
**Solution:** Remove artificial boundary constraints, allow overflow into padding

### Issue: Font Width Estimation

**Problem:** Text overflows or underflows target width
**Cause:** Inaccurate width estimates for proportional fonts
**Solution:** Measure actual font metrics with correction factor

## Performance Optimizations

### Font Measurement Caching

Width calculations cached per font:
```rust
thread_local! {
    static FONT_WIDTH_CACHE: RefCell<HashMap<FontId, f32>> = RefCell::new(HashMap::new());
}
```

### Monospace Fast Path

Skip width calculation when all style spans use monospace fonts:
```rust
let is_monospace_only = style_spans.iter().all(|span| span.is_monospace());
if is_monospace_only {
    return cell_size.width;
}
```

### Syntax Highlighting Cache

Cache syntax highlighter instances per language to avoid re-initialization.

## Configuration

### User Settings

In wezterm.lua:
```lua
config.clibuddy.right_sidebar.fonts = {
    syntax_dimming_factor = 0.7,  -- Syntax color intensity
    families = {
        heading = "Roboto",       -- Bold headers
        body = "Roboto",          -- Regular text
        code = "JetBrains Mono"   -- Code blocks
    }
}
```

### Width Correction Factor

Fine-tune width estimation:
```rust
const WIDTH_CORRECTION_FACTOR: f32 = 1.02; // 2% extra width
```

Consider making this configurable for different font combinations.

## Future Improvements

### Render-to-Texture

For perfect text clipping in scrollable regions:
1. Render full text to off-screen texture
2. Display viewport portion via UV coordinates
3. Enables pixel-perfect scrolling

### Advanced Layout

Consider integrating a full text layout engine:
- Pango or DirectWrite for complex scripts
- Better bidirectional text support
- Advanced typography features

### Performance

- Incremental re-shaping for edits
- Parallel shaping for long documents
- GPU-accelerated text layout

## Testing Text Layout

### Test Cases

1. **Line Wrapping**
   - Very long words requiring character breaks
   - Mixed wide/narrow characters
   - Unicode edge cases

2. **Style Changes**
   - Mid-word style transitions
   - Nested styles (bold italic)
   - Style at line boundaries

3. **Performance**
   - Large documents
   - Many style changes
   - Rapid re-layouts

### Debug Helpers

Enable debug logging:
```bash
WEZTERM_LOG=debug ./target/release/wezterm
```

Look for:
- "Wrapped line" - Line break decisions
- "Style span" - Style application
- "Font width" - Width calculations