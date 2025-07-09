# Implementation Plan: Multi-line Chat Input & Text Selection

## Goals
- Implement precise focus management. Focus defaults to terminal area and only moves to input components (chat input) when clicked. During text selection (drag), focus temporarily follows the selection but returns if no selection is made. Modals steal focus and return it when closed.
- Implement text selection for copy/paste activity. Plain text selection within individual components (activity items, suggestion cards, goal text, chat input). Text and background should change colors when selected.
- Get Multi-line Chat Input Functionality working with full editing capabilities
    - Click-to-position cursor
    - Text selection within input
    - Enter to send, Shift+Enter for newline
    - Proper focus management

## Current Status (Updated)

### Completed (Phases 0-2)
1. ✅ **MultilineTextInput Foundation** - All core methods implemented with proper UTF-8 handling
2. ✅ **Focus Infrastructure** - Extended sidebar focus system, keyboard routing works
3. ✅ **Chat Input Focus** - Click-to-focus works, Enter sends, Shift+Enter newlines, Escape unfocuses

### Completed (Phase 3)
1. **Phase 3: Text Selection in Sidebar** - COMPLETED
   - ✅ SelectionState and SelectionTarget types implemented
   - ✅ Selection rendering with StyleSpan (blue background)
   - ✅ Character-level hit testing during rendering phase
   - ✅ Pre-calculated character positions stored in UIItemType
   - ✅ Mouse event handlers for all text types
   - ✅ Drag-to-extend selection working properly
   - ✅ UTF-8 safe byte offset tracking

### Completed (Phase 4)
1. **Phase 4: Clipboard Integration** - COMPLETED
   - ✅ Ctrl/Cmd+C keyboard shortcut intercepted
   - ✅ Works with both focused chat input selections
   - ✅ Works with sidebar text selections (even without focus)
   - ✅ Platform-specific modifier handling (Cmd on macOS, Ctrl elsewhere)

## Key Implementation Details

### Focus Management  
- ✅ IMPLEMENTED: Renamed `has_modal_focus()` to `has_keyboard_focus()` for clarity
- ✅ NO DEPRECATED ALIAS: Direct rename was done, all usages updated
- ✅ Extended to include `chat_input.focused` check  
- ✅ Keyboard events automatically route to chat input when focused

### Selection Rendering
- StyleSpan with ElementColors.bg works for selection backgrounds
- ⚠️ ISSUE: ElementColors uses InheritableColor enum, not Option
- ⚠️ ISSUE: ElementColors::new() doesn't exist, use ::default()
- Use blue background (0.3, 0.5, 0.8, 0.8) and white text for selections
- Must convert character indices to byte offsets for UTF-8 safety

### Text Hit Testing
- Use `font.metrics().cell_width.get()` for character width approximation
- Proper text shaping would be better but this works for MVP
- Handle UTF-8 properly when converting positions to byte offsets

## Phase 1: Focus Infrastructure

### 1.1 Extended Sidebar Focus System
Build on the existing focus system but rename for clarity:

```rust
// In ai_sidebar.rs - rename and extend focus tracking
impl AiSidebar {
    // Rename has_modal_focus() to has_keyboard_focus() for clarity
    pub fn has_keyboard_focus(&self) -> bool {
        self.modal_manager.is_active() || self.chat_input.focused
    }
    
    // Keep the old name as deprecated alias during transition
    #[deprecated(note = "Use has_keyboard_focus() instead")]
    pub fn has_modal_focus(&self) -> bool {
        self.has_keyboard_focus()
    }
    
    // More specific focus queries
    pub fn has_input_focus(&self) -> bool {
        self.chat_input.focused
    }
    
    pub fn has_modal_active(&self) -> bool {
        self.modal_manager.is_active()
    }
    
    // For restoring focus after modal
    pub fn restore_focus(&mut self) {
        if self.had_chat_focus_before_modal {
            self.chat_input.focused = true;
        }
    }
}

// Update keyevent.rs to use the new method name
// Replace sidebar.has_modal_focus() with sidebar.has_keyboard_focus()
```

Focus rules remain the same:
1. Default to terminal area
2. Only move focus when clicking on input components
3. Text selection doesn't change focus
4. Modals steal focus and return it when closed
5. Keyboard events route based on `has_keyboard_focus()`

### 1.2 Add UIItemType Variants
```rust
// In termwindow/mod.rs
UIItemType::ChatInput,
UIItemType::ActivityItemText { index: usize },
UIItemType::SuggestionText,
UIItemType::GoalText,
// For future extensibility of selectable text areas
```

