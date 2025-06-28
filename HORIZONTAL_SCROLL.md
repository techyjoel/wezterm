# Horizontal Scrolling for Code Blocks - Implementation Status

## Current Status: PARTIALLY WORKING ⚠️

### Working Features ✅
- ✅ **Horizontal scrollbar mechanics** - thumb drag and shift+wheel work correctly
- ✅ **Scrollbar appearance** - thin (8px), proper colors, doesn't shift layout
- ✅ **Scrollbar animations** - fade in/out with correct timing (100ms in, 75ms out, 0.25s delay)
- ✅ **Animation performance** - uses proper WezTerm animation system, stops when not needed
- ✅ **Vertical scroll pass-through** - vertical scrolling works over code blocks and copy buttons
- ✅ **Mouse event routing** - events properly forwarded to sidebar when appropriate
- ✅ **Content Rendering Fixed** - Text no longer truncated, full content is shaped and rendered

### Critical Issues Still Present 🔴

1. **Clipping Not Working**:
   - Content visually overflows code block boundaries when scrolled
   - Manual clipping implemented but not effective (only skips entire glyphs)
   - **Root cause**: Timing mismatch between element processing and GPU drawing
     - Elements push/pop scissor rects during `render_element` (processing phase)
     - GPU drawing happens later in `draw.rs` (draw phase)
     - By draw time, scissor stack doesn't match what it was during element processing
     - Different z-indices don't help - scissor state is per render pass, not per element

2. **Text Selection Broken**:
   - Cannot select any text in the sidebar (not just code blocks)
   - **Root cause**: Element system is purely visual - no text data retained after rendering
   - **Fundamental limitation**: Elements convert text → glyphs → quads with no way back

3. **Borderline Content Detection**:
   - Scrollbars may not appear for content slightly wider than viewport
   - Works in suggestion modal but not in activity log
   - Has 5px buffer but may need different calculations for different contexts

## Text Selection Solutions (Ranked by Implementation Success Likelihood)

### 1. **Parallel Text Mapping** - 85% Success Likelihood ✅
Build a shadow data structure during rendering that maps screen positions to text:

**Implementation:**
- During `wrap_text` and glyph shaping, record character positions
- Store mapping of screen coordinates → text indices
- Create selection overlay at z-index 100+
- Intercept mouse events before UIItem processing
- Render selection highlights behind text

**Pros:**
- Clear implementation path
- Works within current architecture
- Can be built incrementally
- Similar to web browser selection

**Cons:**
- Memory overhead for position tracking
- Need careful handling of wrapped text

### 2. **CopyOverlay Adaptation** - 60% Success Likelihood ⚠️
Reuse WezTerm's existing `CopyOverlay` selection system:

**Implementation:**
- Create `SidebarCopyOverlay` that implements Pane trait
- Feed it pre-computed text lines instead of terminal content
- Overlay handles selection logic (already implemented)
- Modal approach - enter "selection mode"

**Pros:**
- Reuses battle-tested code
- Full keyboard navigation included
- Search functionality for free

**Cons:**
- Modal UX may not fit sidebar usage patterns
- Requires adapting Pane interface for non-terminal content
- Visual overlay might feel foreign

### 3. **Enhanced Element System** - 30% Success Likelihood ❌
Build selection support directly into Element system:

**Implementation:**
- Add `selectable_text` field to Elements
- Global `ElementSelectionManager`
- Custom hit-testing system
- Multi-element selection support

**Pros:**
- Most "native" feeling
- Could support rich content selection

**Cons:**
- Massive architectural change
- Hundreds of edge cases to handle
- Weeks of development time
- High risk of bugs

## Clipping Solution Options

### 1. **Enhanced Manual Clipping** (Recommended) ✅
**Implementation**: Improve current manual clipping to handle partial glyphs
- Check bounds for each glyph/sprite before creating quads
- Clip partial glyphs at boundaries (not just skip entire glyphs)
- Handle both horizontal edges properly

**Pros**:
- Works within current architecture
- Can be implemented incrementally
- No GPU state timing issues
- **Perfect synergy with text selection** - build text map while clipping

