# Text Wrapping Implementation

## Status

### Completed ✅
- Added `WrappedText(String)` variant to `ElementContent` enum
- Added `MultilineText` variant to `ComputedElementContent` with line storage
- Implemented core `wrap_text` function with word and character wrapping
- Updated `compute_element` to handle WrappedText
- Updated `render_element` to render MultilineText
- Updated height estimation in ScrollableContainer
- Converted AI sidebar components to use WrappedText:
  - Goal text
  - Chat messages (both user and AI)
  - Suggestion content (via MarkdownRenderer)
- Updated MarkdownRenderer to use WrappedText for:
  - Paragraphs
  - Headings
  - Code blocks (including syntax highlighted lines)
- Added support for all match arms in box_model and fancy_tab_bar

### To-Do 📝
- Unit tests for text wrapping logic
- Performance optimization (caching wrapped text)
- Configurable wrapping behavior
- Support for preserving newlines in wrapped text
- RTL text wrapping support
- Hyphenation support for long words

## Overview

This document describes the text wrapping implementation for WezTerm's Element rendering system. Text wrapping was added to fix truncation issues in the AI sidebar where long text in suggestions, goals, chat messages, and markdown content would be cut off instead of wrapping to multiple lines.

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
    style: &config::TextStyle,  // Added parameter for text style
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
        let word_width = self.calculate_text_width(word, &word_infos, font, context, style)?;
        
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
            let cells = self.shape_text_to_cells(word, &word_infos, font, context, style)?;
            let mut char_line = Vec::new();
            let mut char_width = 0.0;
            
            for cell in cells {
                let cell_width = self.get_cell_width(&cell, context)?;
                
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
            let cells = self.shape_text_to_cells(word, &word_infos, font, context, style)?;
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
    text: &str,
    infos: &[GlyphInfo],
    font: &Rc<LoadedFont>,
    context: &LayoutContext,
    style: &config::TextStyle,
) -> anyhow::Result<f32> {
    let mut width = 0.0;
    let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();
    
    for info in infos {
        // Check if it's a unicode block glyph
        let cell_start = &text[info.cluster as usize..];
        let mut iter = Graphemes::new(cell_start).peekable();
        if let Some(grapheme) = iter.next() {
            if let Some(_key) = BlockKey::from_str(grapheme) {
                width += context.width.pixel_cell;
                continue;
            }
            
            let followed_by_space = iter.peek() == Some(&" ");
            let num_cells = grapheme_column_width(grapheme, None);
            let glyph = glyph_cache.cached_glyph(
                info,
                style,
                followed_by_space,
                font,
                context.metrics,
                num_cells as u8,
            )?;
            width += glyph.x_advance.get() as f32;
        }
    }
    
    Ok(width)
}

/// Helper to get width of a single ElementCell
fn get_cell_width(
    &self,
    cell: &ElementCell,
    context: &LayoutContext,
) -> anyhow::Result<f32> {
    match cell {
        ElementCell::Sprite(_) => Ok(context.width.pixel_cell),
        ElementCell::Glyph(glyph) => Ok(glyph.x_advance.get() as f32),
    }
}

