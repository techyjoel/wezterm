# Horizontal Scrolling Removal and Line Wrapping Implementation Plan

## Implementation Status: ✅ REMOVAL COMPLETE, 🔧 WRAPPING NEEDS WORK

**Summary**: Horizontal scrolling has been successfully removed and basic line wrapping implemented. However, several issues remain with code block rendering that need to be addressed.

## Current Issues to Fix

### 1. **Code Block Indentation Lost** 🟡 FIXED BUT MADE WRAPPING ISSUE
**Problem**: Leading whitespace/indentation was being stripped from code blocks
**Symptom**: Python code showed `def`, `try:`, `with`, etc. all left-aligned
**Root Cause**: 
- The `wrap_text` function in `box_model.rs` was skipping consecutive spaces (lines 785-787)
- This stripped leading indentation when processing text for wrapping
**Fix Applied**: 
- Modified `wrap_text` to preserve leading spaces by counting them separately
- Added logic to prepend indentation to each line before processing words
- Lines 772-843 in `wezterm-gui/src/termwindow/box_model.rs`

### 2. **Syntax Coloring Simplified** 🟡 FIXED BUT CAUSES ISSUES
**Problem**: Code blocks were only using a single color instead of full syntax highlighting
**Current State**: Full syntax highlighting now preserved across wrapped lines
**Solution Implemented**: 
- Changed from `WrappedText` to `Element::Children` with inline display
- Each syntax segment is rendered as a separate inline element that wraps naturally
- Lines 445-467 in `wezterm-gui/src/sidebar/components/markdown.rs`
**How It Works**:
- Syntax segments maintain their original colors
- Inline display allows segments to wrap at word boundaries
- Parent block element handles line height and margin settings

### 3. **Modal Clipping Issues** 🟡 MEDIUM - ARCHITECTURAL LIMITATION - NEED TO FIX MODAL BACKGROUND
**Problem**: Modal content renders outside the modal background bounds
**Symptom**: Text appears above/below the modal's visual container
**Root Cause**: 
- WezTerm's Element system doesn't support true clipping/scissor rectangles
- The GPU-based rendering happens in a later pass, preventing parent-based clipping
- Elements are positioned absolutely and can render anywhere
**Current Workaround**:
- Using "cut-a-hole" pattern with higher z-index frame sections
- Modal background at z-index 22, content at z-index 21
- This only hides content behind the frame, doesn't truly clip
**Note**: This is a known architectural limitation that would require significant changes to fix properly. For now, we need to extend the modal background all the way to the top/bottom of the window with a color that fades from the modal background to some darker color at the top and bottom of the modal, to look like a shadow.

### 4. **Text Styling Partially Implemented** 🟢 LOW
**Problem**: Bold and italic text formatting not fully implemented
**Current State**:
- Bold headings ARE be working (using separate heading font)
- Bold/italic in paragraphs NOT working (TODOs in code)
- Lists structure working but no bullet/number rendering
- Inline code shows with backticks but no styling
**Implementation Path**:
- WezTerm supports FontWeight and FontStyle (see `config/src/font.rs`)
- SidebarFonts struct only has heading/body/code fonts currently
- Would need to load bold/italic variants and apply based on emphasis stack
**Key Files**: 
- `wezterm-gui/src/sidebar/components/markdown.rs` (lines 298-312)
- Currently has TODOs for applying font variants

## What Has Been Working Well
- ✅ Line wrapping for long lines
- ✅ Newline preservation between logical lines  
- ✅ Configurable line height and margin
- ✅ Copy button functionality
- ✅ Vertical scrolling and scrollbar dragging
- ✅ Basic markdown structure (headings, paragraphs, code blocks)

## Implementation Details for Completed Fixes

### Fix 1: Preserve Code Indentation ✅
**Implementation**:
- Added leading space detection: `line_text.len() - line_text.trim_start().len()`
- Store indentation separately: `indentation = " ".repeat(leading_spaces)`
- Prepend indentation to line before processing words
- Skip leading spaces during word parsing to avoid duplication

