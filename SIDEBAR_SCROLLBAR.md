# WezTerm Sidebar Scrollbar Work

## Current Status Summary (2025-06-24)
- **Dynamic height system**: ✅ Implemented with font metrics
- **Pixel-based scrolling**: ✅ Working
- **25% height overestimation**: ❌ Major issue - excess blank space at bottom
- **Filter chip clicks**: ❓ Code complete but needs verification
- **Debug borders**: ❌ Still visible (red borders)
- **Auto-hide scrollbar**: ❌ Not implemented

## As-Implemented Scrollbar Notes

### 1. Fixed Sidebar Background Rendering ✅
- Updated `paint_sidebars()` to use dedicated z-indices
- Each sidebar now allocates its own RenderLayer via `gl_state.layer_for_zindex()`
- Background uses sub-layer 0 for proper ordering

### 2. Right Sidebar "Cut a Hole" Rendering ✅
- Background rendered at z-index 12 with hole cut out for activity log
- Activity log content rendered at z-index 10 (shows through hole)
- Main sidebar content at z-index 14
- Scrollbars at z-index 16
- Implementation in `paint_right_sidebar()` with debug borders

### 2. Created ScrollbarRenderer Component ✅
- New module: `wezterm-gui/src/termwindow/render/scrollbar_renderer.rs`
- Features implemented:
  - Vertical/horizontal orientation support
  - Direct rendering at any z-index
  - Mouse hit testing (thumb, above, below regions)
  - Drag scrolling with state tracking
  - Page up/down on track clicks
  - Hover state management
  - Configurable min thumb size
- Uses RectF and window::PixelUnit types for proper coordinate handling

### 3. Integrated Scrollbar Rendering ✅
- AI sidebar exposes scrollbar info via `get_scrollbars()` trait method
- Added `render_sidebar_scrollbars()` method to render at approporiate z-index
- Removed Element-based scrollbar from ScrollableContainer
- ScrollableContainer now only provides ScrollbarInfo for external rendering

### 4. Current Status ✅
- Basic implementation working but bugs exist:
  - The user can see the scrollbar and activity log
  - The activity log and scrollbar track are properly positioned 
  - The scrollbar thumb moves when the activity log content scrolls

## Known Issues and Remaining Tasks

### 1. Known Issues
- **FACT**: Activity log shows ~25% excess blank space at the bottom when scrolled fully down (confirmed by user with screenshots)
- **FACT**: Debug borders are still visible (red borders around the activity log "hole")
- **THEORY**: The excess space may be caused by compound margins/padding from nested components (Cards add 8px margins, items add margins, markdown adds padding)
- **THEORY**: Height calculations might be double-counting line spacing when applying line height multipliers
- **UNKNOWN**: Whether filter chip click detection is working (code appears complete but needs user confirmation with debug logs)

### 2. Testing & Polish
- [ ] Implement auto-hide behavior with fade in/out on hover and scroll (hide after no activity or after mousing away for 0.25 secs)

### 3. Update Documentation
- [ ] Update TASKS.md with implementation results
- [ ] Document the finished scrollbar architecture and usage after user confirms all bugs are fixed

## Revised Improvement Plan (2025-06-24)

After architectural review, the following issues were identified and a comprehensive improvement plan developed:

### Critical Issues Identified

1. **Height Calculation Accuracy** (Partially Resolved)
   - **RESOLVED**: Dynamic height calculation implemented using font metrics (commit 2a5d73bef)
   - **RESOLVED**: ScrollbarInfo now uses pixel-based calculations instead of item counts
   - **REMAINING**: 25% excess blank space suggests height estimation is still inaccurate
   - **THEORY**: Compound margins/padding from nested components not properly accounted for

2. **Mouse Interaction Problems**
   - **IMPLEMENTED**: Filter chip click detection code exists with bounds checking
   - **UNKNOWN**: Whether clicks are actually working (needs debug log verification)
   - **FACT**: `is_over_scrollbar()` always returns false (not implemented)
   - **RESOLVED**: Coordinate transformations properly handle sidebar X position

3. **Visual Issues**
   - **FACT**: Debug red borders still visible around activity log
   - **RESOLVED**: Background color fills activity log area (commit 2a5d73bef)

4. **Performance Issues**
   - **FACT**: Markdown re-renders every frame
   - **FACT**: No caching of computed heights

### Phase 1: Fix Height Calculation System (Priority: Critical)

