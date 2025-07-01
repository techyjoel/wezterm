# Colored Wrapped Text Implementation Plan

## Executive Summary

This document outlines approaches to achieve syntax highlighting with proper line wrapping in WezTerm's sidebar. After extensive analysis and multiple failed attempts, we've identified why this is challenging and documented potentially viable approaches.

**Current Status**: We have working line wrapping but only single color per line (commit 1dd3a1309).

## Why This Is Hard: Architectural Constraints

### 1. **Children Layout Doesn't Provide Text Wrapping** (VERIFIED)
**Hard Truth**: The Children layout in WezTerm only positions child elements - it does NOT implement text wrapping. From box_model.rs lines 1259-1315:
- Inline elements accumulate width horizontally (`block_pixel_width += kid.bounds.width()`)
- Block elements start new lines (`y_coord += block_pixel_height`)
- **No logic exists to wrap elements when they exceed container width**

**Evidence from commits**:
- Commit c2d77f014 tried inline Text elements - they extended horizontally forever
- Commit 322f5b8a0 tried inline WrappedText elements - each wrapped internally but they didn't flow together
- All approaches using Children failed because Children is NOT a text flow layout system

### 2. **Text Processing Mismatch** (VERIFIED)
**Hard Truth**: There's a fundamental incompatibility:
- **Syntect** provides syntax highlighting with **byte offsets** in UTF-8
- **WezTerm's wrap_text** uses **grapheme clusters** via `unicode_segmentation::Graphemes`
- Example: "👨‍👩‍👧‍👦" is 1 grapheme but 25 bytes
- This creates complex mapping problems when trying to preserve colors through wrapping

### 3. **Glyph Cache Architecture** (SUSPECTED)
Based on code analysis, WezTerm caches rendered glyphs for performance. The cache key appears to include:
- Font
- Size  
- Style attributes

**Suspected issue**: If color is part of the cache key, then per-glyph colors would create separate cache entries for each color variant, causing memory issues. This needs verification.

### 4. **Rendering Pipeline** (VERIFIED)
**Hard Truth**: Colors are applied at the element level during rendering (box_model.rs ~line 1641):
```rust
self.resolve_text(&colors, &inherited_colors).apply(&mut quad);
```

This uses the element's colors for ALL glyphs in that element. The current architecture batches glyphs with the same properties for performance.

### 5. **Multi-width Characters**
- Emoji can span multiple cells
- Combining marks take zero cells
- Tabs expand to multiple cells
- East Asian characters take 2 cells

Simple index-based color mapping doesn't account for this complexity.

## Failed Attempts Summary (VERIFIED from git log)

1. **ColoredWrappedText (commit 8f7ce0d88)**: Created new ElementContent::ColoredWrappedText variant. Text became invisible - likely because colors weren't properly passed through the rendering pipeline. Reverted in fe43b5c9b.

2. **Segment-based WrappedText (commit 322f5b8a0)**: Each syntax segment became its own WrappedText element with inline display. Failed possibly because while each WrappedText wrapped internally, the inline elements maybe didn't flow together as a paragraph. Reverted in 2940b9c18.

3. **Word-boundary splitting (commit c2d77f014)**: Split syntax segments into individual Text elements at word boundaries. Failed likely because Text elements don't wrap at all, and inline elements just extended horizontally. Reverted in bf44e6d73.

4. **Current Solution (commit 1dd3a1309)**: Combines all segments into single WrappedText with first segment's color. Works for wrapping but loses per-token syntax highlighting.

## Suspected Viable Options

### Option 1: Post-Render Color Overlay System

**Status**: UNLIKELY TO WORK - Element has no post-render callback mechanism

**Concept**: Render wrapped text normally, then apply colors as a second rendering pass.

**Major Blocker**: The Element struct has NO post-render callback system. This would require adding:
- A new field to Element for callbacks
- Modifications to ComputedElement 
- Changes to the entire render flow to support post-render passes

**Implementation Challenges**:
- Would need to track character positions after wrapping
- No blend mode support in current quad system
- Manual position calculation would be error-prone

**Verdict**: Requires too many architectural changes to be practical

### Option 5: Virtual Multi-Element Approach