### Implementation Approach Notes for Remaining Issues

### Text Styling Enhancement
1. Load bold/italic font variants in sidebar font initialization
2. Modify markdown renderer to:
   - Check emphasis stack when creating text elements
   - Switch fonts based on Bold/Italic state
   - Apply appropriate font variant to each text segment
3. Add bullet/number rendering for lists
4. Style inline code with code font and background color


## Refined Color-Aware Text Wrapping Implementation Plan

### Overview
Extend WezTerm's `wrap_text` algorithm to preserve syntax highlighting colors through line wrapping. This plan addresses all issues discovered in previous attempts and provides a clear path to full per-grapheme color support.

### Critical Insights from Failed Attempts
1. **Inline elements don't wrap** - WezTerm's box model lacks CSS-like inline wrapping
2. **ColoredWrappedText (8f7ce0d88) had the right idea** - Failed due to rendering pipeline bug, not design flaw
3. **Colors must flow through to quad rendering** - The missing piece in previous attempts

### 1. Simplified Data Structures

```rust
// Store color spans alongside text (byte ranges)
#[derive(Debug, Clone)]
pub struct ColorSpan {
    pub start: usize,      // Byte offset in string
    pub end: usize,        // Byte offset in string  
    pub color: LinearRgba,
}

// Add new ElementContent variant - minimal change
#[derive(Debug, Clone)]
pub enum ElementContent {
    // ... existing variants ...
    ColoredWrappedText {
        text: String,
        spans: Vec<ColorSpan>,
    },
}

// NO CHANGES to ElementCell - avoid breaking existing code
// ElementCell stays as is:
// pub enum ElementCell {
//     Sprite(Sprite),
//     Glyph(Rc<CachedGlyph>),
// }

// Instead, track colors at the ComputedElement level
// Add color tracking to MultilineText variant
#[derive(Debug, Clone)]
pub enum ComputedElementContent {
    // ... existing variants ...
    MultilineText {
        lines: Vec<Vec<ElementCell>>,
        line_height: Option<f64>,
        // NEW: Optional color spans per line for rendering
        line_colors: Option<Vec<Vec<(usize, usize, LinearRgba)>>>, // cell_start, cell_end, color
    },
}
```

### 2. Core Implementation Strategy

#### Phase 1: Update Rendering Pipeline
The issue wasn't that colors weren't passed to quads - WezTerm already does this correctly via `resolve_text()`. The real issue is tracking colors through the wrapping process.

```rust
// In box_model.rs, modify MultilineText rendering (around line 1638):
ComputedElementContent::MultilineText { lines, line_colors, .. } => {
    for (line_idx, line) in lines.iter().enumerate() {
        // Get color spans for this line if available
        let color_spans = line_colors.as_ref()
            .and_then(|lc| lc.get(line_idx));
        
        for (cell_idx, cell) in line.iter().enumerate() {
            // Check if this cell has a specific color
            let cell_color = color_spans
                .and_then(|spans| {
                    spans.iter().find(|(start, end, _)| {
                        cell_idx >= *start && cell_idx < *end
                    }).map(|(_, _, color)| *color)
                })
                .unwrap_or_else(|| self.colors.text);
            
            match cell {
                ElementCell::Glyph(glyph) => {
                    let mut quad = layers.allocate(layer_num)?;
                    // Use cell-specific color instead of element color
                    let colors = ElementColors {
                        text: cell_color.into(),
                        ..self.colors.clone()
                    };
                    self.resolve_text(&colors, &inherited_colors).apply(&mut quad);
                    // ... rest of glyph rendering ...
                }
                // ... handle Sprite case ...
            }
        }
    }
}
```

#### Phase 2: Implement Color-Aware Text Wrapping

