# Modal Overlay Framework Implementation Plan

## Overview

This document outlines the implementation plan for a modal overlay system in the CLiBuddy Terminal sidebar. The initial use case focuses on expanding suggestion cards that contain more than 3 lines of content.

## Architecture

### Core Components

#### 1. Modal Manager (`sidebar/components/modal/mod.rs`)
```rust
pub struct ModalManager {
    active_modal: Option<Modal>,
    dimmer_opacity: f32,  // Simplified from ColorEase
    animation_start: Option<std::time::Instant>,
    scroll_offset: f32,
    content_height: f32,
    visible_height: f32,
}

pub struct Modal {
    pub id: String,
    pub size: ModalSize,
    pub content: Box<dyn ModalContent>,
    pub animation_state: ModalAnimationState,
    pub close_on_click_outside: bool,
    pub close_on_escape: bool,
    pub position: Option<RectF>,  // Added for event handling
}

pub enum ModalSize {
    FillSidebar,        // Fill the entire sidebar
    HalfWindow,         // Fill half the app window (extends beyond sidebar)
    Fixed(f32, f32),    // Fixed width and height
}
```

#### 2. Modal Content Trait (`sidebar/components/modal/content.rs`)
```rust
pub trait ModalContent: Send + Sync {
    fn render(&self, context: &ModalRenderContext) -> Element;
    fn handle_event(&mut self, event: &ModalEvent) -> ModalEventResult;
    fn get_content_height(&self) -> f32; // For scroll calculations
}

pub struct ModalRenderContext<'a> {
    pub modal_bounds: RectF,
    pub fonts: &'a SidebarFonts,
    pub visible_height: f32, // Available height for content
}
```

#### 3. Suggestion Expansion Modal (`sidebar/components/modal/suggestion_modal.rs`)
```rust
pub struct SuggestionModal {
    pub suggestion: CurrentSuggestion,
    pub markdown_renderer: MarkdownRenderer,
}

impl ModalContent for SuggestionModal {
    fn render(&self, context: &ModalRenderContext) -> Element {
        // Render full suggestion with markdown
        // Include action buttons (Run, Dismiss)
        // Support scrolling for long content
    }
}
```

### Integration with Suggestion Cards

#### 1. Detecting Need for "More..." Link
```rust
// In render_current_suggestion()
fn should_show_more_link(content: &str, available_height: f32) -> bool {
    // Calculate rendered height using font metrics
    // Return true if content would exceed set number of lines (3 to start)
}
```

#### 2. Rendering "More..." Link
```rust
// Add to suggestion card rendering
if should_show_more_link(&suggestion.content, available_height) {
    // Truncate content to set number of lines (3 to start)
    // Add clickable "more..." element styled as hyperlink
}
```

#### 3. Click Handling
```rust
// Track "more..." link bounds similar to filter chips
pub struct MoreLinkBounds {
    pub suggestion_id: String,
    pub bounds: RectF,
}

// In mouse event handling
if clicked_more_link {
    sidebar.show_suggestion_modal(suggestion);
}
```

## Implementation Progress

### Phase 1: Core Modal Infrastructure ✅ COMPLETED

**Status**: Core infrastructure fully implemented and compiling successfully

**What's Done**:
- ✅ Created modal module structure (all files created)
- ✅ Modal manager with basic state management (simplified from ColorEase to direct opacity)
- ✅ Modal content trait with render context
- ✅ Basic modal rendering pipeline integrated
- ✅ Integration with AI sidebar (modal_manager field, show_suggestion_modal method)
- ✅ Mouse and keyboard event handling hooks
- ✅ Rendering at correct z-indices (20 for dimmer, 21 for container, 24 for scrollbar)
- ✅ All compilation errors resolved

**Key Implementation Details**:
1. **Simplified Animation**: Instead of using ColorEase directly, using simple opacity values
2. **Import Fixes Applied**: 
   - Use `crate::color::LinearRgba` instead of `window::LinearRgba`
   - Use `termwiz::input::KeyCode` instead of `wezterm_term::KeyCode`
   - Use `wezterm_term::KeyModifiers` (correct)
3. **RectF Construction Fixed**: 
   ```rust
   // Use euclid::rect() directly for RectF construction
   euclid::rect(x, y, width, height)
   ```
