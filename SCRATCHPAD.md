# SCRATCHPAD - Implementation Plans

## Previous Implementation: Scrollbar Rendering Fixes (COMPLETED)

The shared scrollbar state implementation from the previous plan has been completed. This included:
- Created shared ScrollbarState struct for consistent state management
- Updated ScrollableContainer and ModalManager to use shared state  
- Fixed scrollbar background color using closure interception
- Implemented animation frame invalidation
- Fixed modal scroll direction and focus issues

---

## Current Implementation: Virtual Scrolling with Height Collection

## Problem Statement

Scrollbars in both the activity log and modals don't scroll far enough to show all content at the bottom. Additionally, we have performance issues when rendering many items. The root causes are:
1. Inaccurate height estimation for wrapped text and markdown content
2. Rendering all items regardless of visibility causes performance degradation with many messages

## Previous Approach: Line Count Capture (Not Viable)

The previous plan attempted to capture actual line counts during text wrapping and store them in the Element struct. This approach failed because:
- Elements are created before wrapping happens
- The actual rendering width isn't known during element creation  
- The architecture has a one-way data flow with no feedback mechanism

## Chosen Solution: Virtual Scrolling with Height Caching

### Core Concept

Implement virtual scrolling that:
1. Only renders visible items (plus a small buffer)
2. Caches rendered heights for accurate scrolling:
   - First render: Use estimates, cache actual heights
   - Subsequent renders: Use cached heights for accuracy
3. Keeps raw message data for search/selection functionality
4. Accepts minor scrollbar jumping on first render only

### Why This Works

1. **Solves Performance**: Only rendering ~10 items instead of 100+ gives immediate speedup
2. **Accurate Heights**: Cached heights from actual rendering, not estimates
3. **Preserves Features**: Search and selection work on raw data, like the terminal
4. **Architecture Compatible**: Works within one-way data flow
5. **Simple Implementation**: No complex height collection mechanisms needed

### Critical Implementation Discoveries

1. **Thread Safety Issue**: The initial approach using closures in a generic ScrollableContainer failed because:
   - Sidebar trait requires Send + Sync
   - Fonts use `Rc<LoadedFont>` which isn't thread-safe
   - Closures capturing fonts can't be stored in Arc<dyn Fn + Send + Sync>

2. **Simplified Approach**: Implemented virtual scrolling directly in AiSidebar:
   - Store raw ActivityItem data (not Elements)
   - Calculate visible range in render_activity_log() 
   - Only create Elements for visible items
   - Height cache stored as HashMap<String, f32> by item ID
   - No need for complex generic container

### Original Implementation Design

The original plan was to create a generic ScrollableContainer that could handle any type of scrollable content:

#### Phase 1: Add computed_height to Element ✓

**Original Plan**: Replace `computed_line_count` with `computed_height: Option<f32>` in Element struct.

**What We Did**: Implemented exactly as planned. Added `with_computed_height()` builder method.

#### Phase 2: Separate Data from Rendering

**Original Plan**: Create a generic ScrollableContainer struct that:
- Stores raw message data (not Elements) as `messages: Vec<ActivityLogMessage>`
- Has a height cache as `HashMap<MessageId, f32>`
- Tracks visible range and selection state
- Handles all scrolling logic internally

**What We Actually Did**: 
- Attempted to create generic `ScrollableContainer<T>` in scrollable_v2.rs
- Hit thread safety issues - closures capturing fonts couldn't be Send + Sync
- **Pivoted to**: Implement virtual scrolling directly in AiSidebar with:
  - Store raw `Vec<ActivityItem>` in AiSidebar
  - Added `activity_log_height_cache: HashMap<String, f32>` to AiSidebar
  - Added `activity_log_visible_range: Range<usize>` to AiSidebar
  - Calculate visible range in `render_activity_log()` method

#### Phase 3: Height Collection Mechanism

**Original Plan**: Pass a HeightCollector through render context and correlate heights based on element order and visible range. Update height cache in parent component after compute_element returns.

**What We Actually Did**: 
- Simplified to use item IDs directly for correlation
- Each ActivityItem has an ID field (Command::id, Chat::id, etc.)
- Heights attached to elements via `with_computed_height()` when cached
- Height cache update mechanism still needs connection to ComputedElement results (pending)