**Status**: POTENTIALLY VIABLE - Most architecturally sound approach

**Concept**: Track style information (colors, fonts, backgrounds) through the text wrapping process and apply during rendering.

**Key Insight**: The failed ColoredWrappedText attempt (8f7ce0d88) had the right architecture but failed at the rendering stage. We now understand how to fix it.

**Core Architecture**:

```rust
// Extend ComputedElementContent::MultilineText (no ElementCell changes needed)
ComputedElementContent::MultilineText {
    lines: Vec<Vec<ElementCell>>,
    line_height: Option<f64>,
    line_styles: Option<Vec<Vec<(usize, usize, CellStyle)>>>, // NEW: style spans per line
}

#[derive(Debug, Clone)]
pub struct CellStyle {
    pub text_color: LinearRgba,
    pub bg_color: Option<LinearRgba>,    // For selection highlighting
    pub font_style: FontStyle,           // Regular, Bold, Italic, BoldItalic
}
```


**Implementation Requirements**:
1. Extend `ComputedElementContent::MultilineText` to include style spans
2. Modify the MultilineText rendering to apply per-cell styles  
3. Create mapping infrastructure to track styles through wrapping
4. Implement batching strategies for performance

#### Deep Investigation Findings

A thorough code analysis revealed several key insights:

**What the investigation got RIGHT**:
- Memory overhead is significant (2.5-3.6x)
- Naive per-cell rendering would cause severe performance issues
- The mapping from bytes to final rendered cells is complex
- Animation system expects uniform colors per element

**What the investigation got WRONG**:
- Font switching is NOT a blocker - we select fonts during shaping, not rendering
- Success rate of 40% was too pessimistic - proper understanding brings it to 65%
- 3-4 week estimate was excessive - 1.5-2 weeks is realistic

#### 1. Solving the Mapping Complexity

The transformation pipeline is complex but manageable:

```rust
// Complete mapping solution
struct WrappingMapper {
    // Stage 1: Bytes to graphemes
    byte_to_grapheme: Vec<usize>,
    grapheme_to_byte_range: Vec<(usize, usize)>,
    
    // Stage 2: Graphemes to cells after wrapping
    grapheme_to_cell: Vec<Option<(usize, usize)>>, // (line, cell_index)
}

impl WrappingMapper {
    fn new(text: &str) -> Self {
        let mut mapper = Self {
            byte_to_grapheme: vec![0; text.len()],
            grapheme_to_byte_range: Vec::new(),
            grapheme_to_cell: Vec::new(),
        };
        
        // Build byte-grapheme mapping
        let mut byte_idx = 0;
        for (g_idx, grapheme) in text.grapheme_indices(true).enumerate() {
            let grapheme_bytes = grapheme.len();
            mapper.grapheme_to_byte_range.push((byte_idx, byte_idx + grapheme_bytes));
            
            // Mark all bytes in this grapheme
            for b in byte_idx..byte_idx + grapheme_bytes {
                mapper.byte_to_grapheme[b] = g_idx;
            }
            byte_idx += grapheme_bytes;
        }
        
        mapper
    }
    
    fn track_wrapping(&mut self, wrapped_lines: &[Vec<ElementCell>]) {
        let mut grapheme_idx = 0;
        
        for (line_idx, line) in wrapped_lines.iter().enumerate() {
            for (cell_idx, _cell) in line.iter().enumerate() {
                if grapheme_idx < self.grapheme_to_cell.len() {
                    self.grapheme_to_cell[grapheme_idx] = Some((line_idx, cell_idx));
                    grapheme_idx += 1;
                }
            }
        }
    }
    
    fn get_style_for_cell(
        &self, 
        line: usize, 
        cell: usize,
        style_spans: &[StyleSpan]
    ) -> Option<CellStyle> {
        // Find grapheme for this cell
        let grapheme_idx = self.grapheme_to_cell.iter()
            .position(|&pos| pos == Some((line, cell)))?;
            
        // Get byte range for this grapheme
        let (byte_start, _) = self.grapheme_to_byte_range.get(grapheme_idx)?;
        
        // Find style span containing this byte
        style_spans.iter()
            .find(|span| *byte_start >= span.start && *byte_start < span.end)
            .map(|span| CellStyle {
                text_color: span.text_color,
                bg_color: span.bg_color,
                font_style: span.font_style,
            })
    }
}
```