**Cons**:
- More complex than current implementation
- Need to handle partial glyph rendering

### 2. **Immediate Mode Rendering** ⚠️
**Implementation**: Apply scissor and draw immediately for each element
- Don't batch quads across elements
- Set scissor → draw element → repeat

**Pros**:
- Scissor state always correct
- Could maintain text mapping during rendering

**Cons**:
- Major architectural change
- Performance impact from many draw calls
- Affects entire rendering pipeline

### 3. **Store Scissor Per Layer** ⚠️
**Implementation**: Capture scissor rect when layer is created
- Store scissor state with each layer
- Apply during draw phase

**Pros**:
- Less invasive than immediate mode
- Preserves batching benefits

**Cons**:
- Still have timing issues within a layer
- Doesn't help with text selection

### 4. **GPU Scissor Rect** (Current Implementation) ❌
**Status**: Already implemented but doesn't work due to timing
- Infrastructure complete (scissor stack, GPU commands)
- Fails because scissor state at draw time ≠ state during element processing

## Solution Intersection with Text Selection

### Manual Clipping + Parallel Text Mapping = Best Synergy 🎯

```rust
// Single pass through glyphs handles both problems:
for glyph in glyphs {
    let bounds = calculate_glyph_bounds(glyph);
    
    // Manual clipping check
    if bounds.intersects(clip_bounds) {
        // Partial clipping if needed
        let clipped_bounds = bounds.intersection(clip_bounds);
        render_clipped_glyph(glyph, clipped_bounds);
        
        // Build text selection map for visible portions
        text_map.add_glyph(
            char_index,
            clipped_bounds,  // Only selectable area
            original_text
        );
    }
    // Skip both rendering and selection mapping if not visible
}
```

**Why this works**:
1. Single iteration through glyphs
2. Consistent visibility for both clipping and selection
3. Only selectable text is what's actually visible
4. Can implement incrementally (clipping first, then selection)

### Other Combinations:
- **Immediate Mode + Text Mapping**: Too much change at once
- **GPU Scissor + Any Selection**: Different abstraction levels
- **Per-Layer Scissor + Selection**: No synergy, separate concerns

## Architectural Insights

### Why GPU Scissor Rect Doesn't Work

The fundamental issue is a **timing mismatch** in WezTerm's rendering pipeline:

1. **Element Processing Phase** (`render_element`):
   - Elements are processed sequentially
   - Each element pushes its clip bounds to scissor stack
   - Quads (vertices) are added to shared buffers
   - Scissor is popped from stack
   - **Key point**: No actual GPU drawing happens here

2. **GPU Drawing Phase** (`draw.rs`):
   - Happens after ALL elements are processed
   - Checks current scissor stack state (likely empty or wrong)
   - Draws entire vertex buffer with single scissor state
   - **Problem**: Scissor state now ≠ what it was during element processing

3. **Why Different Z-Indices Don't Help**:
   - Each z-index creates a separate layer with its own render pass
   - But scissor rect is still applied per render pass, not per element
   - All elements in that z-index still share the same scissor state

### The Manual Clipping Advantage

Manual clipping avoids the timing issue entirely:
- Clipping decisions made during element processing
- No dependency on GPU state
- Works with the batched rendering architecture
- Can be enhanced to handle partial glyphs

## Manual Clipping Implementation Plan

### Phase 1: Fix Current Whole-Glyph Clipping (Testing Ground)

**Goal**: Get the current implementation working to validate the approach before adding partial glyph support.

**Current Issues**:
- Glyphs are being skipped but content still overflows
- Clip bounds may not be calculated correctly
- Clipping logic may have bugs

**Implementation Steps**:

1. **Debug Current Clipping**:
   ```rust
   // Add extensive logging to understand what's happening
   log::info!("Clip bounds: {:?}, glyph bounds: [{}, {}]", 
       clip_bounds, glyph_left, glyph_right);
   ```

2. **Verify Clip Bounds Calculation**:
   - Check that `element.clip_bounds` contains correct absolute coordinates
   - Ensure clip bounds are in same coordinate space as glyph positions
   - Log the transformation from content_rect to clip_bounds

