# Text Wrapping Implementation Plan

## Overview

This document outlines the implementation plan for adding text wrapping support to WezTerm's Element rendering system. The goal is to fix text wrapping issues in the AI sidebar where long text in suggestions, goals, chat messages, and markdown content gets truncated instead of wrapping to multiple lines.

## Problem Statement

Currently, `ElementContent::Text` renders on a single line and truncates when reaching `max_width`. This affects:
- AI suggestions in the sidebar
- Chat messages between user and AI
- Goal descriptions
- Markdown paragraphs
- Command output in activity logs

## Solution: Add WrappedText Variant

Add a new `ElementContent::WrappedText` variant that automatically wraps text at word boundaries, falling back to character boundaries for words that exceed the maximum width. This hybrid approach handles both natural language and terminal output with long strings. The existing `Text` variant remains unchanged for backward compatibility.

## Implementation Steps

### 1. Update Data Structures

**File: `wezterm-gui/src/termwindow/box_model.rs`**

```rust
// Update ElementContent enum (line ~466)
#[derive(Debug, Clone)]
pub enum ElementContent {
    Text(String),
    WrappedText(String),  // Automatically wraps at word boundaries, falling back to character boundaries
    Children(Vec<Element>),
    Poly { line_width: isize, poly: SizedPoly },
}

// Update ComputedElementContent enum (line ~550)
#[derive(Debug, Clone)]
pub enum ComputedElementContent {
    Text(Vec<ElementCell>),
    MultilineText {
        lines: Vec<Vec<ElementCell>>,
        line_height: f32,
    },
    Children(Vec<ComputedElement>),
    Poly {
        line_width: isize,
        poly: PixelSizedPoly,
    },
}
```

### 2. Implement Word Wrapping Logic

**File: `wezterm-gui/src/termwindow/box_model.rs`**

Add new method in impl TermWindow (around line 600):

```rust
/// Wraps text at word boundaries to fit within max_width, with character-level fallback
fn wrap_text(
    &self,
    text: &str,
    font: &Rc<LoadedFont>,
    max_width: f32,
    context: &LayoutContext,
) -> anyhow::Result<Vec<Vec<ElementCell>>> {
    let mut lines = Vec::new();
    let mut current_line = Vec::new();
    let mut current_width = 0.0;
    
    // Split by whitespace while preserving it
    let words = text.split_inclusive(' ');
    
    for word in words {
        // Shape the word to get its width
        let word_infos = font.shape(
            word,
            || {}, // notification callback
            BlockKey::filter_out_synthetic,
            None,  // presentation
            wezterm_bidi::Direction::LeftToRight,
            None,  // range
            None,  // direction override
        )?;
        
        // Calculate word width
        let word_width = self.calculate_text_width(&word_infos, font, context)?;
        
        // Check if word fits on current line
        if current_width > 0.0 && current_width + word_width > max_width {
            // Finalize current line
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = Vec::new();
                current_width = 0.0;
            }
        }
        
        // If word itself is too wide, break it at character boundaries
        if word_width > max_width {
            let cells = self.shape_text_to_cells(&word_infos, font, context)?;
            let mut char_line = Vec::new();
            let mut char_width = 0.0;
            
            for cell in cells {
                let cell_width = self.get_cell_width(&cell, font, context)?;
                
                if char_width + cell_width > max_width && !char_line.is_empty() {
                    lines.push(char_line);
                    char_line = Vec::new();
                    char_width = 0.0;
                }
                
                char_line.push(cell);
                char_width += cell_width;
            }
            
            if !char_line.is_empty() {
                current_line = char_line;
                current_width = char_width;
            }
        } else {
            // Word fits, add it to current line
            let cells = self.shape_text_to_cells(&word_infos, font, context)?;
            current_line.extend(cells);
            current_width += word_width;
        }
    }
    
    // Add final line
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    
    Ok(lines)
}

/// Helper to calculate text width from shaped glyphs
fn calculate_text_width(
    &self,
    infos: &[GlyphInfo],
    font: &Rc<LoadedFont>,
    context: &LayoutContext,
) -> anyhow::Result<f32> {
    let mut width = 0.0;
    let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();
    
    for info in infos {
        let glyph = glyph_cache.cached_glyph(
            info,
            None, // style
            false, // followed_by_space
            font,
            context.metrics,
            1, // num_cells
        )?;
        width += glyph.x_advance.get() as f32;
    }
    
    Ok(width)
}

/// Helper to get width of a single ElementCell
fn get_cell_width(
    &self,
    cell: &ElementCell,
    font: &Rc<LoadedFont>,
    context: &LayoutContext,
) -> anyhow::Result<f32> {
    match cell {
        ElementCell::Sprite(_) => Ok(context.width.pixel_cell),
        ElementCell::Glyph(glyph) => Ok(glyph.x_advance.get() as f32),
    }
}
```

### 3. Update compute_element Method

**File: `wezterm-gui/src/termwindow/box_model.rs`**

In the `compute_element` method's match statement (around line 669), add the new variant:

```rust
ElementContent::WrappedText(text) => {
    let lines = self.wrap_text(text, &element.font, max_width, context)?;
    let line_height = context.height.pixel_cell;
    let num_lines = lines.len() as f32;
    
    let content_rect = euclid::rect(
        0.,
        0.,
        max_width.max(min_width),
        (line_height * num_lines).max(min_height),
    );
    
    let rects = element.compute_rects(context, content_rect);
    
    Ok(ComputedElement {
        item_type: element.item_type.clone(),
        zindex: element.zindex + context.zindex,
        baseline,
        border,
        border_corners,
        colors: element.colors.clone(),
        hover_colors: element.hover_colors.clone(),
        bounds: rects.bounds,
        border_rect: rects.border_rect,
        padding: rects.padding,
        content_rect: rects.content_rect,
        content: ComputedElementContent::MultilineText {
            lines,
            line_height,
        },
    })
}
```