### 1.3 Update AI Sidebar State
```rust
// In ai_sidebar.rs
pub struct AiSidebar {
    // ... existing fields ...
    selection_state: SelectionState,
    
    // Track bounds for hit testing
    activity_item_bounds: HashMap<usize, euclid::default::Rect<f32>>,
    suggestion_bounds: Option<euclid::default::Rect<f32>>,
    goal_bounds: Option<euclid::default::Rect<f32>>,
}
```

### 1.4 Update Keyboard Event Routing
The existing keyboard event routing needs to be updated to use the new method name:
```rust
// In keyevent.rs - update method calls
if let Some(sidebar) = &mut self.left_sidebar {
    if sidebar.has_keyboard_focus() {  // Changed from has_modal_focus()
        if sidebar.handle_key_event(&key.key).unwrap_or(false) {
            return true;
        }
    }
}
// Same for right_sidebar...
```

Since we're extending the focus check to include chat input focus, keyboard routing will automatically work for both modals and chat input!

## Phase 2: Multi-line Chat Input Full Functionality

### 2.1 Update Chat Input Rendering
```rust
// In render_chat_input()
let input_field = self.chat_input.render(&fonts.body)
    .with_item_type(UIItemType::ChatInput);
```

### 2.2 Handle ChatInput Click in mouseevent.rs
```rust
// In mouseevent.rs - add to UIItemType match
UIItemType::ChatInput => {
    if let Some(tw) = context.window.as_any().downcast_ref::<TermWindow>() {
        // Get the actual bounds used during rendering
        let bounds = /* retrieve from context or stored location */;
        
        tw.with_right_sidebar_mut(|sidebar| {
            if let Some(ai_sidebar) = sidebar.as_any_mut().downcast_mut::<AiSidebar>() {
                ai_sidebar.chat_input.focused = true;
                
                // Calculate relative position within the input field
                let relative_pos = euclid::point2(
                    coords.x - bounds.origin.x,
                    coords.y - bounds.origin.y,
                );
                
                // TODO: Implement handle_click after adding it to MultilineTextInput
                // ai_sidebar.chat_input.handle_click(relative_pos);
            }
        });
    }
}
```

### 2.3 First Implement Missing MultilineTextInput Methods
```rust
// In MultilineTextInput - these methods need to be implemented first!
impl MultilineTextInput {
    // Use proper font metrics instead of character width estimation
    pub fn handle_click(&mut self, relative_pos: Point2D<f32>, font: &FontConfigPtr) {
        // Determine which display line was clicked
        let line_height = font.get_line_height(); // Get actual line height
        let display_line = (relative_pos.y / line_height) as usize;
        
        if display_line < self.display_lines {
            let actual_line = self.first_visible_line + display_line;
            if actual_line < self.lines.len() {
                // Get the actual text and use font metrics for hit testing
                let text = &self.lines[actual_line];
                // TODO: Use proper text shaping for hit testing
                // let shaped = font.shape_text(text);
                // let hit = shaped.hit_test(relative_pos.x);
                // self.cursor_column = hit.char_index;
                
                self.cursor_line = actual_line;
                self.selection_start = None;
            }
        }
    }
    
    pub fn start_selection(&mut self) {
        self.selection_start = Some((self.cursor_line, self.cursor_column));
    }
    
    pub fn update_selection(&mut self, relative_pos: Point2D<f32>, font: &FontConfigPtr) {
        // Similar to handle_click but preserves selection_start
        self.handle_click(relative_pos, font);
    }
    
    pub fn get_selected_text(&self) -> Option<String> {
        let (start_line, start_col) = self.selection_start?;
        let (end_line, end_col) = (self.cursor_line, self.cursor_column);
        
        // Handle single-line selection
        if start_line == end_line {
            let line = &self.lines[start_line];
            let start = start_col.min(end_col);
            let end = start_col.max(end_col);
            
            // Convert character indices to byte offsets
            let start_byte = line.chars().take(start).map(|c| c.len_utf8()).sum();
            let end_byte = line.chars().take(end).map(|c| c.len_utf8()).sum();
            
            return Some(line[start_byte..end_byte].to_string());
        }
        
        // Handle multi-line selection
        // TODO: Implement multi-line selection
        None
    }
}
```

