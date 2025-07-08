# SCRATCHPAD - Implementation Plans

## Completed: Virtual Scrolling with Height Collection ✓

Successfully implemented virtual scrolling that solves both performance and accuracy issues.

### Key Implementation Details

1. **Height Caching**: Cache heights for ANY visible item (partial or full)
   - `border_rect.size.height` provides full unclipped height
   - Critical for preventing viewport-height-sized jumps

2. **Spacing Constants**: All margins/padding/borders defined as constants
   - Ensures consistency between rendering and estimation
   - Includes SCROLLBAR_SPACE for layout calculations

3. **Buffer Calculation**: Fixed to use `start_idx` for y_offset
   - Prevents jumps when render range changes
   - Maintains stable positioning

4. **Performance**: Removed verbose logging from hot paths
   - Changed to trace level for scroll-time logs
   - Kept essential debug logs only

Virtual scrolling implementation that:
- Only renders visible items (plus 200px buffer)
- Caches actual rendered heights from `border_rect`
- Uses cached heights consistently in all calculations
- Maintains raw ActivityItem data for features

### Results

✅ **No scrolling jumps** - Fixed viewport-height-sized jumps by caching all visible items
✅ **Can scroll to bottom** - Accurate height calculations include all spacing
✅ **Smooth performance** - Only ~10 items rendered regardless of total count
✅ **Maintainable** - Clear constants and separation of concerns

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

3. **Missing Content at Bottom** (PARTIALLY FIXED):

4. **Width Change Sensitivity** (FIXED):
   - Was clearing cache on 0.1 pixel changes
   - Increased threshold to 5.0 pixels
   - Prevents constant cache invalidation

### Updated Understanding

After investigation, we discovered:
- No GPU viewport clipping occurs (only at window boundaries)
- The sidebar uses z-layer tricks to create a visual viewport "window"

### Revised Implementation Plan: Pixel-Based Virtual Scrolling

#### Primary Fix: Pixel-Based Visible Range (Done)

Replace the current item-count buffer (3 items above/below) with a pixel-based approach:

1. **Define a pixel-based render margin** (e.g., 200px beyond viewport)
2. **Only render items that intersect this extended viewport**
3. **This prevents tall items from pulling in many off-screen items**

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

**Session 2 Progress**:

1. **Fixed y_offset calculation bug** (BELIEVED RESOLVED)
   - Issue: Was calculating offset to buffer start (0) instead of first visible item
   - Fix: Changed to use `first_visible` index for y_offset calculation
   - Result: Content now appears at correct position when scrolled

2. **Removed early scan termination** (BELIEVED RESOLVED)
   - Issue: Optimization was stopping scan after viewport, preventing full height calculation
   - Fix: Removed the `break` when past viewport to scan all items
   - Result: Can now see more content (up to "Test message 5 from AI")

3. **Attempted Visual Anchor System** (FAILED - Made things worse)
   - Goal: Prevent content jumping when heights change
   - Implementation: Calculate anchor at 25% viewport, restore position after height changes
   - Problems:
     - Visual anchor is being restored on every frame, not just on height changes
     - Anchor calculation in render_activity_log happens BEFORE height changes
     - The system is fighting against user scrolling
   - Result: Scrolling barely works, constantly jumps back to top

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
- ⚠️ Cannot scroll to see last ~5 items (they use estimated heights that may be too small)
- ⚠️ Some minor scroll position adjustments when heights update, however caching doesn't fix this
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

### Key Implementation Details

- **Height Cache**: `HashMap<String, f32>` keyed by item ID
- **Height Trackers**: `HashMap<String, HeightTracker>` for scroll-based measurement (should not be using anymore)
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

## Future Improvements

1. **Search/Selection on Raw Data** - Not critical for scrolling fix but needed for feature parity
2. **Extract to Reusable Component** - Current implementation works well but could be generalized
3. **Performance Optimizations** - Consider caching filtered items, better data structures

## Documentation Updated

- Added virtual scrolling section to `dev-docs/sidebar-patterns.md`
- Added critical implementation note about height caching
- Added height extraction info to `dev-docs/rendering-pipeline.md`