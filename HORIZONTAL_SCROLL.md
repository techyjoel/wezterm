# Horizontal Scrolling for Code Blocks - Implementation Status

## Current Status: MECHANICALLY WORKING, VISUALLY BROKEN ⚠️

### Working Features ✅
- ✅ **Horizontal scrollbar mechanics** - thumb drag and shift+wheel work correctly
- ✅ **Scrollbar appearance** - thin (8px), proper colors, doesn't shift layout
- ✅ **Scrollbar animations** - fade in/out with correct timing (100ms in, 75ms out, 0.25s delay)
- ✅ **Animation performance** - uses proper WezTerm animation system, stops when not needed
- ✅ **Vertical scroll pass-through** - vertical scrolling works over code blocks and copy buttons
- ✅ **Mouse event routing** - events properly forwarded to sidebar when appropriate
- ✅ **Content Rendering Fixed** - Text no longer truncated, full content is shaped and rendered
- ✅ **Unique Code Block IDs** - Each code block has unique ID preventing scroll state sharing

### Critical Issues Still Present 🔴

1. **Clipping Not Working**:
   - Content visually overflows code block boundaries when scrolled
   - **Attempted Solutions**:
     a) **GPU Scissor Rect** (FAILED - Removed):
        - Implemented full scissor stack infrastructure in RenderState
        - Added scissor push/pop in render_element
        - **Failed because**: Scissor state set during element processing, but GPU draws later in separate batched render passes
        - **Outcome**: Didn't cause crashes but clipping was ineffective, removed during cleanup
     b) **Manual Clipping with Partial Glyphs** (IMPLEMENTED but INEFFECTIVE):
        - Added coordinate-corrected clipping logic with partial glyph support
        - Adjusts texture coordinates for glyphs at boundaries
        - **Failed because**: Even with correct coordinates and max_width constraints, content still overflows
        - **Theory**: May be rendering issue with how Elements handle overflow
     c) **Explicit ClipBounds** (CAUSES CRASHES):
        - Setting ClipBounds::Explicit on viewport element
        - **Failed because**: Triggers RefCell BorrowMutError in quad allocator
        - **Root cause**: WezTerm's quad allocator has nested borrow issues with clipped elements

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

### 4. **GPU Scissor Rect** ❌ ATTEMPTED AND REMOVED
**Status**: Was fully implemented but had to be removed
- Infrastructure was complete (scissor stack, GPU commands in both WebGPU and OpenGL)
- **Failed because**: 
  - Scissor state set during element processing but GPU draws in batched render passes later
  - Timing mismatch: scissor stack is empty by draw time
  - Each z-index has its own render pass, can't clip across layers
- **Outcome**: Clipping was ineffective, code removed during cleanup

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

### WezTerm's Rendering Architecture (VERIFIED)

WezTerm uses a **batched rendering pipeline** that fundamentally conflicts with per-element clipping:

1. **Element Processing Phase** (`render_element`):
   - Elements are processed recursively, building a tree
   - Each element calls `quad_allocator()` which returns a `BorrowedLayers` struct
   - `BorrowedLayers` holds an immutable borrow of the vertex buffers via RefCell
   - Quads are allocated and vertices added to layer-specific buffers (0=background, 1=text, 2=sprites)
   - **Critical**: The immutable borrow is held for the entire element tree processing

2. **GPU Drawing Phase** (`draw.rs`):
   - After ALL elements are processed, iterate through layers
   - Each layer draws its entire vertex buffer in one draw call
   - Draw parameters (like scissor) apply to the entire draw call
   - **Key insight**: One scissor rect per draw call, not per element

3. **The RefCell Borrow Problem** (VERIFIED):
   - `quad_allocator()` creates `BorrowedLayers` with `self.vb.borrow()` 
   - This immutable borrow is extended via unsafe lifetime extension
   - When allocating quads, it needs `current_vb_mut()` which calls `self.bufs.borrow_mut()`
   - **Crash**: Can't get mutable borrow while immutable borrow exists
   - This happens when using ClipBounds::Explicit, likely due to deeper element nesting

