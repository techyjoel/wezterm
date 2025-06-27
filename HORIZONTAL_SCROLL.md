# Horizontal Scrolling for Code Blocks Implementation Plan

## Overview
Implement horizontal scrolling for code blocks in the markdown renderer to handle long lines without wrapping, preserving code formatting and readability.

## Architecture

### 1. Code Block Container Component
Create a new `CodeBlockContainer` struct that manages:
- Viewport width (from parent constraints)
- Content width (longest line in the code block)
- Horizontal scroll offset
- Mouse interaction state
- Scrollbar visibility and hover state
- Focus state for keyboard navigation

```rust
// In markdown.rs or new file code_block.rs
pub struct CodeBlockContainer {
    id: String,
    content_width: f32,
    viewport_width: f32,
    scroll_offset: f32,
    hovering_scrollbar: bool,
    hovering_content: bool,
    dragging_scrollbar: bool,
    drag_start_x: Option<f32>,
    drag_start_offset: Option<f32>,
    has_focus: bool,
    scrollbar_opacity: f32,
    last_activity: Option<Instant>,
}
```

### 2. Rendering Strategy
Use **relative z-index layering** within the current rendering context:

```
Layer Stack (relative to parent z-index):
- Sub-layer 0: Code block background and border
- Sub-layer 1: Scrollable code content (clipped to viewport)
- Sub-layer 2: Horizontal scrollbar (when needed)
- Z-index +1: Copy button (floats above code block)
```

### 3. Content Measurement
Before rendering, measure all code lines to find the maximum width:

```rust
fn measure_code_block_width(
    lines: &[String], 
    font: &Rc<LoadedFont>
) -> f32 {
    lines.iter()
        .map(|line| {
            // Use font metrics to calculate pixel width
            let cells = unicode_column_width(line, None);
            cells as f32 * font.metrics().cell_width.get()
        })
        .max()
        .unwrap_or(0.0)
}
```

### 4. Scrollbar Implementation

#### Shared Hover/Activity Behavior
Create a shared trait for auto-hiding scrollbars:

```rust
trait AutoHideScrollbar {
    fn update_visibility(&mut self, delta_time: f32) {
        const FADE_IN_TIME: f32 = 0.15;
        const FADE_OUT_TIME: f32 = 0.3;
        const HIDE_DELAY: f32 = 1.5;
        
        if self.is_active() {
            // Fade in
            self.set_opacity(
                (self.opacity() + delta_time / FADE_IN_TIME).min(1.0)
            );
            self.set_last_activity(Some(Instant::now()));
        } else if let Some(last) = self.last_activity() {
            let elapsed = last.elapsed().as_secs_f32();
            if elapsed > HIDE_DELAY {
                // Fade out
                self.set_opacity(
                    (self.opacity() - delta_time / FADE_OUT_TIME).max(0.0)
                );
            }
        }
    }
    
    fn is_active(&self) -> bool;
    fn opacity(&self) -> f32;
    fn set_opacity(&mut self, opacity: f32);
    fn last_activity(&self) -> Option<Instant>;
    fn set_last_activity(&mut self, time: Option<Instant>);
}
```

#### Horizontal Scrollbar Rendering
Leverage the existing `ScrollbarRenderer` in horizontal mode:

```rust
// In highlight_code_block method
let scrollbar = ScrollbarRenderer::new_horizontal(
    code_container.viewport_width,
    6.0, // scrollbar height (thinner than vertical)
);
scrollbar.update(
    code_container.content_width,
    code_container.viewport_width,
    code_container.scroll_offset,
);
```

### 5. Copy Button Implementation

Add a copy button that appears just above the code block:

```rust
// Render copy button positioned above the code block at top-right
if code_container.hovering_content {
    let copy_button = Element::new(font, ElementContent::Text("📋".to_string()))
        .colors(ElementColors {
            bg: LinearRgba(0.2, 0.2, 0.2, 0.8).into(),
            text: LinearRgba(0.9, 0.9, 0.9, 1.0).into(),
            ..Default::default()
        })
        .padding(BoxDimension::new(Dimension::Pixels(4.0)))
        .border(BoxDimension::new(Dimension::Pixels(1.0)))
        .colors(ElementColors {
            border: BorderColor::new(LinearRgba(0.4, 0.4, 0.4, 0.5)),
            ..Default::default()
        })
        .with_item_type(UIItemType::CodeBlockCopyButton(id.clone()))
        .margin(BoxDimension {
            bottom: Dimension::Pixels(4.0), // Space between button and code block
            ..Default::default()
        })
        .display(DisplayType::Block); // Render as separate block above code
}
```

