# Implementation Plan: Adding 9 Layers to WezTerm

## Overview

This document outlines the implementation plan for expanding WezTerm's rendering system from 3 layers to 9 layers using a 3×3 architecture. This will enable complex UI features like scrollbars, modal overlays, and tooltips while maintaining optimal performance at 120fps.

## Architecture Design

### Current State
- 3 fixed layers using `TripleLayerQuadAllocator`
- Hardcoded array sizes and match statements
- Layers 0-2 handle all current rendering needs

### Target State
- 9 layers total using three `TripleLayerQuadAllocator` instances
- Lazy allocation for UI and overlay groups
- Maintain zero overhead for terminal rendering

### Layer Assignment
```
Base Group (always allocated):
  Layer 0: Underlines, backgrounds
  Layer 1: Terminal text, main content  
  Layer 2: Base UI elements, sidebar backgrounds

UI Group (allocated when sidebars active):
  Layer 3: Sidebar content, activity log items
  Layer 4: Scrollbars, UI controls
  Layer 5: Floating panels, expanded cards

Overlay Group (allocated when modals shown):
  Layer 6: Modal overlays, dialogs
  Layer 7: Dropdown menus, autocomplete
  Layer 8: Tooltips, notifications
```

## Implementation Steps

### Phase 1: Core Infrastructure (2-3 hours)

#### 1.1 Create New Layer Management Structure
**File**: `wezterm-gui/src/render/layer_manager.rs` (new)

```rust
use crate::quad::{TripleLayerQuadAllocator, QuadTrait};

pub struct LayerManager {
    base: TripleLayerQuadAllocator,
    ui: Option<Box<TripleLayerQuadAllocator>>,
    overlay: Option<Box<TripleLayerQuadAllocator>>,
}

impl LayerManager {
    pub fn new() -> Self {
        Self {
            base: TripleLayerQuadAllocator::new(),
            ui: None,
            overlay: None,
        }
    }
    
    #[inline(always)]
    pub fn get_layer(&mut self, layer: u8) -> &mut Vec<Box<dyn QuadTrait>> {
        match layer {
            0..=2 => self.base.get_layer(layer),
            3..=5 => self.ui
                .get_or_insert_with(|| Box::new(TripleLayerQuadAllocator::new()))
                .get_layer(layer - 3),
            6..=8 => self.overlay
                .get_or_insert_with(|| Box::new(TripleLayerQuadAllocator::new()))
                .get_layer(layer - 6),
            _ => panic!("Invalid layer {} (must be 0-8)", layer),
        }
    }
}
```

#### 1.2 Add Layer Constants
**File**: `wezterm-gui/src/render/mod.rs` (modify)

```rust
pub mod layer_manager;

/// Layer assignments for consistent usage
pub mod layers {
    pub const UNDERLINE: u8 = 0;
    pub const TEXT: u8 = 1;
    pub const BASE_UI: u8 = 2;
    pub const SIDEBAR_CONTENT: u8 = 3;
    pub const SCROLLBAR: u8 = 4;
    pub const FLOATING_PANEL: u8 = 5;
    pub const MODAL: u8 = 6;
    pub const DROPDOWN: u8 = 7;
    pub const TOOLTIP: u8 = 8;
}
```

### Phase 2: Integration Points (3-4 hours)

#### 2.1 Update RenderState
**File**: `wezterm-gui/src/termwindow/render/mod.rs` (modify)

```rust
// Replace current layer field with LayerManager
pub struct RenderState {
    // ... existing fields ...
    pub layers: RefCell<LayerManager>, // Changed from [TripleVertexBuffer; 3]
}
```

#### 2.2 Update Paint Methods
**File**: `wezterm-gui/src/termwindow/render/paint.rs` (modify)

```rust
impl TermWindow {
    pub fn paint_impl(&mut self, frame: &mut Frame) {
        // Change layer allocation
        let mut layers = self.render_state.layers.borrow_mut();
        
        // Existing painting code remains the same for layers 0-2
        // Just update direct layer access to use layers.get_layer(n)
    }
}
```