### 2.4 Update Keyboard Event Handling
```rust
// In ai_sidebar.rs - modify handle_key_event()
fn handle_key_event(&mut self, key: &KeyCode, modifiers: Modifiers) -> Result<bool> {
    // Always check chat input first when it has focus
    if self.chat_input.focused {
        match (key, modifiers) {
            (KeyCode::Escape, _) => {
                self.chat_input.focused = false;
                // Return focus to terminal
                Ok(true)
            }
            (KeyCode::Enter, modifiers) if !modifiers.contains(Modifiers::SHIFT) => {
                if !self.chat_input.get_text().trim().is_empty() {
                    self.handle_chat_send();
                }
                Ok(true)
            }
            (KeyCode::Enter, modifiers) if modifiers.contains(Modifiers::SHIFT) => {
                // Insert newline
                self.chat_input.insert_newline();
                Ok(true)
            }
            _ => {
                // Forward all other keys to MultilineTextInput
                self.chat_input.handle_key_event(key, modifiers)
            }
        }
    } else if self.modal_manager.is_active() {
        // ... existing modal handling ...
    } else {
        Ok(false)
    }
}
```

## Phase 3: Text Selection Implementation

### 3.1 Unified Selection State
```rust
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    active_selection: Option<SelectionTarget>,
    is_dragging: bool,
}

#[derive(Debug, Clone)]
pub enum SelectionTarget {
    ActivityItem {
        index: usize,
        anchor_byte: usize,
        current_byte: usize,
    },
    Suggestion {
        anchor_byte: usize,
        current_byte: usize,
    },
    Goal {
        anchor_byte: usize,
        current_byte: usize,
    },
    // ChatInput selection handled internally by MultilineTextInput
}

impl SelectionState {
    pub fn get_selected_text(&self, sidebar: &AiSidebar) -> Option<String> {
        match &self.active_selection? {
            SelectionTarget::ActivityItem { index, anchor_byte, current_byte } => {
                let text = get_item_text(&sidebar.activity_log[*index]);
                let start = anchor_byte.min(current_byte);
                let end = anchor_byte.max(current_byte);
                Some(text[*start..*end].to_string())
            }
            SelectionTarget::Suggestion { anchor_byte, current_byte } => {
                if let Some(suggestion) = &sidebar.current_suggestion {
                    let start = anchor_byte.min(current_byte);
                    let end = anchor_byte.max(current_byte);
                    Some(suggestion.content[*start..*end].to_string())
                }
            }
            SelectionTarget::Goal { anchor_byte, current_byte } => {
                if let Some(goal) = &sidebar.current_goal {
                    let start = anchor_byte.min(current_byte);
                    let end = anchor_byte.max(current_byte);
                    Some(goal.text[*start..*end].to_string())
                }
            }
        }
    }
    
    pub fn clear(&mut self) {
        self.active_selection = None;
        self.is_dragging = false;
    }
}
```

### 3.2 Activity Item Text Extraction
```rust
fn get_item_text(item: &ActivityItem) -> &str {
    match item {
        ActivityItem::Chat { message, .. } => message,
        ActivityItem::Command { command, output, .. } => {
            output.as_deref().unwrap_or(command)
        }
        ActivityItem::Suggestion { content, .. } => content,
        ActivityItem::Goal { text, .. } => text,
    }
}
```

### 3.3 Proper Text Hit Testing
```rust
// NOTE: This is a simplified version. Real implementation needs font shaping
fn estimate_text_position(&self, item_bounds: &Rect, click_x: f32, text: &str, font: &FontConfigPtr) -> usize {
    let relative_x = (click_x - item_bounds.origin.x).max(0.0);
    
    // For MVP, use a simple approach with character iteration
    // TODO: Use proper text shaping when available
    let mut accumulated_width = 0.0;
    let mut byte_offset = 0;
    
    // Rough estimate using average character width
    let avg_char_width = font.get_metrics().average_advance_width;
    
    for ch in text.chars() {
        let char_width = if ch.is_ascii() {
            avg_char_width
        } else {
            avg_char_width * 1.5 // Rough estimate for non-ASCII
        };
        
        if accumulated_width + char_width / 2.0 > relative_x {
            break;
        }
        
        accumulated_width += char_width;
        byte_offset += ch.len_utf8();
    }
    
    byte_offset
}
```

### 3.4 Selection Rendering with StyleSpan
StyleSpan supports background colors, so we can use it directly for selection rendering:

