# Modal Overlay Framework

## Current Status

**Phases 1-3 Complete** ✅ - Modal system is fully functional with all UI elements working correctly.

### What's Working
- Core modal infrastructure with dimmer and proper z-index layering (20-23)
- Scrollable content with interactive scrollbar (drag, wheel, hover effects)
- Suggestion modal with "more..." link detection and full markdown rendering
- All buttons use UIItemType pattern for accurate click detection
- Close via X button, click outside, or Escape key

### Known Issues
- Modal dimmer background may not stretch to top of screen (positioning regression)

### Remaining Tasks (Phase 4)
1. **Button Actions**: Wire up Run/Dismiss to actual functionality
2. **Keyboard Navigation**: Tab between buttons, Enter to activate, arrow keys to scroll
3. **Visual Polish**: Smooth fade/scale animations, loading states, error handling
4. **Edge Cases**: Handle failures, long-running commands, sidebar/window resize

## Key Implementation Details

### Critical Imports & Types
```rust
// Correct imports (common mistakes fixed)
use crate::color::LinearRgba;  // NOT window::LinearRgba
use termwiz::input::KeyCode;   // NOT wezterm_term::KeyCode
use wezterm_term::KeyModifiers;
use window::MouseEvent;        // With pattern match on event.kind

// RectF construction
euclid::rect(x, y, width, height)  // NOT RectF::new()

// Element z-index
element.zindex()  // NOT with_zindex()

// Dimensions
Dimension::Pixels(value as f32)  // Must be f32, not f64
```

### Modal Manager State
```rust
pub struct ModalManager {
    active_modal: Option<Modal>,
    dimmer_opacity: f32,  // Simplified from ColorEase
    animation_start: Option<std::time::Instant>,
    scroll_offset: f32,
    content_height: f32,
    visible_height: f32,
    // Scrollbar interaction state
    scrollbar_hover: bool,
    scrollbar_dragging: bool,
    drag_start_y: f32,
    drag_start_offset: f32,
}
```

### Z-Index Assignments
- **20**: Dimmer background (semi-transparent)
- **21**: Modal container (main content)
- **22**: Modal content elements
- **23**: Modal scrollbar (highest modal z-index)

### Integration Points

**Rendering** (`termwindow/render/sidebar_render.rs`):
- `render_sidebar_modals()` called after scrollbar rendering (line 650)
- Modals render as separate elements at designated z-indices (20-23)
- Modal elements computed and rendered via standard Element system

**Event Handling**:
- Modal manager checks events before sidebar
- Mouse events need type conversion between `window::MouseEvent` and `wezterm_term::MouseEvent`
- Keyboard events use `wezterm_term::KeyModifiers`

**Files Modified**:
- `sidebar/components/modal/mod.rs` - Modal manager
- `sidebar/components/modal/content.rs` - Content trait
- `sidebar/components/modal/suggestion_modal.rs` - Suggestion implementation
- `sidebar/ai_sidebar.rs` - Added modal_manager field and integration
- `termwindow/render/sidebar_render.rs` - Added render_sidebar_modals()

## Technical Gotchas

### MarkdownRenderer
- Don't store instance - use static methods directly
- Pass registry for code block state management

### Borrow Checker Issues
- Extract values before operations to avoid mutable borrow conflicts
- Be careful with modal manager state updates

### Suggestion Truncation
- Character-based truncation for suggestions exceeding available space
- "more..." link rendered inline after truncated text
- Click detection covers entire suggestion card when truncated
- Exact truncation length may vary based on content and configuration

### Panic Fix
- Avoid `SizedPoly::none()` - causes panic in customglyph.rs
- Use `ElementContent::Text(String::new())` for empty Poly elements
- Always set `display(DisplayType::Block)` on background elements

## Open Design Questions

1. **Run Button Behavior**:
   - What command to execute?
   - How to show progress/output?
   - Should modal stay open after success?

2. **Dismiss Button Behavior**:
   - Remove suggestion permanently or just for session?
   - Should it just close modal?

3. **Error Handling**:
   - What happens if Run fails?
   - How to handle long-running commands?

4. **Performance**:
   - Consider "hole cutting" for scrollable content like activity log
   - Cache markdown rendering results
   - Implement dirty tracking for updates

## Testing Checklist

- [ ] Various content lengths (especially edge cases around 3 lines)
- [ ] Scrolling with very long content
- [ ] Rapid open/close sequences
- [ ] Window/sidebar resize while modal open
- [ ] Keyboard navigation flow
- [ ] Button action integration
- [ ] Animation smoothness at 60fps

## Future Extensions

Once core system complete:
- Confirmation dialogs
- Error/success messages  
- Settings panels
- File pickers
- Screen reader support
- Focus trapping