#### 2.3 Update Layer Rendering
**File**: `wezterm-gui/src/termwindow/render/draw.rs` (modify)

```rust
// Update draw_layers to handle all 9 layers
pub fn draw_layers(&mut self, layers: &LayerManager) -> anyhow::Result<()> {
    // Render base layers (always present)
    for layer_idx in 0..=2 {
        if !layers.base.is_empty(layer_idx) {
            self.draw_layer(layer_idx, &layers.base)?;
        }
    }
    
    // Render UI layers if present
    if let Some(ref ui) = layers.ui {
        for layer_idx in 0..=2 {
            if !ui.is_empty(layer_idx) {
                self.draw_layer(layer_idx + 3, ui)?;
            }
        }
    }
    
    // Render overlay layers if present
    if let Some(ref overlay) = layers.overlay {
        for layer_idx in 0..=2 {
            if !overlay.is_empty(layer_idx) {
                self.draw_layer(layer_idx + 6, overlay)?;
            }
        }
    }
    
    Ok(())
}
```

### Phase 3: Update Existing Code (4-5 hours)

#### 3.1 Find and Update Layer References
Search and replace patterns:

1. **Direct layer access**:
   ```rust
   // Old: self.filled_rectangle(layers, 2, rect, color)
   // New: self.filled_rectangle(layers, layers::BASE_UI, rect, color)
   ```

2. **Match statements**:
   ```rust
   // Old:
   match layer_num {
       0 => ...,
       1 => ..., 
       2 => ...,
       _ => unreachable!()
   }
   
   // New: No change needed! LayerManager handles internally
   ```

3. **Element z-index**:
   ```rust
   // Old: element.zindex(2)
   // New: element.zindex(layers::SCROLLBAR)
   ```

#### 3.2 Files to Update
Priority files that need layer number updates:
- `wezterm-gui/src/termwindow/render/pane.rs` - Terminal content rendering
- `wezterm-gui/src/termwindow/render/sidebar_render.rs` - Sidebar rendering
- `wezterm-gui/src/termwindow/box_model.rs` - Element rendering
- `wezterm-gui/src/sidebar/components/scrollable.rs` - Scrollbar elements
- `wezterm-gui/src/termwindow/render/fancy_tab_bar.rs` - Tab bar rendering

### Phase 4: Update Components (2-3 hours)

#### 4.1 Update Scrollbar to Use Layer 4
**File**: `wezterm-gui/src/sidebar/components/scrollable.rs`

```rust
// Change:
.zindex(2) // Render on top layer

// To:
.zindex(layers::SCROLLBAR) // Layer 4
```

#### 4.2 Update Sidebar Content to Use Layer 3
**File**: `wezterm-gui/src/sidebar/ai_sidebar.rs`

```rust
// Activity log items should use layer 3
// Scrollbars use layer 4
// Future modals will use layer 6
```

### Phase 5: Testing and Validation (2-3 hours)

#### 5.1 Performance Testing
Create benchmark to verify 120fps maintained:

```rust
#[cfg(test)]
mod layer_perf_tests {
    #[test]
    fn test_render_performance_120fps() {
        // Render frame with all 9 layers active
        // Verify < 8.33ms frame time
    }
    
    #[test]
    fn test_layer_allocation_performance() {
        // Verify lazy allocation works
        // Measure overhead of first UI/overlay use
    }
}
```

#### 5.2 Visual Testing Checklist
- [ ] Terminal text renders correctly (layer 1)
- [ ] Sidebar background renders (layer 2)
- [ ] Sidebar content renders above background (layer 3)
- [ ] Scrollbar renders above content (layer 4)
- [ ] Future: Modal renders above everything (layer 6)

### Phase 6: Documentation (1 hour)

#### 6.1 Update Architecture Docs
- Document layer assignment strategy
- Add examples of proper layer usage
- Update CLAUDE.md with layer information