#### Phase 4: Visible Range Calculation ✓

**Original Plan**: Calculate visible range in ScrollableContainer's `update_visible_range()` method.

**What We Did**: Implemented as planned but directly in `render_activity_log()` instead of a separate container.

#### Phase 5: Search and Selection Support

**Original Plan**: Implement search/selection methods on ScrollableContainer that operate on raw data.

**Current Status**: Not yet implemented - will need to add methods to AiSidebar instead.

### Height Estimation Improvements ✓

Implemented in `estimate_activity_item_height()`:
- Base padding of 16px (top + bottom)
- Command items: line height + padding, expanded shows output
- Chat items: estimate wrapped lines with margins
- Suggestions: estimate with 20% overhead for markdown
- Goals: simple text estimation

Uses `estimate_wrapped_lines()` from box_model.rs for consistency.

### Key Implementation Details

1. **Height Caching**: Store heights by message ID to persist across frames
2. **Progressive Refinement**: First render uses estimates, subsequent renders use cached heights
3. **Buffer Size**: Render 3 items above and below viewport for smooth scrolling
4. **Height Invalidation**: Clear cache when container width changes (TODO)
5. **Computed Height Usage**: Elements store cached height via `with_computed_height()`

### Implementation Status

**Completed**:
1. ✓ Step 1: Replace `computed_line_count` with `computed_height` in Element
2. ✓ Step 2: Implemented virtual scrolling directly in AiSidebar (modified from original plan)
3. ✓ Step 3: Height caching with HashMap<String, f32> by item ID
4. ✓ Step 4: Visible range calculation with 3-item buffer
5. ✓ Step 5: Only rendering visible elements (~10 instead of all)
6. ✓ Step 6: Height cache update mechanism in `update_activity_log_height_cache()`
7. ✓ Step 8: Improved height estimation using `estimate_wrapped_lines()`
8. ✓ Step 9: Removed +20px hack
9. ✓ Added cache invalidation on width changes

**Remaining**:
- Step 7: Implement search/selection on raw data (not critical for scrolling fix)
  - Need to add search methods that operate on the raw `Vec<ActivityItem>`
  - Selection state should track item IDs, not rendered elements
  - Similar to how terminal search works on raw buffer

### Fixed Bugs

1. **Negative Heights in Cache** (FIXED):
   - Was caching `bounds.size.height` which included negative scroll margins
   - Fixed by using `content_rect.size.height` instead
   - Heights are now stable and represent actual content size

2. **Scroll Wheel Too Coarse** (FIXED):
   - Was scrolling 3 lines at a time (hardcoded multiplier)
   - Fixed by removing the `* 3.0` multiplier
   - Now scrolls 1 line at a time as expected

3. **Missing Content at Bottom** (FIXED):
   - Visible range calculation excluded items that started within viewport
   - Fixed by including items if they start before viewport end
   - Added check to ensure all items are included if they fit

4. **Width Change Sensitivity** (FIXED):
   - Was clearing cache on 0.1 pixel changes
   - Increased threshold to 5.0 pixels
   - Prevents constant cache invalidation

### Updated Understanding

After investigation, we discovered:
- `border_rect.size.height` represents the **full content height** including padding/borders
- No GPU viewport clipping occurs (only at window boundaries)
- The sidebar uses z-layer tricks to create a visual viewport "window"
- Heights like 7045px are likely accurate measurements of very tall markdown content

### Revised Implementation Plan: Pixel-Based Virtual Scrolling

#### Primary Fix: Pixel-Based Visible Range

Replace the current item-count buffer (3 items above/below) with a pixel-based approach:

1. **Define a pixel-based render margin** (e.g., 200px beyond viewport)
2. **Only render items that intersect this extended viewport**
3. **This prevents tall items from pulling in many off-screen items**

#### Secondary Improvements

1. **Stabilize height measurements**:
   - Keep the hybrid approach for accurate height caching
   - But simplify the logic and fix the math errors
   - Accept that tall items may have slight variations

2. **Optimize for common case**:
   - Most items are smaller than viewport
   - These should get immediate, accurate height caching
   - Tall items can use estimates or progressive refinement

