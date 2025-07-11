# Per-Layer Scissor Rect Implementation Plan for Scrollable Content

## Executive Summary

This document provides a detailed implementation plan for adding hardware-accelerated scissor rect clipping to WezTerm's rendering pipeline. This approach enables proper scrollable content with clipping while working within WezTerm's existing architecture.

### Key Design Decision
We implement **per-layer scissor rects** that apply clipping at the GPU level for entire z-index layers. This avoids the architectural issues of per-element clipping while providing hardware-accelerated performance.

### Goals
- Implement smooth, pixel-perfect scrolling with proper clipping
- Zero performance overhead using GPU scissor functionality
- Maintain all existing functionality including text selection
- Work within WezTerm's batched rendering architecture
- No frame delays or complex state management

### Why Per-Layer Scissor?
Previous attempts at scissor rects failed because they tried to apply scissor per-element. By applying scissor per render layer (z-index), we:
- Align with WezTerm's batched rendering model
- Avoid timing issues between element processing and GPU drawing
- Leverage hardware acceleration with no performance cost
- Maintain architectural simplicity

## Architecture Overview

### Current Rendering Pipeline

```
Phase 1: Element Processing
├── Process element tree recursively
├── Elements allocate quads to layers by z-index
├── Each unique z-index creates a RenderLayer
└── Vertex data batched into triple buffers

Phase 2: GPU Drawing
├── Iterate through layers by z-index
├── Each layer drawn with single GPU call
├── WebGPU: Separate render pass per layer
└── OpenGL: Separate draw call per layer
```

### Enhanced Pipeline with Scissor

```
Phase 1: Element Processing
├── Process element tree recursively
├── Elements mark scissor requirements → NEW
├── Layers collect scissor bounds → NEW
└── Vertex data batched as before

Phase 2: GPU Drawing  
├── Iterate through layers by z-index
├── Apply layer scissor rect if present → NEW
├── Draw layer with hardware clipping
└── Clear scissor for next layer
```

## Implementation Plan

### Phase 1: Core Infrastructure (Week 1)

#### 1.1 Extend RenderLayer Structure

Modify `wezterm-gui/src/termwindow/renderstate.rs`:

```rust
pub struct RenderLayer {
    pub vb: RefCell<[TripleVertexBuffer; 3]>,
    context: RenderContext,
    zindex: i8,
    
    /// NEW: Optional scissor rect for this layer
    /// Accumulated from all elements that contribute scissor bounds
    scissor_rect: RefCell<Option<euclid::default::Rect<f32>>>,
}

impl RenderLayer {
    pub fn new(context: &RenderContext, num_quads: usize, zindex: i8) -> anyhow::Result<Self> {
        // ... existing implementation ...
        Ok(Self {
            vb: RefCell::new(vbs),
            context: context.clone(),
            zindex,
            scissor_rect: RefCell::new(None), // NEW
        })
    }
    
    /// NEW: Update this layer's scissor rect
    pub fn update_scissor_rect(&self, rect: euclid::default::Rect<f32>) {
        let mut scissor = self.scissor_rect.borrow_mut();
        *scissor = match *scissor {
            Some(existing) => Some(existing.union(&rect)),
            None => Some(rect),
        };
    }
    
    /// NEW: Get scissor rect for drawing (doesn't remove it)
    pub fn get_scissor_rect(&self) -> Option<euclid::default::Rect<f32>> {
        *self.scissor_rect.borrow()
    }
}
```

#### 1.2 Extend Element System

Modify `wezterm-gui/src/termwindow/box_model.rs`:

```rust
pub struct Element {
    // ... existing fields ...
    
    /// NEW: Whether this element contributes scissor bounds to its layer
    pub layer_scissor: Option<LayerScissor>,
}

#[derive(Clone, Debug)]
pub struct LayerScissor {
    /// The clipping rectangle in screen coordinates
    pub rect: euclid::default::Rect<f32>,
    
    /// Optional scroll offset to apply
    pub scroll_offset: Option<euclid::default::Point2D<f32>>,
}

impl Element {
    /// NEW: Mark element to contribute scissor bounds
    pub fn with_layer_scissor(mut self, viewport: euclid::default::Rect<f32>) -> Self {
        self.layer_scissor = Some(LayerScissor {
            rect: viewport,
            scroll_offset: None,
        });
        self
    }
    
    /// NEW: Update scroll offset for scissor
    pub fn with_scroll_offset(mut self, offset: euclid::default::Point2D<f32>) -> Self {
        if let Some(scissor) = &mut self.layer_scissor {
            scissor.scroll_offset = Some(offset);
        }
        self
    }
}
```