```rust
// Create style spans for text with selection
fn create_selection_spans(text: &str, start_byte: usize, end_byte: usize) -> Vec<StyleSpan> {
    let mut spans = vec![];
    
    // Text before selection (if any)
    if start_byte > 0 {
        spans.push(StyleSpan {
            start: 0,
            end: start_byte,
            colors: ElementColors::default(), // Will inherit from element
            font: None,
            font_style: None,
        });
    }
    
    // Selected text with blue background
    spans.push(StyleSpan {
        start: start_byte,
        end: end_byte,
        colors: ElementColors {
            bg: LinearRgba::with_components(0.3, 0.5, 0.8, 0.8).into(), // Blue selection
            text: LinearRgba::with_components(1.0, 1.0, 1.0, 1.0).into(), // White text
            ..Default::default()
        },
        font: None,
        font_style: None,
    });
    
    // Text after selection (if any)
    if end_byte < text.len() {
        spans.push(StyleSpan {
            start: end_byte,
            end: text.len(),
            colors: ElementColors::default(), // Will inherit from element
            font: None,
            font_style: None,
        });
    }
    
    spans
}

// Render text with selection using StyledWrappedText
fn render_text_with_selection(&self, text: &str, selection: Option<(usize, usize)>, font: &FontConfigPtr) -> Element {
    if let Some((start, end)) = selection {
        let spans = create_selection_spans(text, start, end);
        Element::new(font, ElementContent::StyledWrappedText { 
            text: text.to_string(), 
            style_spans: spans 
        })
    } else {
        // No selection - normal rendering
        Element::new(font, ElementContent::WrappedText(text.to_string()))
    }
}

// For MultilineTextInput - modify the render method to include selection
impl MultilineTextInput {
    pub fn render_with_selection(&self, font: &FontConfigPtr) -> Element {
        let mut all_lines = vec![];
        
        // Process each visible line
        for (line_idx, line_text) in self.visible_lines().enumerate() {
            let actual_line_idx = self.first_visible_line + line_idx;
            
            // Check if this line has any selection
            if let Some((start_line, start_col)) = self.selection_start {
                let (end_line, end_col) = (self.cursor_line, self.cursor_column);
                
                if actual_line_idx >= start_line.min(end_line) && actual_line_idx <= start_line.max(end_line) {
                    // This line has selection
                    let (sel_start_col, sel_end_col) = if actual_line_idx == start_line && actual_line_idx == end_line {
                        // Selection within single line
                        (start_col.min(end_col), start_col.max(end_col))
                    } else if actual_line_idx == start_line {
                        // Selection starts on this line
                        (start_col, line_text.len())
                    } else if actual_line_idx == end_line {
                        // Selection ends on this line
                        (0, end_col)
                    } else {
                        // Entire line is selected
                        (0, line_text.len())
                    };
                    
                    // Convert column positions to byte offsets
                    let start_byte = line_text.chars().take(sel_start_col).map(|c| c.len_utf8()).sum();
                    let end_byte = line_text.chars().take(sel_end_col).map(|c| c.len_utf8()).sum();
                    
                    // Create styled text with selection
                    let spans = create_selection_spans(line_text, start_byte, end_byte);
                    all_lines.push(Element::new(font, ElementContent::StyledWrappedText { 
                        text: line_text.to_string(), 
                        style_spans: spans 
                    }));
                } else {
                    // No selection on this line
                    all_lines.push(Element::new(font, ElementContent::Text(line_text.to_string())));
                }
            } else {
                // No selection at all
                all_lines.push(Element::new(font, ElementContent::Text(line_text.to_string())));
            }
        }
        
        // Combine all lines into a single element
        Element::new(font, ElementContent::Children(all_lines))
    }
}
```

### 3.5 Selection Rendering for Different Components

```rust
// In render_activity_log_item() - for plain text selection
let selection = match &self.selection_state.active_selection {
    Some(SelectionTarget::ActivityItem { index, anchor_byte, current_byte }) 
        if *index == item_index => {
        Some((*anchor_byte.min(current_byte), *anchor_byte.max(current_byte)))
    }
    _ => None,
};

// For AI messages with markdown, we need to extract plain text for selection
if selection.is_some() {
    // Extract plain text from markdown (strip formatting)
    let plain_text = extract_plain_text_from_markdown(message);
    self.render_text_with_selection(&plain_text, selection, font)
} else {
    // Normal markdown rendering
    markdown_renderer.render(message)
}

// In render_suggestion() 
let selection = match &self.selection_state.active_selection {
    Some(SelectionTarget::Suggestion { anchor_byte, current_byte }) => {
        Some((*anchor_byte.min(current_byte), *anchor_byte.max(current_byte)))
    }
    _ => None,
};
self.render_with_selection(&suggestion.content, selection)
    .with_item_type(UIItemType::SuggestionText)

// Similar for goal rendering
```