#### Implementation Details

**Step 1: Add tracking structures**
```rust
// Add to AiSidebar struct:
height_trackers: HashMap<String, HeightTracker>,

#[derive(Debug, Clone)]
struct HeightTracker {
    // For tall items - track scroll positions
    top_entered_at: Option<f32>,      // scroll_offset when top edge entered viewport
    bottom_entered_at: Option<f32>,    // scroll_offset when bottom edge entered viewport
    
    // For all items - track if we've seen the full item
    seen_full_height: bool,            // true once bottom edge has been in viewport
    
    // Measurement state
    measured_height: Option<f32>,      // final calculated height
}
```

**Step 2: Modify height extraction logic**
In `update_activity_log_height_cache()`:
```rust
// Re-enable the height extraction (currently disabled with continue)
// But add smart logic:

let viewport_height = /* get from scrollbar info */;
let rendered_height = computed_item.border_rect.size.height;

// Check if this item is fully visible (bottom edge in viewport)
let item_position = /* calculate based on visible_idx and scroll_offset */;
let item_bottom = item_position + rendered_height;
let is_fully_visible = item_bottom <= viewport_height;

if rendered_height < viewport_height && is_fully_visible {
    // Small item, fully visible - cache the height immediately
    self.activity_log_height_cache.insert(item_id.clone(), rendered_height);
    
    // Mark as measured
    self.height_trackers.entry(item_id).or_default().seen_full_height = true;
} else {
    // Tall item or partially visible - need scroll tracking
    // Don't cache the potentially clipped height
}
```

**Step 3: Add scroll-based measurement**
In `render_activity_log()` during visible range calculation:
```rust
// For each item being considered for visibility:
let tracker = self.height_trackers.entry(item_id).or_default();

// Detect when top edge enters viewport
if !tracker.top_entered_at.is_some() && item_overlaps_viewport {
    tracker.top_entered_at = Some(self.activity_log_scroll_offset);
}

// Detect when bottom edge becomes visible
if tracker.top_entered_at.is_some() && !tracker.bottom_entered_at.is_some() {
    if item_bottom <= viewport_end {
        tracker.bottom_entered_at = Some(self.activity_log_scroll_offset);
        
        // Calculate height from scroll distance
        let scroll_distance = tracker.bottom_entered_at.unwrap() - tracker.top_entered_at.unwrap();
        let visible_height_at_top = /* calculate partial visibility at top entry */;
        let measured_height = scroll_distance + visible_height_at_top;
        
        // Cache the measured height
        self.activity_log_height_cache.insert(item_id.clone(), measured_height);
        tracker.measured_height = Some(measured_height);
    }
}
```

**Step 4: Handle edge cases**
- Items visible on first render: Mark for measurement on next scroll
- Scrolling backwards: Measurements work in both directions
- Very tall items: Track cumulative scroll distance

### Revised Implementation Plan

**Key Adjustments**:
- NO hardcoded viewport assumptions
- Use relative safety margins (e.g., 5% of viewport height)
- Both approaches are REQUIRED, not optional phases
- Must handle varying viewport sizes dynamically

**Safety Margin Calculation**:
```rust
let safety_margin = viewport_height * 0.05; // 5% of viewport
let safe_top = safety_margin;
let safe_bottom = viewport_height - safety_margin;
```

### Revised Implementation Tasks

**Task 1: Fix Pixel-Based Visible Range** (PRIORITY 1 - 1 hr)
Replace item-count buffer with pixel-based visibility:
```rust
// In render_activity_log(), replace current visible range logic:
const RENDER_MARGIN: f32 = 200.0; // Render 200px beyond viewport

let viewport_top = self.activity_log_scroll_offset - RENDER_MARGIN;
let viewport_bottom = self.activity_log_scroll_offset + viewport_height + RENDER_MARGIN;

let mut current_y = 0.0;
let mut visible_start = None;
let mut visible_end = 0;

for (idx, item) in self.activity_log.iter().enumerate() {
    let item_height = self.get_activity_item_height(item);
    let item_bottom = current_y + item_height;
    
    // Include if any part intersects the render zone
    if item_bottom > viewport_top && current_y < viewport_bottom {
        if visible_start.is_none() {
            visible_start = Some(idx);
        }
        visible_end = idx + 1;
    } else if visible_start.is_some() {
        // We've passed the visible range, stop scanning
        break;
    }
    
    current_y += item_height;
}
```