4. **Why Different Z-Indices Make It Worse**:
   - Each z-index creates a separate RenderLayer
   - More layers = more potential for nested borrows
   - The borrow checker can't track the complex lifetime relationships

### Why Manual Clipping Also Failed

Despite implementing manual clipping with:
- Correct coordinate system (applied `left` offset to match rendering)
- Partial glyph support with texture coordinate adjustment
- Proper bounds checking before creating quads

**It still doesn't work because**:
- The viewport element expands to contain all content despite max_width constraint
- Even with manual clipping in render_element, the element's background/borders render at full size
- The Element system doesn't have a true "overflow: hidden" concept
- Parent elements don't constrain child rendering bounds

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

#### Phase 1: GPU Scissor Rect Infrastructure (REMOVED)
- ❌ ~~Added `scissor_stack: RefCell<Vec<RectF>>` to `RenderState`~~ - REMOVED
- ❌ ~~Implemented stack-based scissor rect management~~ - REMOVED
- ❌ ~~Added OpenGL scissor implementation~~ - REMOVED
- ❌ ~~Added WebGPU scissor implementation~~ - REMOVED
- All GPU scissor code removed as it was ineffective due to timing issues

#### Phase 2: Element System Integration (PARTIALLY KEPT)
- ✅ Created `ClipBounds` enum with `ContentBounds` and `Explicit` variants
- ✅ Added `clip_bounds` field to both `Element` and `ComputedElement`
- ✅ Added builder methods for setting clip bounds
- ✅ Updated `compute_element()` to transform clip bounds to absolute coordinates
- ✅ Implemented manual clipping with partial glyph support in `render_element()`
- ❌ Cannot use ClipBounds::Explicit due to crashes

#### Phase 3: Animation & Event Fixes
- ✅ Fixed animation timing to respect actual FPS setting
- ✅ Implemented proper animation scheduling with `has_animation`
- ✅ Added animation stop logic when all animations complete
- ✅ Fixed mouse event routing for vertical scroll pass-through
- ✅ Updated scrollbar thickness and appearance

#### Phase 4: Scroll State Management
- ✅ Made code block IDs unique by adding context prefix
- ✅ Fixed scroll state sharing between different code blocks
- ✅ Separate IDs for activity log, modal, and suggestion items

### Working Features (Pre-existing) ✅
- **Basic horizontal scrolling mechanics** in activity log (thumb drag and shift+wheel)
- **Scrollbar thumb properly positioned** - appears within track
- **Copy button always visible** above code blocks
- **Modal shows horizontal scrollbars** (but scrolling doesn't work)
- **UIItems properly extracted** for mouse event handling



## Paths Forward

### Option 1: CSS-Style Overflow Container (Most Promising)
Create a new Element type that truly constrains child rendering:
- Add `ElementContent::OverflowContainer { children, overflow_x, overflow_y }`
- Implement proper bounds checking at the container level
- Render to texture first, then blit only visible portion
- **Pros**: Works with existing architecture, true clipping
- **Cons**: Significant implementation effort, performance impact

### Option 2: Pane-Based Code Blocks
Treat each code block as a mini terminal pane:
- Implement Pane trait for code blocks
- Use existing terminal scrolling/clipping infrastructure
- **Pros**: Reuses battle-tested code, gets selection for free
- **Cons**: Heavy-weight solution, may feel foreign

### Option 3: Immediate Mode Rendering for Code Blocks
Render code blocks in a separate pass with proper clipping:
- Extract code blocks from normal element tree
- Render them after main content with scissor rects
- **Pros**: Avoids borrow checker issues
- **Cons**: Breaks element tree abstraction, complex integration

### Option 4: Virtual Scrolling (Workaround)
Only render visible portion of code:
- Calculate visible lines based on scroll offset
- Only create elements for visible content
- **Pros**: No clipping needed, works today
- **Cons**: Can't partially show lines at boundaries, jarring UX

## Recommendations

1. **Short term**: Implement virtual scrolling as a workaround to get something usable
2. **Long term**: Design and implement proper overflow containers in the Element system
3. **Alternative**: Investigate using terminal Panes for code blocks if selection is critical
- [ ] Add unit tests for clipping behavior