3. **Fix Coordinate System Issues**:
   - Glyph positions use `pos_x` (content-relative)
   - But may need window-relative coordinates for clipping
   - Ensure both use same coordinate system

4. **Test with Visible Boundaries**:
   - Temporarily render clip bounds as colored rectangles
   - Visually verify bounds are where expected
   - Confirm glyphs outside bounds are actually being skipped

### Phase 2: Enhanced Manual Clipping with Partial Glyphs

**Goal**: Implement proper clipping that handles glyphs crossing boundaries.

**Key Challenges**:
- Glyphs crossing left/right edges need partial rendering
- Must adjust texture coordinates to show only visible portion
- Need to modify quad positions to align with clip edge

**Implementation Steps**:

1. **Calculate Glyph Intersection**:
   ```rust
   enum GlyphClipResult {
       FullyVisible,
       FullyClipped,
       PartiallyClipped {
           visible_left: f32,
           visible_right: f32,
           texture_offset_x: f32,
           texture_width: f32,
       }
   }
   
   fn calculate_glyph_clip(glyph_bounds: RectF, clip_bounds: RectF) -> GlyphClipResult {
       if !glyph_bounds.intersects(&clip_bounds) {
           return GlyphClipResult::FullyClipped;
       }
       
       if clip_bounds.contains_rect(&glyph_bounds) {
           return GlyphClipResult::FullyVisible;
       }
       
       // Calculate visible portion
       let visible_left = glyph_bounds.min_x().max(clip_bounds.min_x());
       let visible_right = glyph_bounds.max_x().min(clip_bounds.max_x());
       
       // Calculate texture coordinate adjustments
       let glyph_width = glyph_bounds.width();
       let left_clip = (visible_left - glyph_bounds.min_x()) / glyph_width;
       let right_clip = (glyph_bounds.max_x() - visible_right) / glyph_width;
       
       GlyphClipResult::PartiallyClipped {
           visible_left,
           visible_right,
           texture_offset_x: left_clip,
           texture_width: 1.0 - left_clip - right_clip,
       }
   }
   ```

2. **Modify Quad Creation**:
   ```rust
   match calculate_glyph_clip(glyph_bounds, clip_bounds) {
       GlyphClipResult::FullyClipped => continue,
       
       GlyphClipResult::FullyVisible => {
           // Current behavior - render full glyph
           quad.set_position(glyph_pos_x + left, pos_y, ...);
           quad.set_texture(texture.texture_coords());
       }
       
       GlyphClipResult::PartiallyClipped { visible_left, visible_right, texture_offset_x, texture_width } => {
           // Adjust quad position to visible portion
           quad.set_position(
               visible_left + left,
               pos_y,
               visible_right + left,
               pos_y + height
           );
           
           // Adjust texture coordinates
           let tex_coords = texture.texture_coords();
           let adjusted_coords = TextureRect::new(
               Point2D::new(
                   tex_coords.min_x() + texture_offset_x * tex_coords.width(),
                   tex_coords.min_y()
               ),
               Size2D::new(
                   tex_coords.width() * texture_width,
                   tex_coords.height()
               )
           );
           quad.set_texture(adjusted_coords);
       }
   }
   ```

3. **Handle Edge Cases**:
   - Very small visible portions (< 1 pixel)
   - Sprite elements (block drawing characters)
   - Multi-line text elements
   - RTL text (if supported)

### Phase 3: Integration with Text Selection

**Goal**: Build text selection map during clipping pass.

**Implementation**:
1. Add `text_map` parameter to render_element
2. During glyph processing, record positions for visible portions only
3. Map clipped bounds to original text indices
4. Store in sidebar state for selection handling

### Testing Strategy

1. **Phase 1 Tests**:
   - Create code block with known width
   - Scroll to specific offset
   - Verify no glyphs rendered outside bounds
   - Test with different font sizes

2. **Phase 2 Tests**:
   - Code block with text exactly at boundaries
   - Verify partial glyphs render correctly
   - Check texture coordinates are adjusted properly
   - Test scrolling reveals/hides partial glyphs smoothly

3. **Visual Debug Mode**:
   - Add config option to show clip bounds
   - Highlight partially clipped glyphs
   - Show glyph bounds for debugging

