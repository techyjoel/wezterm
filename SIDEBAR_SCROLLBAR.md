# WezTerm Sidebar Scrollbar Work
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
- The scrollbar thumb moves too fast: when the user scrolls a small amount the thumb goes all the way to the bottom of the track and stays there. We need to properly calculate the length of the content and ensure the thumb position (and the height of the thumb) accurately reflect that as the activity log content changes and scrolls.
- Not all activity log content is being rendered. If we compare the mock content in ai_sidebar.rs to what is being shown, some is being cut off or not displayed for some reason
- There is odd coloration to the left and right of the activity log area. It looks like some margin exists there without any background. This probably needs to be filled in with background color

### 2. Testing & Polish
- [ ] Implement auto-hide behavior with fade in/out on hover and scroll (hide after no activity or after mousing away for 0.25 secs)

### 3. Update Documentation
- [ ] Update TASKS.md with implementation results
- [ ] Document the finished scrollbar architecture and usage after user confirms all bugs are fixed

## Revised Improvement Plan (2025-06-24)

After architectural review, the following issues were identified and a comprehensive improvement plan developed:

### Critical Issues Identified

1. **Hardcoded Height Calculations**
   - Scrollbar calculations assume 40px per item throughout the codebase
   - No adaptation to font size changes, font selection, or line spacing
   - Causes scrollbar thumb to move incorrectly relative to actual content

2. **Mouse Interaction Problems**
   - Filter chips don't respond to clicks consistently
   - `is_over_scrollbar()` always returns false
   - Complex coordinate transformations between components

3. **Incomplete Content Display**
   - Some activity log content is cut off or not displayed
   - Margins showing odd coloration (no background fill)

4. **Performance Issues**
   - Markdown re-renders every frame
   - No caching of computed heights

### Phase 1: Fix Height Calculation System (Priority: Critical)

**1.1 Implement Dynamic Height Measurement**
- Create a proper element measurement system that accounts for:
  - Font metrics (size, family, line height)
  - Text wrapping and line breaks
  - Padding, margins, and borders
  - Nested element structures
- Hook into font configuration changes to invalidate cached measurements
- Store measurements in pixels, not item counts

**1.2 Update ScrollbarInfo Structure**
- Change from item-based to pixel-based:
  ```rust
  pub struct ScrollbarInfo {
      pub should_show: bool,
      pub thumb_position: f32,      // 0.0-1.0
      pub thumb_size: f32,          // 0.0-1.0
      pub content_height: f32,      // Total height in pixels
      pub viewport_height: f32,     // Visible height in pixels
      pub scroll_offset: f32,       // Current offset in pixels
  }
  ```

**1.3 Fix Scrollbar Renderer Initialization**
- Remove all hardcoded 40px multiplications
- Use actual measured heights from ScrollableContainer

### Phase 2: Fix Mouse Interaction (Priority: High)

**2.1 Fix Filter Chip Click Detection**
- Debug coordinate transformation issues
- Ensure filter chip bounds are correctly calculated relative to sidebar position
- Add comprehensive logging for click events and bounds

**2.2 Implement Proper Scrollbar Hit Testing**
- Store scrollbar bounds in the ScrollableContainer
- Implement `is_over_scrollbar()` to use actual bounds
- Consolidate mouse event handling into single flow

**2.3 Add Visual Feedback**
- Highlight filter chips on hover
- Show scrollbar thumb hover state
- Add click feedback animations

### Phase 3: Fix Content Rendering (Priority: Medium)

**3.1 Fix Activity Log Margins**
- Ensure background color fills entire allocated area
- Remove gaps between activity log area and sidebar edges
- Make debug borders toggleable via config

**3.2 Ensure All Content is Visible**
- Validate viewport height calculations
- Check for off-by-one errors in content range calculations
- Add debug mode to show content bounds

**3.3 Implement Scrollbar Auto-hide**
- Fade in on hover or scroll activity
- Fade out after 0.25s of inactivity
- Smooth transitions using existing animation system

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

1. **Immediate fixes** (Phase 1.1-1.3 + Phase 2.1):
   - Dynamic height measurement system
   - Fix filter chip click detection
   - These are blocking usability

2. **User experience** (Phase 2.2-2.3 + Phase 3):
   - Complete mouse interaction fixes
   - Visual polish and feedback
   - Content visibility fixes

3. **Polish** (Phase 4):
   - Performance optimizations
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

- Scrollbar thumb accurately represents visible portion of content
- Scrollbar movement is smooth and proportional to content
- Filter chips respond reliably to clicks
- All activity log content is visible when scrolled
- Performance: <5ms to render frame with activity log
- Font changes automatically update all measurements