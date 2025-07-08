# Sidebar Implementation Patterns

## Critical Implementation Notes

### Click Detection Pattern (MANDATORY)
**Always use UIItemType** for interactive elements:
```rust
// CORRECT - WezTerm automatically tracks bounds
element.with_item_type(UIItemType::MyButton(data))

// WRONG - Never manually track bounds or calculate positions
// This will fail due to coordinate system mismatches
```
**Type**: Architectural requirement (UIItem system design)

### Window Resizing Bug (KNOWN ISSUE)
```rust
// WRONG - Causes window resize failures during collapse
fn get_window_expansion(&self) -> u16 {
    if self.config.mode == SidebarMode::Expand && 
       (self.right_state.animation_target_visible || self.right_state.is_animating()) {
        // This breaks window shrinking!
    }
}

// CORRECT - Only expand when sidebar should be visible
fn get_window_expansion(&self) -> u16 {
    if self.config.mode == SidebarMode::Expand && 
       self.right_state.animation_target_visible {
        // Use animation_target_visible, NOT is_animating()
    }
}
```
**Why**: Returning expansion during collapse animation creates circular resize calculations.
**Type**: Current implementation bug (specific to our expand mode implementation)

### Font Thread Safety
- **NEVER** store `Rc<LoadedFont>` in sidebar state (not Send/Sync)
- Fonts MUST be resolved in main thread during rendering
- Pass fonts via `&SidebarFonts` parameter to render methods
**Type**: Rust type system requirement (thread safety)

### Import Patterns (COMPILATION REQUIREMENTS)
```rust
// Component files (e.g., in sidebar/components/) MUST use:
use ::window::color::LinearRgba;  // NOT crate::color::LinearRgba
use termwiz::input::KeyCode;      // NOT wezterm_term::KeyCode

// Core sidebar files use:
use crate::color::LinearRgba;

// Dimension types MUST be f32:
Dimension::Pixels(value as f32)   // NOT f64
```
**Type**: Module visibility rules (Rust compilation requirements)

### Patterns That Cause Issues
- Use `ElementContent::Text(String::new())` for empty content instead of empty Poly
- Always set `display(DisplayType::Block)` on background elements for proper layout
**Type**: Best practices for avoiding layout issues

### Virtual Scrolling Height Caching (CRITICAL)
```rust
// WRONG - Only caching fully visible items causes jumps
if item_top >= 0.0 && item_bottom <= viewport_height {
    cache_height(item_id, height);
}

// CORRECT - Cache ANY visible item
if item_bottom > 0.0 && item_top < viewport_height {
    cache_height(item_id, height); // border_rect provides full unclipped height
}
```
**Why**: Using estimated heights when actual heights differ causes viewport-height-sized jumps when items enter/exit the render buffer.
**Type**: Implementation requirement (prevents scrolling jumps)

## Overview

WezTerm's sidebar system provides AI assistance features with a sophisticated UI. This document captures implementation patterns and lessons learned.

## Architecture

### Core Components

- **SidebarManager** (`sidebar/mod.rs`) - Orchestrates sidebar lifecycle
- **AISidebar** (`sidebar/ai_sidebar.rs`) - Main sidebar implementation
- **Component System** (`sidebar/components/`) - Reusable UI components
- **Render Integration** (`termwindow/render/sidebar_render.rs`) - GPU rendering

### State Management

Sidebars maintain their own state separate from terminal:
- Animation states (expand/collapse)
- Scroll positions
- Modal visibility
- Component-specific state

Key pattern: State stored in sidebar, rendering computed fresh each frame.

## UI Component Patterns

### Click Detection via UIItemType

Implementation steps:

1. Add variant to `UIItemType` enum in `termwindow/mod.rs`:
   ```rust
   UIItemType::SidebarButton { action: SidebarAction },
   ```

2. Attach to element during render:
   ```rust
   element.with_item_type(UIItemType::SidebarButton { action })
   ```