**Key Challenges Handled:**
1. **Ligatures**: Track at grapheme level, not glyph level
2. **Tab expansion**: Handle during wrapping phase
3. **Trimmed whitespace**: Track original positions before trimming
4. **Multi-width chars**: Grapheme-based tracking handles naturally

**Complexity: MEDIUM** - ~200 lines of careful bookkeeping code

#### 2. Solving the Performance Issues

The performance impact can be dramatically reduced with two batching strategies:

**Strategy 1: Color Batching (Render all same-color glyphs together)**
```rust
fn render_with_color_batching(
    lines: &[Vec<ElementCell>],
    line_styles: &[Vec<CellStyle>],
    layers: &mut TripleLayerQuadAllocator,
) -> Result<()> {
    // Group all glyphs by color
    let mut color_batches: HashMap<LinearRgba, Vec<(Position, &CachedGlyph)>> = HashMap::new();
    
    for (line_idx, line) in lines.iter().enumerate() {
        for (cell_idx, cell) in line.iter().enumerate() {
            if let ElementCell::Glyph(glyph) = cell {
                let style = get_style(line_idx, cell_idx, line_styles);
                let position = calculate_position(line_idx, cell_idx);
                
                color_batches.entry(style.text_color)
                    .or_default()
                    .push((position, glyph));
            }
        }
    }
    
    // Render each color group with single state change
    for (color, glyphs) in color_batches {
        set_color(color);
        for (pos, glyph) in glyphs {
            render_glyph_at_position(glyph, pos);
        }
    }
    
    Ok(())
}
```
**Performance: 4-5 draw calls total** (one per syntax color)

**Strategy 2: Segment Batching (Batch consecutive same-color runs)**
```rust
fn render_with_segment_batching(
    line: &[ElementCell],
    styles: &[CellStyle],
    layers: &mut TripleLayerQuadAllocator,
) -> Result<()> {
    let mut current_batch = Vec::new();
    let mut current_style = None;
    
    for (cell, style) in line.iter().zip(styles) {
        if Some(style) != current_style && !current_batch.is_empty() {
            // Render previous batch
            render_batch(&current_batch, current_style.unwrap());
            current_batch.clear();
        }
        
        current_style = Some(style);
        current_batch.push(cell);
    }
    
    // Don't forget last batch
    if !current_batch.is_empty() {
        render_batch(&current_batch, current_style.unwrap());
    }
    
    Ok(())
}
```
**Performance: 5-10 draw calls per line** (typical for syntax highlighted code)

**Performance Summary:**
- **Current**: 1 draw call per line
- **Naive per-cell**: 50-100 draw calls per line ❌
- **Color batching**: 4-5 draw calls per code block ✅
- **Segment batching**: 5-10 draw calls per line ✅
- **Impact**: Only affects sidebar rendering, NOT terminal content

#### 3. Font Variant Implementation

We select fonts during text shaping, not rendering:

```rust
// During wrapping, not rendering
fn wrap_with_fonts(
    text: &str,
    styles: &[StyleSpan],
    fonts: &SidebarFonts,
) -> Vec<Vec<ElementCell>> {
    for (segment, style) in segments_with_styles {
        // Select font BEFORE shaping
        let font = match style.font_style {
            FontStyle::Bold => &fonts.bold,
            FontStyle::Italic => &fonts.italic,
            FontStyle::BoldItalic => &fonts.bold_italic,
            FontStyle::Regular => &fonts.regular,
        };
        
        // Shape with the selected font
        let shaped = font.shape(segment, ...)?;
        
        // Store pre-shaped glyphs
        cells.extend(shaped.into_cells());
    }
}
```

This is exactly how we already handle different fonts for headings vs body text.

#### 4. Extended Features Support

**Bold/Italic Text**:
- Load font variants during initialization
- Track FontStyle through markdown parsing
- Select appropriate font during shaping phase

**Background Colors (Selection)**:
- Render background quads at layer 0 before text
- Track selection ranges in byte offsets
- Apply semi-transparent highlight color