**Task 2: Fix Scroll Tracking Math** (PRIORITY 2 - 30 min)
Correct the height calculation formula:
```rust
// Current (wrong): adds item_height twice
let measured_height = (bottom_offset - top_offset) + visible_at_top + item_height;

// Fixed: proper calculation
let measured_height = (bottom_offset - top_offset) + visible_height_at_top_entry;
```

**Task 3: Use Element Y-Position Directly** (PRIORITY 3 - 30 min)
```rust
// In update_activity_log_height_cache, use computed positions:
let item_top = computed_item.border_rect.origin.y;
let item_bottom = item_top + computed_item.border_rect.size.height;
// Instead of manually accumulating heights
```

**Task 4: Simplify Height Caching Logic** (30 min)
Focus on the common case (items smaller than viewport):
```rust
// Only cache heights when we're confident they're accurate
if rendered_height < viewport_height {
    let is_fully_visible = item_top >= 0.0 && item_bottom <= viewport_height;
    if is_fully_visible {
        self.activity_log_height_cache.insert(item_id.clone(), rendered_height);
    }
}
// For tall items, accept estimates or wait for scroll tracking
```

**Task 5: Add Filter Change Handling** (15 min)
```rust
// When filter changes, clear tracking state:
if self.activity_filter != new_filter {
    self.height_trackers.clear();
    // Keep height cache as it's still valid
}
```

**Task 6: Add Performance Logging** (15 min)
```rust
log::debug!(
    "Rendering {} items (indices {}..{}) of {} total, viewport pixels: {:.0}-{:.0}",
    visible_range.len(), visible_start, visible_end, 
    self.activity_log.len(), viewport_top, viewport_bottom
);
```

### Critical Implementation Notes

1. **Relative margins only** - All safety checks must be relative to viewport size
2. **Both methods required** - Small items use extraction, tall items use scroll tracking
3. **Progressive refinement** - Heights improve as user scrolls, but remain stable once measured
4. **No assumptions** - Code must work with any viewport size dynamically

### Benefits of This Approach

1. **Immediate accuracy** for most content (small items)
2. **Handles tall items** correctly via scroll tracking  
3. **No oscillation** - heights are only cached when reliable
4. **Progressive refinement** - heights improve as user scrolls
5. **Maintains virtual scrolling performance** - still only render visible items

### Implementation Status Update

**Session 1 Completed Tasks**:
- ✓ Task 1: Added HeightTracker struct and height_trackers HashMap
- ✓ Task 2: Pass viewport height to update_activity_log_height_cache
- ✓ Task 3: Implemented safe height caching for small items (fully visible check)
- ✓ Task 4: Basic scroll tracking structure in place
- ✓ Width change detection clears both caches

**Session 2 Progress**:

1. **Fixed y_offset calculation bug** (BELIEVED RESOLVED)
   - Issue: Was calculating offset to buffer start (0) instead of first visible item
   - Fix: Changed to use `first_visible` index for y_offset calculation
   - Result: Content now appears at correct position when scrolled

2. **Removed early scan termination** (BELIEVED RESOLVED)
   - Issue: Optimization was stopping scan after viewport, preventing full height calculation
   - Fix: Removed the `break` when past viewport to scan all items
   - Result: Can now see more content (up to "Test message 5 from AI")

3. **Implemented dynamic buffer calculation** (BELIEVED RESOLVED)
   - Issue: Fixed 3-item buffer didn't work well with variable height content
   - Fix: Calculate buffer dynamically to ensure enough content to fill viewport + 400px
   - Result: More items are rendered when needed

4. **Attempted Visual Anchor System** (FAILED - Made things worse)
   - Goal: Prevent content jumping when heights change
   - Implementation: Calculate anchor at 25% viewport, restore position after height changes
   - Problems:
     - Visual anchor is being restored on every frame, not just on height changes
     - Anchor calculation in render_activity_log happens BEFORE height changes
     - The system is fighting against user scrolling
   - Result: Scrolling barely works, constantly jumps back to top