3. Handle in `mouseevent.rs`:
   ```rust
   UIItemType::SidebarButton { action } => {
       self.handle_sidebar_button(action, event);
   }
   ```

The system automatically tracks exact rendered bounds.

### Modal Framework

Modals use a three-layer approach:
1. **Dimmer** (z-index 20) - Semi-transparent background
2. **Modal** (z-index 21-22) - Content container
3. **Controls** (z-index 23) - Scrollbar, buttons

Key files:
- `sidebar/components/modal/mod.rs` - Modal manager
- `sidebar/components/modal/content.rs` - Content trait
- `sidebar/components/modal/suggestion_modal.rs` - Example implementation

Modal integration points:
- Render via `render_sidebar_modals()` 
- Event handling before sidebar processing
- Escape key and click-outside dismissal

### Scrollable Regions

The "cut-a-hole" pattern for scrollable content:

1. Render scrollable content at lower z-index (e.g., 10)
2. Render container background at higher z-index (e.g., 12) 
3. Exclude rectangular region where content shows through
4. Scrollbar at highest z-index (e.g., 16)

This enables independent scrolling while maintaining visual hierarchy.

### Virtual Scrolling

For lists with many variable-height items, use virtual scrolling to maintain performance:

```rust
// Key components:
// 1. Height cache - stores actual rendered heights
let mut height_cache: HashMap<String, f32> = HashMap::new();

// 2. Calculate visible range with pixel-based buffer
const RENDER_MARGIN: f32 = 200.0; // Render 200px beyond viewport
let viewport_start = scroll_offset;
let viewport_end = scroll_offset + viewport_height;

// 3. Only render items that intersect the extended viewport
// See ai_sidebar.rs:render_activity_log() for full implementation
```

**Critical gotchas:**
- **Cache heights for ALL visible items**, not just fully visible ones. The rendering system provides full unclipped heights via `border_rect.size.height`.
- **Use cached heights everywhere** - in buffer calculations, offset calculations, and total height calculations. Mixing estimated and actual heights causes viewport-height-sized jumps.
- **Account for item spacing** - cached heights include padding/borders but NOT margins between items. Track spacing separately.
- **Coordinate system** - item positions after rendering are viewport-relative (0 = top of viewport).

**Height estimation for initial render:**
```rust
// Include all spacing in estimates
let spacing = get_activity_item_spacing(item); // padding + margin + border
let content_height = calculate_content_height(item);
let estimated_height = content_height + spacing;
```

## Font Management

### Multi-Font Architecture

Sidebars use separate fonts from terminal:
```rust
pub struct SidebarFonts {
    pub heading: Rc<LoadedFont>,
    pub body: Rc<LoadedFont>,
    pub body_bold: Option<Rc<LoadedFont>>,
    pub body_italic: Option<Rc<LoadedFont>>,
    pub body_bold_italic: Option<Rc<LoadedFont>>,
    pub code: Rc<LoadedFont>,
    pub code_bold: Option<Rc<LoadedFont>>,
    pub code_italic: Option<Rc<LoadedFont>>,
    pub code_bold_italic: Option<Rc<LoadedFont>>,
    pub code_line_height: f64,
    pub code_line_margin: f64,
}
```

Fonts resolved in main thread during render, passed to components.
Font variants (bold/italic) are loaded lazily and fall back to base font if unavailable.

### Font Configuration

Via `clibuddy.right_sidebar.fonts` in wezterm.lua:
- Font families, sizes, weights
- Syntax highlighting dimming factor
- Code block styling

## Animation Patterns

### Window Resize Coordination

Critical for expand mode sidebars:

1. **Expansion Calculation** - Only return non-zero from `get_window_expansion()` when sidebar should be visible (NOT during animations)
2. **Animation States** - Track `animation_target_visible` not just `is_animating()`
3. **Resize Flow** - Window resize already includes desired expansion
4. **Minimum Width** - When sidebar button is visible but sidebar collapsed, returns MIN_SIDEBAR_WIDTH (25 pixels)