/// Helper to convert shaped text to ElementCells
fn shape_text_to_cells(
    &self,
    text: &str,
    infos: &[GlyphInfo],
    font: &Rc<LoadedFont>,
    context: &LayoutContext,
    style: &config::TextStyle,
) -> anyhow::Result<Vec<ElementCell>> {
    let mut cells = Vec::new();
    let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();

    for info in infos {
        // Check if it's a unicode block glyph
        let cell_start = &text[info.cluster as usize..];
        let mut iter = Graphemes::new(cell_start).peekable();
        if let Some(grapheme) = iter.next() {
            if let Some(key) = BlockKey::from_str(grapheme) {
                let sprite = glyph_cache.cached_block(key, context.metrics)?;
                cells.push(ElementCell::Sprite(sprite));
                continue;
            }

            let followed_by_space = iter.peek() == Some(&" ");
            let num_cells = grapheme_column_width(grapheme, None);
            let glyph = glyph_cache.cached_glyph(
                info,
                style,
                followed_by_space,
                font,
                context.metrics,
                num_cells as u8,
            )?;
            cells.push(ElementCell::Glyph(glyph));
        }
    }

    Ok(cells)
}
```

### 3. Update compute_element Method

**File: `wezterm-gui/src/termwindow/box_model.rs`**

In the `compute_element` method's match statement (around line 669), add the new variant:

```rust
ElementContent::WrappedText(text) => {
    let lines = self.wrap_text(text, &element.font, max_width, context, &style)?;
    let line_height = context.height.pixel_cell;
    let num_lines = lines.len() as f32;
    
    // Calculate max width of all lines for proper content rect
    let mut max_line_width: f32 = 0.0;
    for line in &lines {
        let mut line_width = 0.0;
        for cell in line {
            line_width += self.get_cell_width(cell, context)?;
        }
        max_line_width = max_line_width.max(line_width);
    }
    
    let content_rect = euclid::rect(
        0.,
        0.,
        max_line_width.max(min_width),
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

In the `render_element` method (around line 1116), add handling for MultilineText:

```rust
ComputedElementContent::MultilineText { lines, line_height } => {
    let mut y_offset = 0.0;
    
    for (line_idx, line_cells) in lines.iter().enumerate() {
        let mut pos_x = element.content_rect.min_x();
        let y = element.content_rect.min_y() + (line_idx as f32 * line_height);
        
        for cell in line_cells {
            if pos_x >= element.content_rect.max_x() {
                break;
            }
            match cell {
                ElementCell::Sprite(sprite) => {
                    let width = sprite.coords.width();
                    let height = sprite.coords.height();
                    let pos_y = top + y;

                    if pos_x + width as f32 > element.content_rect.max_x() {
                        break;
                    }

                    let mut quad = layers.allocate(2)?;
                    quad.set_position(
                        pos_x + left,
                        pos_y,
                        pos_x + left + width as f32,
                        pos_y + height as f32,
                    );
                    self.resolve_text(colors, inherited_colors).apply(&mut quad);
                    quad.set_texture(sprite.texture_coords());
                    quad.set_hsv(None);
                    pos_x += width as f32;
                }
                ElementCell::Glyph(glyph) => {
                    if let Some(texture) = glyph.texture.as_ref() {
                        let pos_y = y as f32 + top
                            - (glyph.y_offset + glyph.bearing_y).get() as f32
                            + element.baseline;

                        if pos_x + glyph.x_advance.get() as f32
                            > element.content_rect.max_x()
                        {
                            break;
                        }
                        let pos_x = pos_x + (glyph.x_offset + glyph.bearing_x).get() as f32;
                        let width = texture.coords.size.width as f32 * glyph.scale as f32;
                        let height = texture.coords.size.height as f32 * glyph.scale as f32;

                        let mut quad = layers.allocate(1)?;
                        quad.set_position(
                            pos_x + left,
                            pos_y,
                            pos_x + left + width,
                            pos_y + height,
                        );
                        self.resolve_text(colors, inherited_colors).apply(&mut quad);
                        quad.set_texture(texture.texture_coords());
                        quad.set_has_color(glyph.has_color);
                        quad.set_hsv(None);
                    }
                    pos_x += glyph.x_advance.get() as f32;
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

## Implementation Differences from Plan

### Key Changes Made:
1. **Simplified API**: Instead of using `WrapMode` enum, we went with a single `WrappedText(String)` variant that automatically handles both word and character wrapping
2. **Added text parameter**: Helper functions (`calculate_text_width`, `shape_text_to_cells`) need the original text string to extract grapheme clusters correctly
3. **Added style parameter**: All text processing functions now take `&config::TextStyle` for proper glyph caching
4. **Content rect calculation**: Added logic to calculate the maximum line width for proper content rect sizing (not just using max_width)
5. **Complete rendering implementation**: The MultilineText rendering includes full sprite and glyph positioning logic

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