### Current Critical Issues going into Session 3

1. **Visual Anchor System Broken**
   - Restoring anchor way too often
   - Needs to only activate when height cache actually updates and visual positioning would be disrupted
   - Restoring is not helping, it's moving the content

2. **Still Can't Reach Bottom Content**
   - Can only see up to "Test message 5 from AI" (chat15)
   - Items 16-24 are never visible
   - Likely due to height calculation or visible range issues

3. **Content Position Instability**
   - Without visual anchor: Content jumps when heights update
   - With visual anchor: Can't scroll reliably at all
   - Need a better approach to maintain stability, or find bug(s) in the anchor approach

### Testing Plan

1. **Tall item height verification**:
   ```rust
   // Add to update_activity_log_height_cache
   log::info!(
       "Item {}: rendered_height={}, viewport={}, top={}, bottom={}, is_clipped={}",
       item_id, rendered_height, viewport_height, item_top, item_bottom,
       item_bottom > viewport_height || item_top < 0.0
   );
   ```

2. **Scroll tracking accuracy**:
   - Log when items enter/exit viewport
   - Log calculated vs actual heights
   - Verify heights stabilize after measurement

3. **Test scenarios**:
   - Scroll slowly past tall item (cmd1)
   - Scroll quickly past tall item
   - Change filters with tall items visible
   - Resize window width

### Testing Strategy

1. **Scrollbar Stability**:
   - Test scrolling to bottom and staying there
   - Verify no jumping when heights are refined
   - Test with mixed content types (commands, chat, suggestions)

2. **Performance Verification**:
   - Confirm only visible items are rendered
   - Check frame rate with large activity logs
   - Monitor height cache hit rate

3. **Edge Cases**:
   - Empty activity log
   - Single item
   - Items taller than viewport
   - Rapid content additions while at bottom

### Implementation Challenges & Solutions

1. **Challenge**: Thread safety with fonts in closures
   - **Original Approach**: Generic ScrollableContainer with render/estimate callbacks as `Arc<dyn Fn + Send + Sync>`
   - **Problem**: Fonts use `Rc<LoadedFont>` which isn't Send or Sync
   - **Solution**: Abandoned generic container, implemented virtual scrolling directly in AiSidebar

2. **Challenge**: Connecting ComputedElement heights back to cache
   - **Original Plan**: HeightCollector passed through render context
   - **Current Approach**: Added `update_activity_log_height_cache()` method to AiSidebar
   - **Status**: Pending - need to find where ComputedElements are accessible after rendering

3. **Challenge**: Tracking which rendered elements correspond to which items
   - **Original Plan**: Complex mapping based on visible range indices
   - **Solution**: Use item IDs directly for correlation (simpler and more reliable)

4. **Challenge**: Smooth scrolling with changing heights
   - **Solution**: Progressive refinement - accept minor jumping on first render only

## Session 3 Progress

### Major Discovery: Element Hierarchy Mismatch

**Problem**: We were rendering 2-3 items but only finding 1 element in the computed structure.

**Root Cause**: The computed element hierarchy was:
```
root → viewport → content_area → [items]
```

But `update_activity_log_height_cache()` expected:
```
root → content_area → [items]
```

**Fix**: Updated the traversal to go one level deeper to find the actual items.

## Session 4 Progress

### Major Fixes

1. **Fixed Height Extraction Bug**:
   - **Problem**: Position-based calculation was unreliable with virtual scrolling
   - **Root Cause**: Y positions can be negative or have gaps, making position differences incorrect
   - **Fix**: Now using `border_rect.size.height` directly - it includes the full rendered height

2. **Fixed Scroll Tracking Bug**:
   - **Problem**: Heights were being cached as 365px, 645px, 1025px
   - **Root Cause**: Formula was adding viewport height to scroll distance
   - **Fix**: Disabled scroll tracking (commented out) - not needed since border_rect gives full height

3. **Fixed Sticky Bottom Bug**:
   - **Problem**: Massive jumps (3000+ pixels) when reaching "bottom"
   - **Root Cause**: Sticky bottom used hardcoded line_height (20.0) vs actual font metrics
   - **Fix**: Disabled sticky bottom feature - it was a hack causing more problems