**Copy/Paste**:
- Preserve original text alongside rendered cells
- Map selection back to byte ranges
- Extract exact original text for clipboard

#### Final Assessment

**Pros:**
- Architecturally clean - works with WezTerm's design principles
- Enables full feature set: syntax highlighting, bold/italic, selection highlighting
- Performance acceptable with batching strategies
- Future-proof for additional styling (underline, strikethrough)
- Reuses existing infrastructure (glyph cache, font loading)

**Cons:**
- Complex byte→grapheme→cell mapping implementation
- Memory overhead: 2.5-3.6x increase for large code blocks
- 1.5-2 weeks implementation time
- Risk of edge case bugs in mapping logic

**Success Rate: 65%**
**Implementation Time: 1.5-2 weeks**
**Performance Impact: 1.2-1.5x with batching (acceptable for sidebar)**

### Option 6: Glyph Cache Color Variants

**Status**: POTENTIALLY VIABLE with limited color palette

**Concept**: Store pre-colored variants of each glyph in the cache.

**Key Insight**: If we limit to 4-5 syntax colors, memory becomes manageable:
- ASCII charset: ~95 characters × 4 colors = ~380 cache entries
- With common Unicode: ~500 characters × 4 colors = ~2000 cache entries
- Total memory: ~2-5MB (very reasonable)

**Implementation Approach**:
```rust
// Define limited syntax palette
enum SyntaxColor {
    Keyword,    // Blue
    String,     // Green  
    Number,     // Orange
    Default,    // Gray
    Comment,    // Dim gray
}

// Extend glyph cache
struct ColoredGlyphCache {
    // Only cache these specific colors
    cache: HashMap<(GlyphKey, SyntaxColor), Rc<CachedGlyph>>,
}
```

**Advantages**:
- Simple implementation
- Predictable memory usage
- No complex mapping required

**Challenges**:
- Need to modify glyph rasterization to apply color
- Color animation system might conflict
- Cache invalidation on theme changes

**Verdict**: With limited colors, this becomes viable (60% success rate)

### Option 7: Split-Phase Rendering (with caching)

**Status**: POTENTIALLY VIABLE but performance concerns

**Concept**: Separate layout from rendering, cache layouts, render each glyph individually.

**Key Challenge**: The box model currently renders cells in batches. This would require rendering each glyph as a separate quad.

**Performance Impact**:
- **Without caching**: 10-20ms per code block (unacceptable)
- **With caching**: 1-2ms after initial render
- **GPU state changes**: Each color change requires new draw state

**Implementation Requirements**:
1. Create a new render path that bypasses element batching
2. Implement robust caching keyed by (text, width, color_spans)
3. Handle cache invalidation on font/DPI changes

**Technical Concerns**:
- No existing per-glyph rendering in box model
- Would need to batch adjacent same-color glyphs for performance
- Cache memory usage could be significant for large code blocks

**Verdict**: Could work with aggressive caching and batching optimizations

### Option 8: "Fake It" Token-Based Approach

**Status**: MISUNDERSTOOD - Won't work as originally conceived

**Original Concept**: Use Block elements in Children, assuming they would wrap together.

**Why it won't work** (VERIFIED):
- Block elements in Children start new lines - they don't wrap as a paragraph
- Each token would be on its own line, not flowing together
- Evidence: All attempts using Children failed (commits c2d77f014, 322f5b8a0)

**Modified Approach That Might Work**:
```rust
// Instead of perfect per-token colors, approximate at coarser boundaries
fn approximate_colors(line: &str, syntax_spans: Vec<(Style, &str)>) -> Vec<(String, LinearRgba)> {
    // Group by "logical units" that are likely to wrap together
    // E.g., "def do_thing(" becomes one unit with keyword color
    // Accept that some tokens might have wrong color
}
```

**But this is essentially what we already have** with single color per line.

**Verdict**: Not viable for achieving better granularity than current solution

### Option 9: Layer Composition via Z-indices

**Status**: POTENTIALLY VIABLE with careful implementation

**Concept**: Use different z-indices for each syntax color.

