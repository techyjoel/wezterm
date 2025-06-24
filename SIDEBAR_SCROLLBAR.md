# WezTerm Sidebar Scrollbar Work
## As-Implemented Scrollbar Notes

### 1. Fixed Sidebar Background Rendering ✅
- Updated `paint_sidebars()` to use dedicated z-indices:
  - Left sidebar background: z-index 4
  - Right toggle button: z-index 16 (same as right sidebar scrollbars)
  - Left toggle button: z-index 36 (same as left sidebar scrollbars)
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
- Added `render_sidebar_scrollbars()` method to render at z-index 12
- Removed Element-based scrollbar from ScrollableContainer
- ScrollableContainer now only provides ScrollbarInfo for external rendering

### 4. Current Status ✅
- Basic implementation complete and compiling:
  - Fixed borrowing conflict by creating static helper function
  - ScrollbarRenderer renders at z-index 12 successfully
  - UI items created for mouse interaction

## Remaining Tasks

### 1. Fix Scrollbar Positioning (Possible methods)
- [ ] Get actual activity log bounds from sidebar instead of hardcoded margins
- [ ] Calculate correct scrollbar position relative to activity log viewport
- [ ] Account for sidebar padding and borders in positioning

### 2. Testing & Polish
- [ ] Add hover animations using ColorEase for visual feedback
- [ ] Implement auto-hide behavior with fade in/out
- [ ] Test with extreme content sizes

### 3. Update Documentation
- [ ] Update TASKS.md with implementation results
- [ ] Document the new scrollbar architecture and usage