### 6. Rendering Flow

1. **Measure Phase**:
   - Calculate max line width from syntax-highlighted segments
   - Determine if scrollbar is needed (content_width > viewport_width)
   - Reserve 8px height for scrollbar if needed

2. **Layout Phase**:
   - Create container element with fixed width (viewport)
   - Apply negative left margin based on scroll_offset
   - Render each line as non-wrapping text (ElementContent::Text)

3. **Scrollbar Phase**:
   - Position below code content
   - Apply opacity based on hover/activity state
   - Render using sidebar styling
   - Register UIItems for mouse interaction

4. **Copy Button Phase**:
   - Position at top-right, just above the code block, with slight margin
   - Handle click to copy code to clipboard

### 7. Mouse Event Handling

```rust
// In sidebar mouse event handler
if let Some(code_block) = self.find_code_block_at_position(mouse_pos) {
    // Update hover state
    code_block.hovering_content = true;
    
    // Handle horizontal wheel events
    if let MouseEventKind::HorzWheel(delta) = event.kind {
        code_block.scroll_horizontal(delta * 30.0);
        return true;
    }
    
    // Handle click for focus
    if let MouseEventKind::Press(MousePress::Left) = event.kind {
        code_block.has_focus = true;
        // Clear focus from other code blocks
    }
}
```

### 8. Keyboard Support

When a code block has focus:
- **Left/Right arrows**: Scroll horizontally by ~50 pixels
- **Home/End**: Jump to start/end of longest line
- **Shift+Wheel**: Convert vertical wheel to horizontal scroll
- **Escape**: Clear focus

```rust
// In keyboard event handler
if let Some(focused_block) = self.get_focused_code_block() {
    match key {
        KeyCode::LeftArrow => {
            focused_block.scroll_horizontal(-50.0);
            return true;
        }
        KeyCode::RightArrow => {
            focused_block.scroll_horizontal(50.0);
            return true;
        }
        KeyCode::Home => {
            focused_block.scroll_offset = 0.0;
            return true;
        }
        KeyCode::End => {
            focused_block.scroll_offset = focused_block.max_scroll();
            return true;
        }
        KeyCode::Escape => {
            focused_block.has_focus = false;
            return true;
        }
        _ => {}
    }
}
```

### 9. Integration Points

1. **Markdown Renderer Changes**:
   - Replace direct code block rendering with CodeBlockContainer
   - Pass viewport width from parent's max_width constraint
   - Generate unique IDs for each code block
   - Track containers in a registry

2. **Sidebar State**:
   - Add `code_block_registry: HashMap<String, CodeBlockContainer>`
   - Clean up on content change
   - Update visibility states in render loop

3. **UIItem Registration**:
   - Add new UIItemType variants:
     ```rust
     UIItemType::CodeBlockScrollbar(String),  // ID
     UIItemType::CodeBlockContent(String),    // ID  
     UIItemType::CodeBlockCopyButton(String), // ID
     ```

### 10. Implementation Phases

**Phase 1: Basic Structure**
- Create CodeBlockContainer struct
- Implement width measurement
- Basic rendering without scrolling
- Generate unique IDs for code blocks

**Phase 2: Scrollbar Rendering**
- Integrate with existing scrollbar style
- Implement auto-hide behavior
- Share visibility logic with sidebar scrollbar
- Position below code content

**Phase 3: Mouse Interaction**
- Horizontal wheel scrolling
- Scrollbar dragging
- Click to focus
- Hover state tracking

**Phase 4: Copy Button**
- Add copy button on hover
- Implement clipboard integration
- Visual feedback on copy

**Phase 5: Keyboard Support**
- Focus management
- Arrow key scrolling
- Home/End navigation
- Shift+wheel for horizontal scroll

**Phase 6: Polish**
- Smooth scrolling animation
- Focus indicators
- Ensure proper cleanup
- Performance optimization

### 11. Visual Design