```rust
// Extend existing wrap_text to track color spans
pub fn wrap_colored_text(
    &self,
    line_text: &str,
    spans: &[ColorSpan],
    font: &Rc<LoadedFont>,
    max_width: f32,
) -> (Vec<Vec<ElementCell>>, Vec<Vec<(usize, usize, LinearRgba)>>) {
    // Key insight: Reuse existing wrap_text logic but track byte->cell mapping
    let mut wrapped_lines = Vec::new();
    let mut line_color_spans = Vec::new();
    let mut current_line = Vec::new();
    let mut current_line_colors = Vec::new();
    let mut line_pixel_width = 0.0;
    let mut byte_offset = 0;
    let mut cell_in_line = 0;
    
    // Helper to find color at byte offset
    let color_at_byte = |offset: usize| -> LinearRgba {
        spans.iter()
            .find(|span| offset >= span.start && offset < span.end)
            .map(|span| span.color)
            .unwrap_or(LinearRgba::with_components(0.9, 0.9, 0.9, 1.0))
    };
    
    // Track spans for current line
    let mut add_color_span = |start_cell: usize, end_cell: usize, color: LinearRgba| {
        if let Some(last) = current_line_colors.last_mut() {
            if last.2 == color && last.1 == start_cell {
                // Extend previous span
                last.1 = end_cell;
                return;
            }
        }
        current_line_colors.push((start_cell, end_cell, color));
    };
    
    // Process text preserving indentation
    let trimmed_start = line_text.trim_start();
    let leading_whitespace_len = line_text.len() - trimmed_start.len();
    let leading_whitespace = &line_text[..leading_whitespace_len];
    
    // Handle leading whitespace
    if !leading_whitespace.is_empty() {
        let color = color_at_byte(0);
        for ch in leading_whitespace.chars() {
            let ch_str = if ch == '\t' { "    " } else { &ch.to_string() };
            let shaped = font.shape(ch_str, ...)?;
            
            for info in shaped {
                let glyph = self.cached_glyph(&info, ...)?;
                current_line.push(ElementCell::Glyph(glyph));
                line_pixel_width += glyph.width;
            }
            
            let cells_added = shaped.len();
            add_color_span(cell_in_line, cell_in_line + cells_added, color);
            cell_in_line += cells_added;
            byte_offset += ch.len_utf8();
        }
    }
    
    // Process words with existing wrap_text logic
    let trimmed_line = line_text.trim();
    let words = trimmed_line.split_word_bounds();
    
    for word in words {
        let color = color_at_byte(byte_offset);
        let shaped = font.shape(word, ...)?;
        let word_width = shaped.width();
        
        // Check if word fits
        if line_pixel_width + word_width > max_width && !current_line.is_empty() {
            // Finish current line
            wrapped_lines.push(current_line);
            line_color_spans.push(current_line_colors);
            current_line = Vec::new();
            current_line_colors = Vec::new();
            line_pixel_width = 0.0;
            cell_in_line = 0;
        }
        
        // Add word cells
        let start_cell = cell_in_line;
        for info in shaped {
            let glyph = self.cached_glyph(&info, ...)?;
            current_line.push(ElementCell::Glyph(glyph));
            cell_in_line += 1;
        }
        add_color_span(start_cell, cell_in_line, color);
        
        line_pixel_width += word_width;
        byte_offset += word.len();
    }
    
    // Don't forget last line
    if !current_line.is_empty() {
        wrapped_lines.push(current_line);
        line_color_spans.push(current_line_colors);
    }
    
    (wrapped_lines, line_color_spans)
}
```

#### Phase 3: Integration with compute_element

```rust
// In compute_element function (box_model.rs ~line 1200)
ElementContent::ColoredWrappedText { text, spans } => {
    let (wrapped_lines, color_spans) = self.wrap_colored_text(
        &text,
        &spans,
        &self.font,
        available_width,
    )?;
    
    ComputedElementContent::MultilineText {
        lines: wrapped_lines,
        line_height: self.line_height,
        line_colors: Some(color_spans),
    }
}
```

### 3. Markdown Integration