## Implementation Challenges & Solutions

### Font Access in Mouse Handlers
- **PROBLEM**: Mouse event handlers don't have direct access to FontConfiguration
- **ROOT CAUSE**: WezTerm's two-phase rendering (element processing then GPU drawing) separates font access from event handling
- **SOLUTION**: Pre-calculate character positions during rendering phase when fonts are available
  - Extended UIItemType to include `char_positions: Vec<(f32, f32, usize)>`
  - Calculate positions in `calculate_char_positions()` during element creation
  - Mouse handlers use pre-calculated data for hit testing

### Mouse Event Types
- **ISSUE**: No `WMEK::Drag` variant in MouseEventKind
- **SOLUTION**: Use `WMEK::Move` for drag tracking
- **IMPLEMENTATION**: Check `is_selecting()` state during Move events

### Type System Constraints
- **ISSUE**: f32 doesn't implement Eq trait needed for #[derive(Eq)]
- **SOLUTION**: Removed Eq from UIItemType and UIItem derives, kept PartialEq

### Selection Rendering
- **CONFIRMED**: StyleSpan DOES support background colors via ElementColors
- **IMPLEMENTATION**: Use InheritableColor::Color() for selection highlighting

### 3.6 Mouse Selection Handling (No Focus Change)
```rust
// In mouseevent.rs - handle UIItemType clicks for selection start
// IMPORTANT: Selection doesn't change focus - focus stays where it was
UIItemType::ActivityItemText { index } => {
    if let Some(tw) = context.window.as_any().downcast_ref::<TermWindow>() {
        tw.with_right_sidebar_mut(|sidebar| {
            if let Some(ai_sidebar) = sidebar.as_any_mut().downcast_mut::<AiSidebar>() {
                let relative_x = /* calculate relative x within item */;
                let text = get_item_text(&ai_sidebar.activity_log[index]);
                let byte_offset = ai_sidebar.estimate_text_position(&bounds, relative_x, text);
                
                ai_sidebar.selection_state.active_selection = Some(SelectionTarget::ActivityItem {
                    index,
                    anchor_byte: byte_offset,
                    current_byte: byte_offset,
                });
                ai_sidebar.selection_state.is_dragging = true;
                // NOTE: No focus change here - terminal/chat input keeps focus
            }
        });
    }
}
// Similar handlers for UIItemType::SuggestionText and UIItemType::GoalText

// In handle_mouse_event() - handle dragging
match event.kind {
    MouseEventKind::Drag { .. } if self.selection_state.is_dragging => {
        // Update current position based on what's being selected
        match &mut self.selection_state.active_selection {
            Some(SelectionTarget::ActivityItem { index, current_byte, .. }) => {
                if let Some(bounds) = self.activity_item_bounds.get(index) {
                    if bounds.contains(event.coords.cast()) {
                        let text = get_item_text(&self.activity_log[*index]);
                        *current_byte = self.estimate_text_position(bounds, event.coords.x, text);
                    }
                }
            }
            Some(SelectionTarget::Suggestion { current_byte, .. }) => {
                if let Some(bounds) = &self.suggestion_bounds {
                    if bounds.contains(event.coords.cast()) {
                        if let Some(suggestion) = &self.current_suggestion {
                            *current_byte = self.estimate_text_position(bounds, event.coords.x, &suggestion.content);
                        }
                    }
                }
            }
            // Similar for Goal
            _ => {}
        }
        return Ok(true);
    }
    MouseEventKind::Release(MousePress::Left) => {
        self.selection_state.is_dragging = false;
    }
    _ => {}
}
```

## Phase 4: Clipboard Integration

```rust
// In ai_sidebar.rs - need access to window for clipboard
impl AiSidebar {
    pub fn handle_copy(&mut self, window: &dyn WindowOps) -> bool {
        if self.chat_input.focused {
            // Get selection from MultilineTextInput
            if let Some(text) = self.chat_input.get_selected_text() {
                window.set_clipboard(text);
                return true;
            }
        } else if let Some(text) = self.selection_state.get_selected_text(self) {
            // Copy sidebar selection to clipboard
            window.set_clipboard(text);
            return true;
        }
        false
    }
}

// In handle_key_event() - delegate to handle_copy
(KeyCode::Char('c'), modifiers) if modifiers.contains(Modifiers::CTRL | Modifiers::SUPER) => {
    // Need to get window reference - this might require passing it through
    // or storing it in the sidebar
    Ok(self.handle_copy(window))
}
```