Common bug: Returning expansion during collapse animation prevents window shrinking.

### Smooth Transitions

Using `ColorEase` for animations:
- Fade effects: 200ms in, 150ms out
- Position transitions: EaseOut for natural feel
- Opacity animations: Linear for simplicity

Track animation progress with `Instant::now()` timestamps.

## Component Guidelines

### Activity Log
- Virtualized rendering (only visible entries)
- Entry pooling to reduce allocations
- Markdown parsing cached where possible

### Chips (Interactive Tags)
- Hover states via UIItemType
- Consistent padding/margins
- Theme-aware colors

### Markdown Rendering
- Don't store MarkdownRenderer instances
- Pass registries for code state
- Handle nested structures carefully

## Common Gotchas

### Import Confusion
```rust
// Correct imports
use crate::color::LinearRgba;      // In core sidebar files
use ::window::color::LinearRgba;  // In component files
use termwiz::input::KeyCode;       // NOT wezterm_term::KeyCode
use wezterm_term::KeyModifiers;
```

### Element Construction
```rust
// Correct patterns
euclid::rect(x, y, w, h)          // NOT RectF::new()
element.zindex()                   // NOT with_zindex()
Dimension::Pixels(val as f32)      // Must be f32
```

### Avoiding Panics
- Never use `SizedPoly::none()` - causes panic
- Use `ElementContent::Text(String::new())` for empty Poly
- Always set `display(DisplayType::Block)` on containers

### Borrow Checker
- Extract values before mutable operations
- Be careful with nested borrows in callbacks
- Use `Rc<RefCell<>>` sparingly

## Performance Optimization

### Rendering
- Minimize z-index layers
- Batch similar elements
- Cache computed layouts
- Use dirty flags for updates

### Memory
- Pool frequently allocated objects
- Clear old state regularly
- Limit history (e.g., activity log entries)

### Text Layout
- Cache shaped text when possible
- Use monospace optimizations for code
- Avoid re-parsing markdown unnecessarily

## Integration with Terminal

### Event Flow
1. Sidebar checks events first (can consume)
2. Terminal processes remaining events
3. Render called if state changed

### Coordinate Systems
- Sidebar uses window coordinates
- Must transform for terminal coordinates
- Account for DPI scaling

### Focus Management
- Sidebars can capture focus
- Must explicitly release to terminal
- Handle tab key for navigation

## Testing Patterns

### Visual Testing
- Multiple DPI settings
- Various terminal sizes
- Animation edge cases
- Theme variations

### Interaction Testing
- Rapid open/close cycles
- Concurrent animations
- Window resize during animation
- Modal stacking scenarios

### Performance Testing
- Large activity logs
- Complex markdown content
- Many simultaneous animations
- Memory leak detection

## Known Issues and Behaviors

### Scroll Wheel Event Skipping
When scrolling at medium speed, some scroll events appear to be "skipped" - the content doesn't move despite scroll wheel input. Investigation shows:
- Events ARE processed correctly when received (offset increases properly)
- The issue is that no events are received during medium-speed scrolling
- This appears to be OS-level event coalescing that occurs when events arrive faster than the render loop processes them
- Slow scrolling: Each event is processed individually
- Fast scrolling: OS sends many events, all get processed
- Medium scrolling: OS coalesces/drops some events to prevent queue overflow

**Testing logs showed**:
```
11:02:46.547  Scroll wheel: old_offset=4100, new_offset=4120
11:02:46.712  Scroll wheel: old_offset=4120, new_offset=4140
11:02:47.604  Scroll wheel: old_offset=4140, new_offset=4160
```
Events arrive when they arrive, but gaps in timestamps show when events were dropped.

This is expected behavior and not a bug in our code.