**1.1 Implement Dynamic Height Measurement** ✅ COMPLETED
- **DONE**: Created element measurement system using font metrics
- **DONE**: DimensionContext passes font size, line height to ScrollableContainer
- **DONE**: Padding, margins, and borders included in calculations
- **REMAINING**: Height estimation still ~25% too high
- **THEORY**: Compound margins from nested elements (Cards + items + markdown)
- **THEORY**: No margin collapsing implementation (all margins are additive)

**1.2 Update ScrollbarInfo Structure** ✅ COMPLETED
- **DONE**: Changed to pixel-based structure with content_height, viewport_height, scroll_offset
- **DONE**: Kept deprecated item-based fields for compatibility
- **DONE**: All scrollbar calculations now use pixel values

**1.3 Fix Scrollbar Renderer Initialization** ✅ COMPLETED
- **DONE**: Removed hardcoded 40px assumptions
- **DONE**: ScrollbarRenderer uses pixel values from ScrollableContainer

### Phase 2: Fix Mouse Interaction (Priority: High)

**2.1 Fix Filter Chip Click Detection** ⚠️ NEEDS VERIFICATION
- **DONE**: Implemented `get_clicked_filter` method with bounds checking
- **DONE**: `update_filter_chip_bounds` called after rendering to populate bounds
- **DONE**: Mouse events properly forwarded to sidebar
- **DONE**: Debug logging added for click detection
- **ISSUE**: Method still uses hardcoded positions instead of populated bounds
- **TODO**: Verify with debug logs that clicks are being detected
- **TODO**: Ensure visual state updates after filter change

**2.2 Implement Proper Scrollbar Hit Testing**
- **DONE**: Scrollbar bounds stored after rendering
- **NOT DONE**: `is_over_scrollbar()` always returns false
- **DONE**: Mouse event handling consolidated in sidebar

**2.3 Add Visual Feedback**
- **NOT DONE**: No hover states for filter chips
- **NOT DONE**: No scrollbar thumb hover indication
- **NOT DONE**: No click feedback animations

### Phase 3: Fix Content Rendering (Priority: Medium)

**3.1 Fix Activity Log Margins**
- **DONE**: Background color fills activity log area
- **NOT DONE**: Debug red borders still visible
- **TODO**: Make debug borders toggleable via config

**3.2 Fix Height Overestimation**
- **PRIORITY**: Address the 25% excess blank space issue
- **THEORY**: Reduce compound margins between nested components
- **THEORY**: Implement margin collapsing logic
- **THEORY**: Verify line height calculations aren't double-counting spacing

**3.3 Implement Scrollbar Auto-hide**
- **NOT DONE**: No auto-hide behavior
- **TODO**: Fade in on hover or scroll activity
- **TODO**: Fade out after 0.25s of inactivity
- **TODO**: Use existing animation system

### Phase 4: Performance Optimization (Priority: Low)

**4.1 Cache Rendered Content**
- Only re-render markdown when content changes
- Cache computed element heights with invalidation on:
  - Font configuration changes
  - Window resize
  - Content updates

**4.2 Optimize Bounds Calculations**
- Calculate activity log bounds once per frame
- Share bounds between rendering and hit testing
- Avoid redundant measurements

### Implementation Order

1. **Immediate fixes** (High Priority):
   - **Phase 3.2**: Fix 25% height overestimation (blocking proper scrolling)
     - Investigate compound margins in Cards, activity items, and markdown
     - Consider implementing margin collapsing
     - Verify line height calculations
   - **Phase 2.1**: Verify filter chip clicks work with debug logs
     - Update `get_clicked_filter` to use populated bounds
     - Ensure visual state updates after filter selection

2. **Visual polish** (Medium Priority):
   - **Phase 3.1**: Remove/toggle debug borders
   - **Phase 2.3**: Add hover states and visual feedback
   - **Phase 3.3**: Implement scrollbar auto-hide

3. **Performance** (Low Priority):
   - **Phase 4**: Caching and optimizations
   - Can be done after core functionality works

### Testing Strategy

1. Create test scenarios with:
   - Different font sizes (10pt to 20pt)
   - Multiple font families
   - Varying line spacing
   - Long and short activity log items

2. Add debug overlays showing:
   - Calculated heights vs actual rendered heights
   - Mouse coordinates and hit test results
   - Scrollbar state and bounds

3. Automated tests for:
   - Height calculation accuracy
   - Scrollbar position math
   - Mouse hit testing

### Success Criteria

- **PARTIAL**: Scrollbar thumb represents content (but 25% excess space affects accuracy)
- **DONE**: Scrollbar movement is proportional to content
- **UNKNOWN**: Filter chips respond to clicks (needs verification)
- **ISSUE**: 25% excess blank space when fully scrolled
- **NOT TESTED**: Performance metrics
- **DONE**: Font changes update measurements via DimensionContext