- **Scrollbar**: Match sidebar style (thin, semi-transparent)
- **Auto-hide**: Fade in on hover/activity, fade out after 1.5s
- **Copy button**: Subtle, appears on hover
- **Focus indicator**: Subtle border highlight when focused
- **Overflow indicator**: Gradient fade on right edge when scrollable

### 12. Technical Considerations

1. **Performance**: Cache measured widths to avoid recalculation
2. **Memory**: Clean up containers when content changes
3. **Coordination**: Ensure only one code block has focus at a time
4. **Accessibility**: Ensure keyboard navigation is discoverable
5. **Edge cases**: Handle empty code blocks, very long lines

### 13. Success Criteria

1. Long code lines don't overflow the sidebar
2. Horizontal scrolling is smooth and responsive
3. Scrollbar appears only when needed and on activity
4. Copy button works reliably
5. Keyboard navigation is intuitive
6. No performance impact on markdown rendering
7. Visual style matches existing sidebar components

## Implementation Status

### Phase 1: Basic Structure ✅ COMPLETED
- ✅ Created `CodeBlockContainer` struct with all required fields
- ✅ Implemented width measurement using `unicode_column_width` (from termwiz)
- ✅ Added `CodeBlockRegistry` type alias (Arc<Mutex<HashMap<String, CodeBlockContainer>>>)
- ✅ Updated `MarkdownRenderer` to:
  - Track code block counter for unique IDs
  - Accept optional `max_width` parameter
  - Generate unique IDs for each code block (format: "code_block_{counter}")
  - Pass viewport width to code block rendering
- ✅ Added new public method `render_with_width()` for width-aware rendering
- ✅ Collected lines for measurement in `highlight_code_block`

### Phase 2: Scrollbar Rendering 🔄 IN PROGRESS
- ✅ Added UIItemType variants:
  - `CodeBlockScrollbar(String)`
  - `CodeBlockContent(String)`
  - `CodeBlockCopyButton(String)`
- ✅ Updated mouse event handling in `mouseevent.rs`:
  - Added match arms for new UIItemType variants
  - Created stub methods for each interaction type
  - Set appropriate cursors (Arrow for scrollbar/button, Text for content)
- ✅ Verified `ScrollbarRenderer` supports horizontal mode via `new_horizontal()`
- 🔲 TODO: Actually render the scrollbar in code blocks
- 🔲 TODO: Implement shared auto-hide behavior
- 🔲 TODO: Position scrollbar below code content

### Phase 3: Mouse Interaction 🔲 NOT STARTED
- 🔲 Horizontal wheel scrolling
- 🔲 Scrollbar dragging
- 🔲 Click to focus
- 🔲 Hover state tracking

### Phase 4: Copy Button 🔲 NOT STARTED
- 🔲 Render copy button above code block
- 🔲 Implement clipboard integration
- 🔲 Visual feedback on copy

### Phase 5: Keyboard Support 🔲 NOT STARTED
- 🔲 Focus management
- 🔲 Arrow key scrolling
- 🔲 Home/End navigation
- 🔲 Shift+wheel for horizontal scroll

### Phase 6: Polish 🔲 NOT STARTED
- 🔲 Smooth scrolling animation
- 🔲 Focus indicators
- 🔲 Cleanup and optimization

## Implementation Differences/Notes

1. **Width Measurement**: Using `unicode_column_width` from termwiz instead of a custom implementation, which is more accurate for terminal rendering.

2. **Renderer Structure**: Made `MarkdownRenderer` methods require `&mut self` to support the code block counter. This allows generating unique IDs without external state.

3. **UIItemType Integration**: Following the existing pattern where UIItemType variants store the ID string directly, not wrapped in a struct.

4. **ScrollbarRenderer**: The existing `ScrollbarRenderer` already supports horizontal mode perfectly, so we can reuse it directly rather than creating a custom implementation.

5. **Mouse Event Stubs**: Added placeholder implementations that log actions and set appropriate cursors. These will be fleshed out in Phase 3.

## Next Steps

1. **Immediate**: Implement actual scrollbar rendering in `highlight_code_block` method
2. **Then**: Create a registry in the sidebar to track CodeBlockContainers
3. **Then**: Wire up mouse interactions to update scroll state
4. **Finally**: Add copy button and keyboard support