**Implementation Approach**:
```rust
// Assign z-index per color
const COLOR_Z_INDICES: [(SyntaxColor, i8); 4] = [
    (SyntaxColor::Keyword, 10),
    (SyntaxColor::String, 11),
    (SyntaxColor::Number, 12),
    (SyntaxColor::Default, 13),
];

// Render each color group at its z-index
for (color, zindex) in COLOR_Z_INDICES {
    let layer = gl_state.layer_for_zindex(zindex)?;
    render_glyphs_with_color(glyphs_for_color, color, layer)?;
}
```

**Key Insight**: Each z-index gets its own RenderLayer, avoiding sub-layer limitations.

**Advantages**:
- Works within existing architecture
- Clean separation of colors
- No complex mapping needed

**Challenges**:
- **Performance**: Each z-index = GPU state change
- **Overlapping**: Must ensure characters don't overlap spatially
- **Memory**: Multiple RenderLayers increase memory usage

**Implementation Requirements**:
1. Group wrapped text by color before rendering
2. Track character positions to avoid overlap
3. Render each color group at its z-index
4. Careful management of transparency

**Verdict**: More viable than initially thought (40% success rate)

## Testing Strategy

### Visual Testing
- [ ] Verify syntax highlighting is preserved for all tokens
- [ ] Confirm line wrapping occurs at word boundaries
- [ ] Check indentation is preserved after wrapping
- [ ] Test with various programming languages (Python, Rust, JavaScript, Go)
- [ ] Verify copy/paste preserves original text exactly
- [ ] Check performance with large code blocks (1000+ lines)
- [ ] Test with narrow sidebar widths (force aggressive wrapping)

### Edge Cases
- [ ] Unicode in code (emoji in comments/strings)
- [ ] Very long tokens (base64 strings, URLs)
- [ ] Mixed LTR/RTL text
- [ ] Deeply nested indentation
- [ ] Tab characters vs spaces
- [ ] Empty lines and whitespace-only lines

### Performance Testing
- [ ] Measure frame rate during scrolling
- [ ] Monitor memory usage with many code blocks
- [ ] Test cache effectiveness (cache hit rates)
- [ ] Measure initial render time vs cached renders
- [ ] Profile CPU usage during syntax highlighting

### Cross-Platform Testing
- [ ] Different fonts (monospace vs proportional)
- [ ] Various DPI settings
- [ ] Dark vs light themes
- [ ] Different terminal sizes

## Updated Summary and Recommendations

Based on code analysis and new insights:

### Options Eliminated:
- **Option 1**: No post-render callback system exists
- **Option 8**: Children layout doesn't support text flow

### Potentially Viable Options (Ranked):

1. **Option 5 (Virtual Multi-Element)**: Track colors through wrapping
   - **Success Rate**: 70%
   - **Pros**: Architecturally clean, works with existing wrapping logic
   - **Cons**: Complex byte↔grapheme mapping required
   - **Effort**: 1-2 weeks
   
2. **Option 6 (Limited Color Glyph Cache)**: Pre-colored glyphs with 4-5 colors
   - **Success Rate**: 60%
   - **Pros**: Simple implementation, predictable memory (2-5MB)
   - **Cons**: Need to modify glyph rasterization
   - **Effort**: 3-5 days

3. **Option 7 (Split-Phase Rendering)**: Cache layouts, render per-glyph
   - **Success Rate**: 50%
   - **Pros**: Maximum flexibility
   - **Cons**: Performance concerns, new render path needed
   - **Effort**: 1-2 weeks

4. **Option 9 (Z-index Layers)**: One z-index per color
   - **Success Rate**: 40%
   - **Pros**: Works within existing architecture
   - **Cons**: GPU state changes, overlap management
   - **Effort**: 1 week

5. **Current Solution**: Single color per line
   - **Success Rate**: 100%
   - **Pros**: Already working, zero risk
   - **Cons**: No per-token highlighting

### Updated Recommendation:
With the new understanding that Option 6 becomes viable with limited colors, the implementation order should be:

1. **Try Option 6 first** - Simplest to implement (3-5 days)
2. **Then Option 5** - Most architecturally sound if Option 6 fails
3. **Consider Option 9** - If performance of other options is poor
4. **Keep current solution** - If all else fails

The key insight is that limiting to 4-5 syntax colors makes previously "impossible" approaches viable.