#### 1.3 Update Element Processing

Modify `render_element()` in `box_model.rs`:

```rust
fn render_element(
    element: &ComputedElement,
    gl_state: &mut dyn RenderState,
) -> anyhow::Result<()> {
    // Get the layer for this element's z-index
    let layer = gl_state.layer_for_zindex(element.zindex)?;
    
    // NEW: If element contributes scissor, update layer
    if let Some(layer_scissor) = &element.layer_scissor {
        layer.update_scissor_rect(layer_scissor.rect);
    }
    
    // ... rest of existing render_element logic ...
}
```

#### 1.4 Frame State Management

Add scissor rect clearing to ensure clean state each frame:

```rust
impl RenderState {
    /// Clear all layer scissor rects at start of frame
    pub fn clear_frame_state(&mut self) -> anyhow::Result<()> {
        for layer in self.layers.borrow().iter() {
            layer.scissor_rect.borrow_mut().take();
        }
        Ok(())
    }
}

// Call at start of paint_impl in draw.rs
context.clear_frame_state()?;

### Phase 2: GPU Drawing Integration (Week 1)

#### 2.1 WebGPU Scissor Implementation

Modify `wezterm-gui/src/termwindow/render/draw.rs`:

```rust
fn call_draw_webgpu(
    frame: &mut WebGpuFrame,
    context: &mut RenderContext,
    layers: &[Rc<RenderLayer>],
) -> anyhow::Result<()> {
    // ... existing setup ...
    
    for layer in layers {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            // ... existing descriptor ...
        });
        
        // NEW: Apply scissor rect if layer has one
        if let Some(scissor_rect) = layer.get_scissor_rect() {
            // Convert to viewport coordinates
            let (width, height) = frame.dimensions;
            
            // WebGPU uses top-left origin, same as our element coords
            render_pass.set_scissor_rect(
                scissor_rect.origin.x as u32,
                scissor_rect.origin.y as u32,
                scissor_rect.size.width.min(width as f32) as u32,
                scissor_rect.size.height.min(height as f32) as u32,
            );
        }
        
        // ... existing drawing code ...
        
        drop(render_pass); // Automatically clears scissor
    }
    
    // ... rest of implementation ...
}
```

#### 2.2 OpenGL Scissor Implementation

Modify OpenGL drawing in `draw.rs`:

```rust
fn call_draw_glium(
    frame: &mut glium::Frame,
    context: &mut RenderContext,
    layers: &[Rc<RenderLayer>],
) -> anyhow::Result<()> {
    // ... existing setup ...
    
    for layer in layers {
        // NEW: Prepare draw parameters with optional scissor
        let scissor_test = if let Some(scissor_rect) = layer.get_scissor_rect() {
            let (width, height) = frame.get_dimensions();
            
            // OpenGL uses bottom-left origin, need to flip Y
            Some(glium::Rect {
                left: scissor_rect.origin.x as u32,
                bottom: (height as f32 - scissor_rect.origin.y - scissor_rect.size.height) as u32,
                width: scissor_rect.size.width.min(width as f32) as u32,
                height: scissor_rect.size.height.min(height as f32) as u32,
            })
        } else {
            None
        };
        
        let draw_params = glium::DrawParameters {
            blend: glium::Blend::alpha_blending(),
            scissor: scissor_test, // NEW
            ..Default::default()
        };
        
        // Draw with scissor parameters
        frame.draw(
            // ... existing draw call with new params ...
        )?;
    }
    
    // ... rest of implementation ...
}
```

### Phase 3: Scrollable Component Integration (Week 2)

#### 3.1 Z-Index Allocation Strategy

Reserve specific z-indices for scrollable content:

```rust
// In appropriate constants file
pub mod ZIndex {
    // ... existing z-indices ...
    