```rust
// In markdown.rs highlight_code_block method
fn highlight_code_block(/* ... */) -> Element {
    // ... existing syntax highlighting code ...
    
    for line in LinesWithEndings::from(code) {
        let ranges = highlighter.highlight_line(line, &self.syntax_set)?;
        
        // Build color spans with byte offsets
        let mut spans = Vec::new();
        let mut byte_offset = 0;
        let mut full_line = String::new();
        
        for (style, text) in ranges {
            let start = byte_offset;
            let end = byte_offset + text.len();
            
            spans.push(ColorSpan {
                start,
                end,
                color: LinearRgba::with_components(
                    style.foreground.r as f32 / 255.0,
                    style.foreground.g as f32 / 255.0,
                    style.foreground.b as f32 / 255.0,
                    style.foreground.a as f32 / 255.0,
                ),
            });
            
            full_line.push_str(text);
            byte_offset = end;
        }
        
        // Create element with colored wrapped text
        let line_element = Element::new(
            font, 
            ElementContent::ColoredWrappedText {
                text: full_line,
                spans,
            }
        )
        .display(DisplayType::Block)
        .line_height(Some(code_line_height))
        .margin(BoxDimension {
            bottom: Dimension::Pixels(code_line_margin as f32),
            ..Default::default()
        });
        
        line_elements.push(line_element);
    }
}
```

### 4. Critical Implementation Details

#### Avoiding Previous Pitfalls
1. **Don't break ElementCell enum** - Keep it unchanged to avoid breaking existing pattern matches
2. **Track colors separately** - Use line_colors in MultilineText instead of modifying cells
3. **Preserve existing line spacing** - Use same line_height and margin as current implementation
4. **Handle whitespace correctly** - Preserve indentation handling from current wrap_text
5. **Work with byte offsets** - Syntect provides byte ranges, so we track bytes not graphemes

#### Key Insights from Subagent Review
1. **ElementCell must stay unchanged** - Changing from tuple to struct variant breaks ~100 pattern matches
2. **Colors already flow to quads** - The issue was tracking colors through wrapping, not rendering
3. **Shape once per word** - Don't reshape text for each color segment
4. **Ligatures and complex scripts** - Accept that color boundaries might not align perfectly with glyphs

#### Performance Considerations
1. **Linear color lookup is acceptable** - Code blocks rarely exceed 1000 characters
2. **Minimize allocations** - Reuse vectors where possible
3. **Cache shaped results** - The existing glyph cache already handles this

### 5. Testing Strategy

#### Phase 1 Test: Rendering Pipeline
```rust
// Test that MultilineText with line_colors renders correctly
let test_lines = vec![vec![
    ElementCell::Glyph(glyph_h),
    ElementCell::Glyph(glyph_i),
]];
let test_colors = Some(vec![vec![
    (0, 1, red),    // First glyph is red
    (1, 2, blue),   // Second glyph is blue
]]);

// Create ComputedElementContent::MultilineText with line_colors
// Verify each glyph renders with its specified color
```

#### Phase 2 Test: Wrapping Algorithm
```rust
// Test with "def do_thing(123: int):"
let text = "def do_thing(123: int):";
let spans = vec![
    ColorSpan { start: 0, end: 4, color: keyword_color },      // "def "
    ColorSpan { start: 4, end: 12, color: function_color },    // "do_thing"
    ColorSpan { start: 12, end: 13, color: default_color },    // "("
    ColorSpan { start: 13, end: 16, color: number_color },     // "123"
    ColorSpan { start: 16, end: 18, color: default_color },    // ": "
    ColorSpan { start: 18, end: 21, color: type_color },       // "int"
    ColorSpan { start: 21, end: 23, color: default_color },    // "):"
];
// Verify wrapping preserves each segment's color
```

### 6. Implementation Order

1. **Phase 1**: Add ColoredWrappedText support (3-4 hours)
   - Add ColorSpan struct and ElementContent::ColoredWrappedText variant
   - Extend ComputedElementContent::MultilineText with optional line_colors
   - NO changes to ElementCell enum

2. **Phase 2**: Update rendering pipeline (2-3 hours)
   - Modify MultilineText rendering to check line_colors
   - Use color spans to override element colors per cell
   - Test with manually created color spans