#### 6.2 Add Migration Guide
- List all changed APIs
- Provide examples of updating custom code
- Document performance characteristics

## Implementation Order

1. **Day 1** (5-6 hours):
   - Phase 1: Core Infrastructure
   - Phase 2: Integration Points
   - Basic testing

2. **Day 2** (6-7 hours):
   - Phase 3: Update Existing Code
   - Phase 4: Update Components
   - Phase 5: Testing and Validation

3. **Day 3** (1-2 hours):
   - Phase 6: Documentation
   - Code review and cleanup
   - Performance optimization if needed

## Risk Mitigation

### Performance Risks
- **Risk**: Frame time exceeds 8.33ms at 120fps
- **Mitigation**: Profile and optimize hot paths, skip empty layers

### Compatibility Risks
- **Risk**: Custom Lua configs may break
- **Mitigation**: Maintain backward compatibility for layers 0-2

### Memory Risks
- **Risk**: Increased memory usage
- **Mitigation**: Lazy allocation ensures memory only used when needed

## Success Criteria

1. **Performance**: Maintain 120fps with all 9 layers active
2. **Compatibility**: Existing code works without modification
3. **Functionality**: Scrollbars and future modals work correctly
4. **Code Quality**: Clean, maintainable implementation

## Future Enhancements

Once 9 layers are working:
1. Add configuration option for max layers
2. Implement layer usage statistics
3. Add debug visualization showing active layers
4. Consider dynamic layer allocation beyond 9 if needed

## Notes

- Keep changes minimal and focused
- Reuse existing TripleLayerQuadAllocator to minimize new code
- Test thoroughly at each phase
- Consider feature flag for rollback if needed

## Post-Implementation: Scrollbar Reversion Plan

Once the 9-layer system is implemented, we should revert the scrollbar from the current Element-based workaround back to direct rendering. This section documents why and how.

### Why Revert to Direct Rendering

1. **Current Workaround Limitations**:
   - Scrollbar implemented as Element due to 3-layer constraint
   - Uses negative margins and Float::Right hacks
   - Mouse events incorrectly routed (clicking cycles filter options)
   - Scrollbar overlaps content slightly
   - Not reusable for other components

2. **Benefits of Direct Rendering with Layer 4**:
   - Clean separation from content (layer 3)
   - Precise pixel positioning without margin hacks
   - Proper mouse hit testing and UIItem registration
   - Better performance (no Element tree overhead)
   - Easily reusable for any scrollable component

### Reversion Implementation Plan

#### Step 1: Create Reusable ScrollbarRenderer
**File**: `wezterm-gui/src/termwindow/render/scrollbar.rs` (new)

```rust
use crate::quad::QuadTrait;
use crate::termwindow::{UIItem, UIItemType};
use crate::sidebar::ScrollbarInfo;

pub struct ScrollbarRenderer {
    pub track_color: LinearRgba,
    pub thumb_color: LinearRgba,
    pub hover_thumb_color: LinearRgba,
    pub width: f32,
}

impl ScrollbarRenderer {
    pub fn render_vertical(
        &self,
        term_window: &mut TermWindow,
        layers: &mut LayerManager,
        x: f32,
        y: f32,
        height: f32,
        info: &ScrollbarInfo,
    ) -> Result<()> {
        // Render track on layer 4
        let track_rect = euclid::rect(x, y, self.width, height);
        term_window.filled_rectangle(
            layers, 
            layers::SCROLLBAR, 
            track_rect, 
            self.track_color
        )?;
        
        // Calculate thumb geometry
        let thumb_height = (info.thumb_size * height).max(20.0);
        let thumb_offset = info.thumb_position * (height - thumb_height);
        
        // Render thumb on layer 4
        let thumb_rect = euclid::rect(x, y + thumb_offset, self.width, thumb_height);
        term_window.filled_rectangle(
            layers,
            layers::SCROLLBAR,
            thumb_rect,
            self.thumb_color
        )?;
        
        // Register UI items for proper mouse interaction
        if thumb_offset > 0.0 {
            term_window.ui_items.push(UIItem {
                x: x as usize,
                y: y as usize,
                width: self.width as usize,
                height: thumb_offset as usize,
                item_type: UIItemType::AboveScrollThumb,
            });
        }
        
        term_window.ui_items.push(UIItem {
            x: x as usize,
            y: (y + thumb_offset) as usize,
            width: self.width as usize,
            height: thumb_height as usize,
            item_type: UIItemType::ScrollThumb,
        });
        
        let below_thumb_height = height - thumb_offset - thumb_height;
        if below_thumb_height > 0.0 {
            term_window.ui_items.push(UIItem {
                x: x as usize,
                y: (y + thumb_offset + thumb_height) as usize,
                width: self.width as usize,
                height: below_thumb_height as usize,
                item_type: UIItemType::BelowScrollThumb,
            });
        }
        
        Ok(())
    }
}
```