## Implementation Complete

All phases have been successfully implemented:

1. **Phase 0**: MultilineTextInput foundation with UTF-8 safe methods
2. **Phase 1**: Focus infrastructure extension 
3. **Phase 2**: Chat input focus and keyboard handling
4. **Phase 3**: Text selection with character-level hit testing
5. **Phase 4**: Clipboard integration with Ctrl/Cmd+C support

## Phase 3 Implementation Summary

Phase 3 has been completed with the following approach:

1. **Extended UIItemType** with pre-calculated character positions
   - Added `char_positions: Vec<(f32, f32, usize)>` to text-related UIItemType variants
   - Positions calculated during rendering when fonts are available

2. **Character Position Calculation**
   - `calculate_char_positions()` uses font metrics to map text positions
   - Returns Vec of (x_start, x_end, byte_offset) for each character
   - Handles ASCII vs non-ASCII character width estimation

3. **Hit Testing in Mouse Handlers**
   - `find_byte_offset_from_x()` converts x coordinate to byte offset
   - Uses pre-calculated positions from UIItemType
   - No font access needed in event handlers

4. **Selection Methods Updated**
   - Removed all `*_simple()` workaround methods
   - Selection methods now take byte offsets directly
   - Drag updates work with pre-calculated positions

## Phase 4 Implementation Details

Clipboard integration was implemented by:

1. **Intercepting Copy Commands** in keyevent.rs
   - Check for Ctrl+C (or Cmd+C on macOS) before normal key routing
   - Handle copy even when sidebar doesn't have focus (for selections)

2. **Platform-Specific Modifiers**
   - Use `cfg` attributes to check SUPER on macOS, CTRL elsewhere
   - Consistent with platform conventions

3. **Copy Priority**
   - Check chat input selection first (if focused)
   - Then check sidebar selection state
   - Return true if text was copied to prevent default handling

## Known Limitations & Future Work

1. **Click-to-position in chat input** - Currently cursor goes to line start, not click position
2. **Markdown selection** - Currently copies raw markdown, not plain text
3. **Multi-line selection in chat** - Basic implementation, could be improved
4. **Visual feedback** - No visual indication when text is copied
5. **Selection across components** - Can only select within single components

## Testing Strategy

1. **Focus Management**
   - Click on chat input moves focus from terminal
   - Click on non-input text (activity, suggestion) doesn't change focus
   - Selecting text doesn't change focus
   - Keyboard events route to focused component only
   - Modal focus stealing and return works correctly

2. **Chat Input**
   - Click to focus and position cursor
   - Focus persists until clicking elsewhere or pressing Escape
   - Drag to select text within input
   - Enter sends, Shift+Enter adds newline
   - Multi-line editing operations work

3. **Text Selection**
   - Click and drag in each text area type (without changing focus)
   - Single click doesn't start selection or change focus
   - Selection highlighting renders correctly
   - Can select text while terminal or chat input has focus

4. **Copy/Paste**
   - Ctrl/Cmd+C copies selected text without changing focus
   - Works whether focus is on terminal or chat input
   - Selected text from any component can be copied
   - Clipboard contains plain text only

## Current Implementation State Summary

### What's Working
- Focus management correctly routes keyboard input to chat or modals
- Chat input focus/unfocus with proper keyboard handling
- Selection state types and infrastructure in place
- Selection rendering with blue background implemented
- Basic mouse click handlers for starting selection

### What's Not Working Yet
- Text hit testing (currently selects all text on click)
- Drag-to-extend selection (Move events not updating selection)
- Bounds tracking for accurate mouse-to-text position mapping
- Font metrics access in mouse handlers

### Next Steps for Phase 3 Completion
1. Store element bounds during rendering phase (when fonts are available)
2. Implement proper hit testing using stored bounds and font metrics
3. Fix drag handling to update selection during mouse move
4. Test selection rendering with actual text content

### Key Files Modified
- `wezterm-gui/src/sidebar/ai_sidebar.rs` - Added SelectionState, rendering
- `wezterm-gui/src/termwindow/mouseevent.rs` - Added selection mouse handlers
- `wezterm-gui/src/sidebar/mod.rs` - Renamed has_modal_focus to has_keyboard_focus
- `wezterm-gui/src/termwindow/keyevent.rs` - Updated to use has_keyboard_focus