### 4. Update Rendering Logic

**File: `wezterm-gui/src/termwindow/box_model.rs`**

In the `paint_element` method (around line 850), add handling for MultilineText:

```rust
ComputedElementContent::MultilineText { lines, line_height } => {
    let mut y_offset = element.content_rect.min_y();
    
    for (line_idx, line_cells) in lines.iter().enumerate() {
        let x = element.content_rect.min_x();
        let y = y_offset + (line_idx as f32 * line_height);
        
        // Render each line similar to single-line text
        for cell in line_cells {
            match cell {
                ElementCell::Sprite(sprite) => {
                    // Existing sprite rendering code with adjusted y
                }
                ElementCell::Glyph(glyph) => {
                    // Existing glyph rendering code with adjusted y
                }
            }
        }
    }
}
```

### 5. Update Height Estimation

**File: `wezterm-gui/src/sidebar/components/scrollable.rs`**

Update `estimate_element_height_recursive` to handle wrapped text (around line 259):

```rust
ElementContent::WrappedText(text) => {
    // Calculate accurate wrapped height using available width
    // The context has pixel_max which represents the available width
    let available_width = context.pixel_max - 
        element.padding.left.evaluate_as_pixels(context) - 
        element.padding.right.evaluate_as_pixels(context) -
        element.margin.left.evaluate_as_pixels(context) -
        element.margin.right.evaluate_as_pixels(context) -
        element.border.left.evaluate_as_pixels(context) -
        element.border.right.evaluate_as_pixels(context);
    
    // Use font metrics to calculate wrapped lines
    // This is a simplified calculation - in the actual implementation,
    // we'd call the same text measurement logic used in wrap_text
    let avg_char_width = context.pixel_cell * 0.6; // Approximate average character width
    let chars_per_line = (available_width / avg_char_width).floor().max(1.0);
    
    // Count words and estimate wrapping
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines = 1.0;
    let mut current_line_chars = 0.0;
    
    for word in words {
        let word_chars = word.len() as f32 + 1.0; // +1 for space
        if current_line_chars + word_chars > chars_per_line && current_line_chars > 0.0 {
            lines += 1.0;
            current_line_chars = word_chars;
        } else {
            current_line_chars += word_chars;
        }
    }
    
    lines * actual_line_height
}
```

### 6. Update Component Usage

**File: `wezterm-gui/src/sidebar/ai_sidebar.rs`**

Update text elements to use wrapped text:

```rust
// For suggestions (line ~436)
Element::new(
    &self.fonts.suggestion_body,
    ElementContent::WrappedText(content.clone())
)

// For chat messages (line ~540)
Element::new(
    &font,
    ElementContent::WrappedText(message.content.clone())
)

// For goals (line ~370)
Element::new(
    &self.fonts.normal,
    ElementContent::WrappedText(goal.clone())
)
```

**File: `wezterm-gui/src/sidebar/components/markdown.rs`**

Update paragraph rendering (line ~90):

```rust
MarkdownElement::Paragraph(children) => {
    let text = extract_text_from_children(&children);
    vec![Element::new(
        &self.fonts.normal,
        ElementContent::WrappedText(text)
    )]
}
```

### 7. Testing Plan

1. **Unit Tests**: Add tests for wrap_text function with various edge cases:
   - Empty text
   - Single word longer than max_width
   - Text with multiple spaces
   - Unicode text (emojis, non-ASCII)

2. **Visual Tests**: 
   - Long suggestion text wraps properly
   - Chat messages with long lines wrap
   - Markdown paragraphs wrap at word boundaries
   - Window resizing causes text to reflow

3. **Performance Tests**:
   - Measure frame rate with many wrapped text elements
   - Check memory usage with large amounts of text
   - Verify no impact on non-wrapped text rendering

## Implementation Order

1. Add data structures (WrapMode, WrappedText, MultilineText)
2. Implement wrap_text method with basic word wrapping
3. Update compute_element to handle WrappedText
4. Update paint_element to render MultilineText
5. Update one component (e.g., suggestions) as proof of concept
6. Test thoroughly
7. Update remaining components
8. Update height estimation for accurate scrolling

## Edge Cases to Handle

1. **Very long words**: Handled automatically - words that exceed max_width are broken at character boundaries

2. **Empty lines**: Multiple spaces or newlines should preserve vertical spacing

3. **Mixed content**: Elements with both text and child elements need careful handling

4. **RTL text**: Ensure bidirectional text wraps correctly

5. **Terminal output**: Long strings without spaces (URLs, file paths, error codes) wrap at character boundaries

## Success Criteria

- Text in AI sidebar wraps at word boundaries
- No truncation of content within available width
- Performance impact < 1ms per frame
- Existing non-wrapped text behavior unchanged
- Scrollbar accurately reflects content height

## Future Enhancements

1. **Hyphenation**: Smart breaking of long words with hyphens
2. **Justification**: Even spacing between words
3. **Rich text**: Support for inline formatting (bold, italic) once font variants are supported
4. **Configurable wrapping**: Allow users to disable wrapping or choose wrap behavior per element