4. **MouseEvent Types**: Using `window::MouseEvent` with proper pattern matching on `event.kind`
5. **Element Construction**: Use `element.zindex()` method, not `with_zindex()`
6. **Dimension::Pixels**: Requires f32, not f64 - use `as f32` casting
7. **MarkdownRenderer**: Removed instance storage, use static methods directly
8. **Borrow Checker**: Resolved mutable borrow conflicts by extracting values before operations

**Compilation Issues Resolved**:
- ✅ Fixed all RectF construction to use euclid::rect()
- ✅ Fixed MouseEvent handling to use window::MouseEvent with proper pattern matching
- ✅ Fixed MarkdownRenderer usage (don't store instance)
- ✅ Fixed LinearRgba import path
- ✅ Fixed Dimension::Pixels to use f32
- ✅ Resolved borrow checker issues in render method

## Implementation Phases

### Phase 1: Core Modal Infrastructure ✅ COMPLETED

1. **Create modal module structure**
   - `sidebar/components/modal/mod.rs` - Modal manager
   - `sidebar/components/modal/content.rs` - Content trait
   - `sidebar/components/modal/animation.rs` - Animation handling

2. **Implement basic modal rendering**
   - Dimmer background at z-index 20
   - Modal container at z-index 21
   - Drop shadow effect using box model
   - Close button in top-right corner

3. **Add modal size calculations**
   - FillSidebar: Use sidebar width and height
   - HalfWindow: Calculate from window dimensions
   - Fixed: Use provided dimensions

4. **Integrate with sidebar rendering**
   - Add modal_manager to AiSidebar
   - Render modals after main content
   - Handle z-index layering

### Important Integration Notes

**Modal Rendering Integration** (`termwindow/render/sidebar_render.rs`):
- Added `render_sidebar_modals()` method called after scrollbar rendering
- Modals render as separate elements at their designated z-indices
- Each modal element is computed and rendered independently

**Event Handling**:
- Modal manager checks events before sidebar handles them
- Mouse events need type conversion between `window::MouseEvent` and `wezterm_term::MouseEvent`
- Keyboard events use `wezterm_term::KeyModifiers` (not termwiz)

**Key Files Created/Modified**:
1. `sidebar/components/modal/mod.rs` - Main modal manager
2. `sidebar/components/modal/content.rs` - Content trait
3. `sidebar/components/modal/animation.rs` - Simple animation helpers
4. `sidebar/components/modal/suggestion_modal.rs` - Placeholder for suggestion modal
5. `sidebar/ai_sidebar.rs` - Added modal_manager field and methods
6. `termwindow/render/sidebar_render.rs` - Added render_sidebar_modals()

### Phase 2: Scrolling Support ✅ COMPLETED

**Status**: Full scrolling support implemented with interactive scrollbar

**What's Done**:
1. **Enhanced ModalManager state**:
   - Added scrollbar interaction tracking (hovering, dragging)
   - Added drag state management for scrollbar thumb
   - Scroll offset reset on modal show

2. **Implemented scrollable content**:
   - ✅ Track scroll position in modal manager
   - ✅ Render scrollbar when content exceeds visible height
   - ✅ Handle mouse wheel events with proper bounds checking
   - ✅ Pass scroll offset to modal content via ModalRenderContext

3. **Content height calculation**:
   - ✅ Calculate visible height from modal bounds
   - ✅ Track content height from modal content
   - ✅ Dynamic scrollbar thumb sizing based on content ratio
   - ✅ Proper clamping of scroll offset

4. **Interactive scrollbar**:
   - ✅ Visual feedback on hover (opacity changes)
   - ✅ Draggable scrollbar thumb
   - ✅ Click on track to jump (implicit via drag)
   - ✅ Smooth scrolling with configurable speed

**Key Implementation Details**:
- Scrollbar rendered using Element composition with margins for positioning
- Mouse event handling includes drag tracking for scrollbar
- Scroll offset passed to content renderer for proper viewport clipping
- Visual states: normal (0.6 opacity), hover (0.8), dragging (0.9)

### Phase 3: Suggestion Modal Implementation ✅ COMPLETED

**Status**: Full suggestion modal implementation with "more..." link detection

**What's Done**:

1. **Created SuggestionModal**:
   - ✅ Full markdown rendering with code font support
   - ✅ Action buttons (Run, Dismiss) with proper styling
   - ✅ Proper spacing and padding
   - ✅ Content height calculation for scrolling
   - ✅ Thread-safe mutable state using Mutex

2. **Implemented "more..." detection**:
   - ✅ Character-based truncation (3 lines * 80 chars estimate)
   - ✅ Truncate content leaving room for "... more" text
   - ✅ Track whether suggestion needs expansion

3. **Added "more..." link rendering**:
   - ✅ Styled with blue color (0.3, 0.5, 1.0)
   - ✅ Hover effect with lighter blue
   - ✅ Positioned inline after truncated content
   - ✅ Simple click area detection (rough approximation)

4. **Wired up click handling**:
   - ✅ Mouse event handling in AI sidebar
   - ✅ Detect clicks in suggestion area
   - ✅ Create and show suggestion modal on click
   - ✅ Pass through complete suggestion data

**Key Implementation Details**:
- Suggestion truncation at ~240 characters (3 lines * 80 chars)
- "more..." link uses hover colors for visual feedback
- Modal shows full content with markdown formatting
- Button actions log to console (ready for backend integration)
- Rough click detection based on Y coordinate (production would track exact bounds)

**Implementation Notes**:
- Suggestion cards fixed at 200px height (not 3 lines)
- All buttons now use UIItemType pattern for accurate click detection
- Run/Dismiss buttons successfully render in both card and modal
- Modal button container fixed by using DisplayType::Block
- Text wrapping works correctly with max_width constraints
- Filter chips use UIItemType pattern (documented in CLAUDE.md)
- Run/Dismiss buttons currently just close the modal (ready for backend)

### Phase 4: Event Handling & Polish (Days 6-7)

1. **Keyboard support**
   - Escape to close
   - Tab navigation for buttons
   - Scroll with arrow keys

2. **Mouse interactions**
   - Click outside to close
   - Hover effects on buttons
   - Proper cursor changes

3. **Animations**
   - Fade in/out for dimmer
   - Slide or scale for modal
   - Use ColorEase for timing

4. **Edge cases**
   - Handle sidebar resize
   - Window resize adjustments
   - Multiple rapid opens/closes

## Rendering Details

### Z-Index Assignments
- **Dimmer**: z-index 20 (semi-transparent background)
- **Modal Container**: z-index 22 (main modal content)
- **Modal Scrollbar**: z-index 24

### Modal Sizing

#### FillSidebar Mode
- Width: Sidebar width - 32px padding
- Height: Sidebar height - 80px (leave space for header/margins)
- Position: Centered in sidebar

#### HalfWindow Mode
- Width: Window width / 2
- Height: Window height - 160px
- Position: Right-aligned in window (to cover right sidebar)

#### Fixed Mode
- Width/Height: As specified
- Position: Centered in sidebar or window (based on size)

### Visual Design
- **Background**: Dark with 0.7 opacity dimmer
- **Modal**: Rounded corners (8px radius when supported)
- **Shadow**: 0 4px 20px rgba(0,0,0,0.3)
- **Header**: Optional title bar with close button
- **Padding**: 20px internal padding
- **Scrollbar**: Auto-hide, 12px wide

## Event Flow

1. **Opening Modal**
   - User clicks "more..." link
   - Create modal with suggestion content
   - Start 20ms fade-in animation
   - Capture mouse/keyboard focus

2. **While Open**
   - Route events to modal first
   - Handle scrolling within modal
   - Update hover states
   - Process button clicks

3. **Closing Modal**
   - Click close button, outside, or Escape
   - Start 20ms fade-out animation
   - Return focus to sidebar
   - Clean up modal state

## Performance Considerations

1. **Rendering Optimization**
   - Only render visible portion of scrollable content (use "hole cutting" method like activity log)
   - Cache markdown rendering results
   - Reuse elements where possible
   - Dirty tracking for updates

2. **Memory Management**
   - Clean up modal state on close
   - Don't retain large content strings
   - Proper event handler cleanup

3. **Animation Performance**
   - Use GPU-accelerated properties
   - Avoid layout recalculation during animation
   - Target 60fps for smooth experience

## Testing Strategy

1. **Unit Tests**
   - Modal size calculations
   - Content height detection
   - Event routing logic
   - Animation state transitions

2. **Integration Tests**
   - Full modal lifecycle
   - Suggestion card integration
   - Scroll handling
   - Keyboard navigation

3. **Manual Testing**
   - Various content lengths
   - Different modal sizes
   - Animation smoothness
   - Edge cases (rapid open/close, resize)

## Future Extensions

Once the core modal system is working for suggestions:

1. **Additional Modal Types**
   - Confirmation dialogs
   - Error messages
   - Settings panels
   - File pickers

2. **Accessibility**
   - Screen reader support
   - Focus trapping
   - Keyboard-only navigation

## Current Implementation Status

### Completed Features (Phases 1-3)

The modal overlay framework is now **fully functional** with the following capabilities:

1. **Core Infrastructure** ✅
   - Modal manager integrated into AI sidebar
   - Dimmer background with configurable opacity
   - Animated show/hide transitions
   - Proper z-index layering (20-24)

2. **Scrolling Support** ✅
   - Interactive scrollbar with drag support
   - Mouse wheel scrolling
   - Visual feedback on hover/drag
   - Scroll offset passed to content renderers

3. **Suggestion Modal** ✅
   - Detects when content exceeds 3 lines
   - Shows "more..." link with hover effect
   - Opens modal with full markdown content
   - Action buttons (Run/Dismiss)
   - Scrollable for long content

### Ready for Backend Integration

The modal system is fully functional with all UI elements working correctly.

**Completed ✅:**
- Modal displays content with proper scrolling
- All buttons (Show more, Run, Dismiss) use UIItemType pattern
- Buttons have accurate click detection without manual bounds tracking
- Text wrapping works properly within modal bounds
- Modal can be closed via X button, clicking outside, or Escape key

**Remaining Tasks:**
- Wire up Run/Dismiss button actions to actual functionality
- Add keyboard navigation (Tab between buttons, Enter to activate)
- Add smooth animations (fade/scale effects)
- Consider adding loading states for Run action
- Add error handling for failed actions

### Troubleshooting Session Fixes

**Issue 1: "more..." link not visible**
- Changed from markdown rendering for truncated content to plain text
- Reduced CHARS_PER_LINE from 80 to 60 for more conservative truncation
- Used simple text elements instead of markdown for truncated content
- "more" link is now rendered as inline element after truncated text

**Issue 2: Panic when clicking suggestion card**
- Fixed SizedPoly::none() usage that caused panic in customglyph.rs
- Changed all Poly elements to use ElementContent::Text(String::new()) instead
- Added proper display(DisplayType::Block) to all background elements
- Added proper margins for positioning modal elements

**Current Implementation Status**:
- Truncation now works at 180 characters (3 lines * 60 chars)
- "more..." link is rendered inline after truncated text
- Click detection covers entire suggestion card area when truncated
- Modal uses proper rectangular elements instead of polygons

## Success Criteria

1. **Functionality**
   - Suggestions > 3 lines show "more..." link
   - Modal displays full content with scrolling
   - Markdown renders correctly
   - Action buttons work in modal

2. **Performance**
   - Modal open/close < 16ms
   - No memory leaks
   - Efficient rendering

3. **User Experience**
   - Intuitive interaction
   - Smooth animations
   - Clear visual hierarchy
   - Easy to dismiss

4. **Code Quality**
   - Reusable modal system
   - Clean separation of concerns
   - Well-documented API
   - Follows existing patterns

**Current Status**: Phases 1-3 complete, modal system fully functional. All UI elements working correctly with proper click detection via UIItemType pattern.

## Next Steps for Discussion

### 1. Button Action Implementation
The Run/Dismiss buttons currently just close the modal. We need to define:
- **Run button**: What command should be executed? How to show progress/output?
- **Dismiss button**: Should it remove the suggestion from the list? Just close modal?

### 2. Keyboard Navigation (Phase 4)
- Tab key to navigate between buttons
- Enter/Space to activate focused button
- Arrow keys for scrolling content
- Visual focus indicators

### 3. Visual Polish
- Smooth fade/scale animations for modal appearance
- Loading spinner or progress indicator for Run action
- Success/error states after action completion
- Button hover effects (currently using chip hover colors)

### 4. Edge Cases
- What happens if Run fails?
- Should modal stay open after Run succeeds?
- How to handle very long-running commands?
- Should Dismiss remove suggestion permanently or just for session?