#### Step 2: Update ScrollableContainer
**File**: `wezterm-gui/src/sidebar/components/scrollable.rs`

```rust
// Remove the Element-based scrollbar rendering
impl ScrollableContainer {
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        // Remove scrollbar from viewport children
        let viewport_children = vec![content_area];
        // No longer need to add scrollbar element
        
        Element::new(font, ElementContent::Children(viewport_children))
            .display(DisplayType::Block)
            .min_height(Some(Dimension::Pixels(self.viewport_height)))
            // ... rest stays the same
    }
    
    // Remove render_scrollbar_element() method entirely
}
```

#### Step 3: Update Sidebar Rendering
**File**: `wezterm-gui/src/termwindow/render/sidebar_render.rs`

```rust
fn paint_right_sidebar(&mut self, layers: &mut LayerManager) -> Result<()> {
    // ... existing background and element rendering ...
    
    // After element rendering, render scrollbars on layer 4
    let sidebar = self.sidebar_manager.borrow().get_right_sidebar();
    if let Some(sidebar) = sidebar {
        let sidebar_locked = sidebar.lock().unwrap();
        let scrollbars = sidebar_locked.get_scrollbars();
        drop(sidebar_locked);
        
        if let Some(info) = scrollbars.activity_log {
            if info.should_show {
                let scrollbar_renderer = ScrollbarRenderer {
                    track_color: self.palette().scrollbar_thumb.to_linear().mul_alpha(0.3),
                    thumb_color: self.palette().scrollbar_thumb.to_linear(),
                    hover_thumb_color: self.palette().scrollbar_thumb.to_linear().mul_alpha(0.9),
                    width: 8.0,
                };
                
                // Position scrollbar at right edge of activity log
                let scrollbar_x = sidebar_x + sidebar_width - 24.0; // 8px scrollbar + 16px padding
                let scrollbar_y = activity_log_y;
                
                scrollbar_renderer.render_vertical(
                    self,
                    layers,
                    scrollbar_x,
                    scrollbar_y,
                    activity_log_height,
                    &info,
                )?;
            }
        }
    }
    
    Ok(())
}
```

#### Step 4: Clean Up Documentation
- Remove the architectural workaround notes from `scrollable.rs`
- Update TASKS.md to reflect the cleaner implementation
- Document the ScrollbarRenderer API for future use

### Expected Improvements

1. **Fixed Mouse Interaction**: Clicks on scrollbar will work correctly
2. **No Content Overlap**: Scrollbar renders on layer 4, above content
3. **Reusable Component**: Any scrollable area can use ScrollbarRenderer
4. **Better Performance**: Direct rendering without Element overhead
5. **Cleaner Code**: No negative margin hacks or Float workarounds

### Migration Timing

This reversion should be done immediately after the 9-layer system is working and tested. It's a high-priority cleanup that will fix current usability issues and establish the pattern for future UI controls.