## Debug Information Added

### Logging Points
1. **Scissor rect operations**: `push_scissor`, `pop_scissor` in `renderstate.rs` (info level)
2. **GPU scissor application**: WebGPU and OpenGL paths in `draw.rs` (info level)
3. **Clip bounds computation**: `compute_clip_bounds` in `box_model.rs` (info level)
4. **Element clipping**: `render_element` clip bounds application (info level)
5. **Code block measurements**: Content width, viewport width, line counts in `markdown.rs` (info level)
6. **Horizontal scroll setup**: Line processing and viewport creation in `horizontal_scroll.rs` (info level)

### Next Investigation Steps

1. **Run with logging enabled**: `WEZTERM_LOG=info ./target/release/wezterm`
2. **Check scissor rect values**: Verify bounds are correct and stack operations work
3. **Monitor GPU state**: Confirm scissor is only set once per render pass
4. **Track element rendering**: See which elements have clip bounds and their values
5. **Analyze content width**: Compare measured vs rendered widths

### Completed Infrastructure ✅

#### Phase 1: GPU Scissor Rect Infrastructure
- ✅ Added `scissor_stack: RefCell<Vec<RectF>>` to `RenderState`
- ✅ Implemented stack-based scissor rect management with automatic intersection
- ✅ Added OpenGL scissor implementation with coordinate transformation
- ✅ Added WebGPU scissor implementation
- ✅ Added bounds validation and safety checks

#### Phase 2: Element System Integration
- ✅ Created `ClipBounds` enum with `ContentBounds` and `Explicit` variants
- ✅ Added `clip_bounds` field to both `Element` and `ComputedElement`
- ✅ Added builder methods for setting clip bounds
- ✅ Updated `compute_element()` to transform clip bounds to absolute coordinates
- ✅ Updated `render_element()` to push/pop scissor rects

#### Phase 3: Animation & Event Fixes
- ✅ Fixed animation timing to respect actual FPS setting
- ✅ Implemented proper animation scheduling with `has_animation`
- ✅ Added animation stop logic when all animations complete
- ✅ Fixed mouse event routing for vertical scroll pass-through
- ✅ Updated scrollbar thickness and appearance

### Working Features (Pre-existing) ✅
- **Basic horizontal scrolling mechanics** in activity log (thumb drag and shift+wheel)
- **Scrollbar thumb properly positioned** - appears within track
- **Copy button always visible** above code blocks
- **Modal shows horizontal scrollbars** (but scrolling doesn't work)
- **UIItems properly extracted** for mouse event handling



## Todo List

### Immediate (Fix Clipping)
- [x] ~~Implement scissor rect infrastructure in RenderState~~ ✅ DONE
- [x] ~~Add OpenGL scissor implementation~~ ✅ DONE
- [x] ~~Add WebGPU scissor implementation~~ ✅ DONE
- [x] ~~Extend ComputedElement with clip_bounds~~ ✅ DONE
- [x] ~~Update render_element to use scissor rects~~ ✅ DONE
- [x] ~~Apply unique z-index per code block for proper clipping~~ ✅ DONE
- [ ] Test clipping with multiple code blocks
- [ ] Verify clipping works with both OpenGL and WebGPU

### Short-term (Complete Horizontal Scrolling)
- [ ] Fix content truncation issue - investigate why scrolling doesn't reveal hidden content
- [ ] Choose and implement text selection solution (Terminal panes vs Element selection)
- [ ] Connect modal to code block registry - pass registry through modal context
- [ ] Test with very long code lines
- [ ] Document the chosen text selection approach

### Medium-term (Vertical Scrolling Migration)
- [ ] Use scissor rect for activity log scrolling
- [ ] Remove cut-a-hole z-index pattern from sidebar_render.rs
- [ ] Simplify activity log to single z-index
- [ ] Add viewport culling optimization
- [ ] Update documentation

### Long-term (Polish & Performance)
- [ ] Profile performance impact
- [ ] Add scissor rect debugging visualization
- [ ] Consider caching rendered content
- [ ] Document new clipping architecture
- [ ] Add unit tests for clipping behavior