4. **Fixed Height Cache Update Bug**:
   - **Problem**: Scroll position would jump when caching heights for first time
   - **Root Cause**: Code was comparing to 0.0 for uncached items instead of estimated height
   - **Fix**: Only adjust scroll position when updating already-cached heights

### Current Implementation Status

**Working Well**:
- ✅ Virtual scrolling renders only visible items
- ✅ Heights are extracted correctly using border_rect (no clipping for tall items)
- ✅ Pixel-based buffer (200px) prevents excessive rendering
- ✅ No more massive jumps at the "bottom"
- ✅ Height caching with hysteresis (2px threshold)

**Remaining Issues**:
- ⚠️ Cannot scroll to see last ~5 items (they use estimated heights that are too small)
- ⚠️ Some minor scroll position adjustments when heights update
- ⚠️ Total content height changes as items are cached (8106px → 7933px)

## Current Architecture

### Height Measurement Approaches

1. **Estimation** (initial render):
   - Uses `estimate_activity_item_height()` 
   - Based on content length and average character width
   - Provides reasonable starting point

2. **Rendered Height Extraction** (when items are visible):
   - Extracts from ComputedElement after layout
   - Most accurate when working correctly
   - Challenges with negative Y positions and coordinate transforms

3. **Scroll Tracking** (for tall items > viewport):
   - Tracks when top/bottom edges enter viewport
   - Calculates height from scroll distance
   - Reliable for very tall content

### Key Implementation Details

- **Height Cache**: `HashMap<String, f32>` keyed by item ID
- **Height Trackers**: `HashMap<String, HeightTracker>` for scroll-based measurement
- **Visible Range**: Calculated with 200px pixel margin beyond viewport
- **Hysteresis**: Only updates cache if height changes by >2px

## Troubleshooting Guide

### Common Issues and Solutions

1. **"Mismatch: expected X item elements, found Y"**
   - Check element hierarchy traversal in `update_activity_log_height_cache()`
   - Verify the structure matches: root → viewport → content_area → items

2. **Heights showing as 5000+ pixels**
   - This occurs when extracting height from container instead of individual item
   - Or when negative Y positions affect height calculations
   - Use position-based calculation (difference between consecutive Y positions)

3. **Content jumping during scroll**
   - Heights are being updated without preserving scroll position
   - Need to implement scroll position adjustment when heights change for items above viewport

4. **Can't see bottom content**
   - Check total height calculation
   - Verify scrollbar max value allows reaching bottom
   - Check visible range calculation includes last items

### Debug Commands

```bash
# See virtual scrolling operation
grep -E "\[VSCROLL\] (Rendering|Total items|Visible range)"

# Check element structure
grep -E "\[VSCROLL\] (Root element|Viewport|Content area|Found.*item elements)"

# Monitor height updates
grep -E "\[VSCROLL\] (Height updated|CALCULATED height|Tall item.*comparison)"

# Check for issues
grep -E "\[VSCROLL\] (Mismatch|CRITICAL|Adjusting scroll)"
```

## Next Steps

1. **Fix "Cannot See Last Items" Issue**
   - Root cause: Items that have never been visible use underestimated heights
   - Solution options:
     a. Improve height estimation (especially for commands/chat)
     b. Pre-render all items once to get accurate heights
     c. Progressively update max scroll as new heights are cached
     d. Add "overscroll" buffer to ensure last items can be reached

2. **Stabilize Minor Scroll Adjustments**
   - Only adjust scroll for items above viewport when height changes significantly
   - Consider tracking cumulative height changes to batch adjustments
   - May need to re-enable a simpler visual anchor system

3. **Clean Up Code**
   - Remove commented-out scroll tracking code after confirming it's not needed
   - Remove visual anchor system if not re-enabling
   - Extract virtual scrolling logic into a more reusable component

## Key Lessons Learned

1. **Element Hierarchy**: Always verify the actual computed structure matches expectations
2. **Don't Assume Clipping**: The rendering system provides full heights even for tall items
3. **Simple Solutions**: Using border_rect directly is simpler than position calculations
4. **Test With Real Data**: Hardcoded values (like line_height = 20.0) cause bugs
5. **Progressive Enhancement**: Start with estimation, refine with actual measurements