    /// Scrollable content layers (with scissor)
    pub const ACTIVITY_LOG_CONTENT: i8 = 10;  // Already used
    pub const CHAT_INPUT_CONTENT: i8 = 13;    // NEW - between 12 and 14
    pub const MODAL_SCROLLABLE: i8 = 21;      // For modal content
    pub const LEFT_SIDEBAR_SCROLLABLE: i8 = 31; // For left sidebar
}
```

#### 3.2 MultilineTextInput with Scissor

Modify `wezterm-gui/src/sidebar/components/forms.rs`:

```rust
impl MultilineTextInput {
    /// Render with scissor-based clipping
    pub fn render_with_scissor(
        &self,
        fonts: &SidebarFonts,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Element {
        let line_height = fonts.body.metrics().cell_height.get() as f32;
        
        // Calculate viewport bounds in screen coordinates
        let viewport = euclid::rect(
            0.0,
            0.0,
            viewport_width,
            viewport_height,
        );
        
        // Create container that will be clipped
        let mut container = Element::new(&fonts.body, ElementContent::Vertical)
            .zindex(ZIndex::CHAT_INPUT_CONTENT) // Dedicated z-index
            .with_layer_scissor(viewport);      // Enable clipping
        
        // Calculate scroll offset based on cursor position
        let scroll_offset = self.calculate_scroll_offset(line_height);
        
        // Virtual rendering - only create elements for visible lines
        let first_visible_line = (scroll_offset / line_height).floor() as usize;
        let last_visible_line = ((scroll_offset + viewport_height) / line_height).ceil() as usize;
        
        // Create a positioned container for proper layout
        let mut content = Element::new(&fonts.body, ElementContent::Children(vec![]));
        
        for line_idx in first_visible_line..=last_visible_line.min(self.lines.len().saturating_sub(1)) {
            let y_position = line_idx as f32 * line_height - scroll_offset;
            
            // Create line element at absolute position
            let line_element = self.render_line(&self.lines[line_idx], line_idx, fonts)
                .position(Position::Absolute)
                .top(Dimension::Pixels(y_position))
                .left(Dimension::Pixels(0.0));
                
            content = content.add_child(line_element);
        }
        
        // Render cursor if visible
        if self.focused {
            let cursor_y = self.cursor_line as f32 * line_height - scroll_offset;
            if cursor_y >= -line_height && cursor_y < viewport_height {
                let cursor = self.render_cursor(fonts)
                    .position(Position::Absolute)
                    .top(Dimension::Pixels(cursor_y))
                    .left(Dimension::Pixels(cursor_x));
                content = content.add_child(cursor);
            }
        }
        
        container.add_child(content)
    }
    
    fn calculate_scroll_offset(&self, line_height: f32) -> f32 {
        // Ensure cursor line is visible
        let cursor_top = self.cursor_line as f32 * line_height;
        let cursor_bottom = cursor_top + line_height;
        
        let viewport_height = self.display_lines as f32 * line_height;
        
        if cursor_bottom > self.scroll_offset + viewport_height {
            // Scroll down to show cursor
            cursor_bottom - viewport_height
        } else if cursor_top < self.scroll_offset {
            // Scroll up to show cursor
            cursor_top
        } else {
            self.scroll_offset
        }
    }
}
```

#### 3.3 Update AI Sidebar Integration

Modify `wezterm-gui/src/sidebar/ai_sidebar.rs`:

```rust
impl AiSidebar {
    fn render_chat_input(&self, fonts: &SidebarFonts) -> Element {
        let input_height = 40.0; // 2 lines
        let input_width = self.width as f32 - 60.0; // Leave room for send button
        
        // Create the scrollable input
        let input = self.chat_input.render_with_scissor(
            fonts,
            input_width,
            input_height,
        );
        
        // Container at standard z-index
        Element::new(&fonts.body, ElementContent::Horizontal)
            .add_child(input)
            .add_child(self.render_send_button(fonts))
            .zindex(ZIndex::RIGHT_SIDEBAR_MAIN)
    }
}
```

### Phase 4: Activity Log Migration (Week 2-3)

#### 4.1 Convert Activity Log to Scissor

The activity log uses virtual scrolling with proper element positioning:

```rust
impl AiSidebar {
    fn render_activity_log(&self, fonts: &SidebarFonts) -> Element {
        let viewport_height = self.activity_log_height;
        let viewport = euclid::rect(
            0.0,
            0.0,
            self.width as f32,
            viewport_height,
        );
        
        // Container with scissor clipping at z-index 10
        let mut container = Element::new(&fonts.body, ElementContent::Vertical)
            .zindex(ZIndex::ACTIVITY_LOG_CONTENT)
            .with_layer_scissor(viewport);
        
        // Virtual scrolling - calculate visible range
        let mut y_pos = 0.0;
        let scroll_top = self.activity_log_scroll_offset;
        let scroll_bottom = scroll_top + viewport_height;
        
        for (idx, item) in self.activity_log.iter().enumerate() {
            let item_height = self.get_item_height(idx);
            
            // Only render items that are at least partially visible
            if y_pos + item_height > scroll_top && y_pos < scroll_bottom {
                // Create item at absolute position
                let item_element = self.render_activity_item(item, fonts)
                    .position(Position::Absolute)
                    .top(Dimension::Pixels(y_pos - scroll_top));
                    
                container = container.add_child(item_element);
            }
            
            y_pos += item_height;
            
            // Early exit if we're past visible area
            if y_pos > scroll_bottom {
                break;
            }
        }
        
        container
    }
}

// Note: Interactive elements like buttons within the activity log
// must use the same z-index (10) to be clipped properly.
// The scrollbar at z-index 16 intentionally renders above the clipped content.
```

### Phase 5: Event Handling and Interaction (Week 3)

#### 5.1 Mouse Coordinate Mapping

Since scissor doesn't change coordinate systems, hit testing remains simple:

```rust
impl AiSidebar {
    fn handle_mouse_event(&mut self, event: &MouseEvent) -> Result<bool> {
        // For scrollable content, adjust for scroll offset
        if self.chat_input_bounds.contains(event.coords) {
            let adjusted_y = event.coords.y + self.chat_input.scroll_offset;
            let adjusted_event = MouseEvent {
                coords: euclid::point2(event.coords.x, adjusted_y),
                ..event
            };
            
            return self.chat_input.handle_mouse_event(adjusted_event);
        }
        
        // ... handle other regions ...
    }
}
```

#### 5.2 Text Selection Through Scissor

Text selection works normally since scissor only affects rendering, not interaction:

```rust
// Existing text selection code continues to work
// Just ensure selection rendering uses same z-index as content
```

### Phase 6: Testing and Optimization (Week 3-4)

#### 6.1 Performance Verification

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_scissor_performance() {
        // Verify no performance regression
        // Scissor should have near-zero cost
        
        let start = std::time::Instant::now();
        // Render with scissor
        let scissor_time = start.elapsed();
        
        let start = std::time::Instant::now();
        // Render without scissor
        let no_scissor_time = start.elapsed();
        
        // Should be within measurement noise
        assert!((scissor_time.as_secs_f32() - no_scissor_time.as_secs_f32()).abs() < 0.001);
    }
}
```

#### 6.2 Coordinate System Tests

```rust
#[test]
fn test_coordinate_transforms() {
    // Verify OpenGL Y-flip is correct
    let element_rect = euclid::rect(10.0, 20.0, 100.0, 50.0);
    let window_height = 600.0;
    
    let gl_rect = element_to_opengl_scissor(element_rect, window_height);
    assert_eq!(gl_rect.bottom, 530); // 600 - 20 - 50
}
```

## Implementation Considerations

### Z-Index Management

**Key Principle**: All content that needs the same clipping must share the same z-index.

**Strategy**:
- Reserve specific z-indices for scrollable content
- Use sub-layers (0-2) for visual ordering within a clipped region
- Non-scrollable UI elements use different z-indices

**Interactive Elements Within Scrollable Content**:
- Buttons, links, and other interactive elements MUST use the same z-index as their container to be clipped
- Visual layering within the scrollable area uses sub-layers:
  - Sub-layer 0: Backgrounds
  - Sub-layer 1: Text content  
  - Sub-layer 2: Buttons and interactive elements
- Elements that should NOT be clipped (like scrollbars) use higher z-indices

**Example**:
```rust
// Activity log content and its buttons - all at z-index 10
content.zindex(10);
button.zindex(10).sub_layer(2); // Same z-index, higher sub-layer

// Scrollbar - intentionally above clipped content
scrollbar.zindex(16); // Not affected by z-index 10's scissor
```

### Coordinate System Handling

**WebGPU**: Top-left origin, no conversion needed
**OpenGL**: Bottom-left origin, flip Y coordinate:
```rust
opengl_y = window_height - element_y - element_height
```

### Performance Characteristics

**Zero overhead**: GPU scissor test is hardware accelerated
**No memory cost**: No additional textures or buffers
**No latency**: No frame delays or post-processing

### RefCell Safety Considerations

Given that `ClipBounds::Explicit` causes RefCell panics, we must be careful:

**Safe Pattern**:
```rust
// In render_element - brief borrow and release
if let Some(layer_scissor) = &element.layer_scissor {
    layer.update_scissor_rect(layer_scissor.rect); // Quick borrow/release
}
// RefCell borrow released before any nested operations
```

**Mitigation**:
- Keep scissor rect updates simple and fast
- No nested element processing during scissor updates  
- Clear separation between collection (phase 1) and application (phase 2)

### Limitations

1. **Per-layer only**: Cannot have different scissor rects for elements at same z-index
2. **Rectangular only**: GPU scissor is always axis-aligned rectangles
3. **No nesting**: Cannot have nested scissor regions

These limitations are manageable through careful z-index assignment.

## Migration from Existing Approaches

### From Cut-a-Hole Pattern

1. Remove the "frame" elements at higher z-indices
2. Move content to dedicated z-index with scissor
3. Simpler code, better performance

### From Manual Clipping

1. Remove manual bounds checking in render_element
2. Let GPU handle clipping via scissor
3. More reliable, handles all content types

## Troubleshooting Guide

### Common Issues

**Content not clipped**: Verify z-index is correct and unique to scrollable content
**Wrong clipping bounds**: Check coordinate system (especially OpenGL Y-flip)
**Performance degradation**: Ensure not setting scissor per-element
**RefCell panics**: Clear scissor rects at frame start, not during processing

## Horizontal Scrolling Support

The same scissor rect approach works for horizontal scrolling:

```rust
impl CodeBlock {
    fn render_with_horizontal_scroll(&self, viewport_width: f32) -> Element {
        let viewport = euclid::rect(0.0, 0.0, viewport_width, self.height);
        
        let mut container = Element::new(&fonts.mono, ElementContent::Horizontal)
            .zindex(ZIndex::CODE_BLOCK_CONTENT)
            .with_layer_scissor(viewport);
        
        // Virtual rendering for visible columns
        let first_visible_col = (self.scroll_x / self.char_width) as usize;
        let visible_chars = (viewport_width / self.char_width).ceil() as usize;
        
        // Render only visible portion of each line
        for (line_idx, line) in self.lines.iter().enumerate() {
            let visible_text = line.chars()
                .skip(first_visible_col)
                .take(visible_chars + 1) // +1 for partial chars
                .collect::<String>();
                
            let line_element = Element::new(&fonts.mono, ElementContent::Text(visible_text))
                .position(Position::Absolute)
                .top(Dimension::Pixels(line_idx as f32 * line_height))
                .left(Dimension::Pixels(-(self.scroll_x % self.char_width)));
                
            container = container.add_child(line_element);
        }
        
        container
    }
}
```

Key differences for horizontal scrolling:
- Calculate visible character range instead of line range
- Position text with negative left offset for smooth sub-character scrolling
- Same scissor rect principle applies

## Future Enhancements

### Short Term
1. **Rounded corners**: Use stencil buffer for non-rectangular clipping
2. **Smooth scrolling**: Add momentum and deceleration
3. **Scroll indicators**: Visual feedback for scroll position
4. **Mixed scrolling**: Combined horizontal and vertical in same container

### Long Term
1. **Nested scrollables**: Support scissor rect stacking
2. **Custom clip shapes**: Beyond rectangular regions
3. **Optimized batching**: Group elements by scissor requirements

## Key Implementation Notes

Based on architectural analysis, this plan addresses several critical requirements:

1. **Virtual Rendering**: Instead of runtime offsets, we use virtual rendering where only visible content creates elements with absolute positioning
2. **Per-Frame State Clear**: Scissor rects must be cleared at frame start to prevent persistence
3. **Z-Index Strategy**: Interactive elements must share the parent's z-index to be clipped
4. **RefCell Safety**: Brief, non-nested borrows prevent the panics seen with ClipBounds::Explicit
5. **Coordinate Accuracy**: Proper Y-flip calculation for OpenGL compatibility

## Conclusion

Per-layer scissor rect clipping provides an elegant solution that:
- Works within WezTerm's existing architecture
- Leverages GPU hardware acceleration
- Requires minimal code changes
- Provides immediate, flicker-free clipping
- Maintains all existing functionality

This approach is superior to both the cut-a-hole pattern (simpler) and render-to-texture (better performance, no delays).