3. **Phase 3**: Implement wrap_colored_text (4-5 hours)
   - Create new method alongside existing wrap_text
   - Track byte offsets through wrapping process
   - Handle indentation and whitespace correctly

4. **Phase 4**: Integration (2-3 hours)
   - Update compute_element to handle ColoredWrappedText
   - Modify markdown renderer to use new system
   - Test with real syntax-highlighted code

5. **Phase 5**: Testing and polish (2-3 hours)
   - Edge cases: empty lines, very long tokens
   - Performance profiling
   - Ensure copy/paste still works correctly

### 7. Success Criteria

1. **Full syntax highlighting preserved**: Every token has correct color (e.g., `def` in purple, `do_thing` in blue, `123` in orange)
2. **Proper word-boundary wrapping**: No breaks mid-word, wraps at spaces/punctuation
3. **Maintained line spacing**: Uses configured line_height and margin
4. **No breaking changes**: ElementCell enum unchanged, existing code continues to work
5. **Copy/paste works**: Original text preserved despite color tracking
6. **Performance acceptable**: No noticeable lag when rendering code blocks

### 8. Risk Mitigation

1. **If byte offset tracking proves problematic**: Fall back to character indices with UTF-8 aware conversion
2. **If performance is poor**: Cache the wrapped results keyed by (text, width, spans)
3. **If color boundaries don't align with glyphs**: Accept best-effort coloring for complex scripts
4. **If implementation takes longer**: Each phase is independently useful and can be shipped

This refined plan addresses all concerns raised by the subagent review while maintaining the core goal of achieving full syntax highlighting with proper line wrapping. The key insight is to work WITH WezTerm's existing architecture rather than trying to fundamentally change core data structures.

## Architecture Notes

### Z-Index Layering (Working)
- **Z-index 10**: Activity log content
- **Z-index 12**: Sidebar background with cut-out hole
- **Z-index 14**: Main sidebar content  
- **Z-index 16**: Scrollbars and buttons
- **Z-index 20-23**: Modals

### Key Files Reference
- **Markdown rendering**: `wezterm-gui/src/sidebar/components/markdown.rs`
- **Text wrapping**: `wezterm-gui/src/termwindow/box_model.rs` (wrap_text function)
- **Modal system**: `wezterm-gui/src/sidebar/components/modal/`
- **Activity log**: `wezterm-gui/src/sidebar/ai_sidebar.rs`

## Testing Required

### Visual Testing (Not Yet Complete)
- [ ] Verify code blocks wrap cleanly at sidebar edges
- [ ] Check that copy buttons remain properly positioned after wrapping
- [ ] Test with various programming languages (Python ✓, Rust, JavaScript, etc.)
- [ ] Ensure no visual artifacts from removed clipping code
- [ ] Verify markdown features:
  - [ ] Bold text rendering (`**text**`)
  - [ ] Italic text rendering (`*text*`)
  - [ ] Bulleted lists
  - [ ] Numbered lists
  - [ ] Inline code styling (`code`)
  - [ ] Link styling and hover effects
  - [ ] Heading styles (sizes and weights)
- [ ] Modal scrolling behavior and bounds clipping
- [ ] Filter chip functionality and click detection

### Functional Testing
- [ ] Test with extreme cases:
  - [ ] Very long lines (URLs, base64 strings)
  - [ ] Deeply indented code (5+ levels)
  - [ ] Mixed languages in same block
  - [ ] Empty code blocks
  - [ ] Code blocks with only whitespace
- [ ] Verify copy functionality provides exact original text
- [ ] Test line height/margin configuration changes
- [ ] Cross-platform testing (different fonts/DPI settings)

### Performance Testing  
- [ ] Verify smooth scrolling performance isn't degraded

### Edge Cases to Verify
- [ ] Unicode/emoji in code blocks
- [ ] RTL text handling
- [ ] Very narrow sidebar widths
- [ ] Dark/light theme compatibility