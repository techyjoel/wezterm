//! Form components for sidebar UI
//! Provides text input, buttons, toggles, dropdowns, and other form elements

use crate::color::LinearRgba;
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent,
    InheritableColor, StyleSpan,
};
use config::Dimension;
use euclid::default::{Point2D, Rect};
use std::rc::Rc;
use wezterm_font::LoadedFont;
use wezterm_term::{KeyCode, KeyModifiers};

/// Text input component for forms
#[derive(Debug, Clone)]
pub struct TextInput {
    /// Current text value
    pub value: String,
    /// Placeholder text when empty
    pub placeholder: String,
    /// Whether the input is focused
    pub focused: bool,
    /// Cursor position
    pub cursor_pos: usize,
    /// Selection start (if any)
    pub selection_start: Option<usize>,
    /// Maximum length (None for unlimited)
    pub max_length: Option<usize>,
    /// Whether the input is disabled
    pub disabled: bool,
    /// Validation error message
    pub error: Option<String>,
}

impl TextInput {
    pub fn new() -> Self {
        Self {
            value: String::new(),
            placeholder: String::new(),
            focused: false,
            cursor_pos: 0,
            selection_start: None,
            max_length: None,
            disabled: false,
            error: None,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self.cursor_pos = self.value.len();
        self
    }

    pub fn with_max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_error(mut self, error: Option<String>) -> Self {
        self.error = error;
        self
    }

    /// Handle character input
    pub fn insert_char(&mut self, c: char) {
        if self.disabled {
            return;
        }

        if let Some(max) = self.max_length {
            if self.value.len() >= max {
                return;
            }
        }

        // Clear selection if any
        if let Some(start) = self.selection_start {
            let end = self.cursor_pos;
            let (start, end) = if start < end {
                (start, end)
            } else {
                (end, start)
            };
            self.value.replace_range(start..end, "");
            self.cursor_pos = start;
            self.selection_start = None;
        }

        self.value.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    /// Handle backspace
    pub fn backspace(&mut self) {
        if self.disabled {
            return;
        }

        if let Some(start) = self.selection_start {
            let end = self.cursor_pos;
            let (start, end) = if start < end {
                (start, end)
            } else {
                (end, start)
            };
            self.value.replace_range(start..end, "");
            self.cursor_pos = start;
            self.selection_start = None;
        } else if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
            self.value.remove(self.cursor_pos);
        }
    }

    /// Handle delete key
    pub fn delete(&mut self) {
        if self.disabled {
            return;
        }

        if let Some(start) = self.selection_start {
            let end = self.cursor_pos;
            let (start, end) = if start < end {
                (start, end)
            } else {
                (end, start)
            };
            self.value.replace_range(start..end, "");
            self.cursor_pos = start;
            self.selection_start = None;
        } else if self.cursor_pos < self.value.len() {
            self.value.remove(self.cursor_pos);
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        if self.cursor_pos < self.value.len() {
            self.cursor_pos += 1;
        }
    }

    /// Move cursor to beginning
    pub fn move_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Move cursor to end
    pub fn move_end(&mut self) {
        self.cursor_pos = self.value.len();
    }

    /// Select all text
    pub fn select_all(&mut self) {
        self.selection_start = Some(0);
        self.cursor_pos = self.value.len();
    }

    /// Clear the input
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor_pos = 0;
        self.selection_start = None;
    }

    /// Render as Element
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let display_text = if self.value.is_empty() && !self.focused {
            &self.placeholder
        } else {
            &self.value
        };

        let border_color = if self.error.is_some() {
            LinearRgba::with_components(0.8, 0.2, 0.2, 1.0)
        } else if self.focused {
            LinearRgba::with_components(0.2, 0.5, 0.8, 1.0)
        } else if self.disabled {
            LinearRgba::with_components(0.3, 0.3, 0.3, 1.0)
        } else {
            LinearRgba::with_components(0.4, 0.4, 0.4, 1.0)
        };

        let bg_color = if self.disabled {
            LinearRgba::with_components(0.1, 0.1, 0.1, 1.0)
        } else {
            LinearRgba::with_components(0.05, 0.05, 0.05, 1.0)
        };

        let text_color = if self.disabled {
            LinearRgba::with_components(0.4, 0.4, 0.4, 1.0)
        } else if self.value.is_empty() && !self.focused {
            LinearRgba::with_components(0.5, 0.5, 0.5, 1.0)
        } else {
            LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
        };

        let element = Element::new(font, ElementContent::Text(display_text.to_string()))
            .colors(ElementColors {
                border: BorderColor::new(border_color),
                bg: bg_color.into(),
                text: text_color.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Pixels(8.),
                right: Dimension::Pixels(8.),
                top: Dimension::Pixels(4.),
                bottom: Dimension::Pixels(4.),
            })
            .border(BoxDimension {
                left: Dimension::Pixels(1.),
                right: Dimension::Pixels(1.),
                top: Dimension::Pixels(1.),
                bottom: Dimension::Pixels(1.),
            })
            .display(DisplayType::Block)
            .min_width(Some(Dimension::Pixels(200.)));

        // Add cursor rendering if focused
        if self.focused {
            // TODO: Add cursor rendering overlay
        }

        element
    }
}

/// Multi-line text input component for forms (e.g., chat)
#[derive(Debug, Clone)]
pub struct MultilineTextInput {
    /// Lines of text
    pub lines: Vec<String>,
    /// Placeholder text when empty
    pub placeholder: String,
    /// Whether the input is focused
    pub focused: bool,
    /// Cursor line position
    pub cursor_line: usize,
    /// Cursor column position  
    pub cursor_col: usize,
    /// Selection start (line, col) if any
    pub selection_start: Option<(usize, usize)>,
    /// Maximum number of lines (None for unlimited)
    pub max_lines: Option<usize>,
    /// Whether the input is disabled
    pub disabled: bool,
    /// Height in lines to display
    pub display_lines: usize,
    /// Scroll offset (first visible line)
    pub scroll_offset: usize,
    /// Pixel-based scroll offset for smooth scrolling
    pub scroll_pixel_offset: f32,
    /// Whether user has manually scrolled (disables auto-scroll to bottom)
    pub user_has_scrolled: bool,
}

impl MultilineTextInput {
    pub fn new(display_lines: usize) -> Self {
        Self {
            lines: vec![String::new()],
            placeholder: String::new(),
            focused: false,
            cursor_line: 0,
            cursor_col: 0,
            selection_start: None,
            max_lines: None,
            disabled: false,
            display_lines,
            scroll_offset: 0,
            scroll_pixel_offset: 0.0,
            user_has_scrolled: false,
        }
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn with_max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    /// Get the full text content
    pub fn get_text(&self) -> String {
        self.lines.join("\n")
    }

    /// Set text content
    pub fn set_text(&mut self, text: &str) {
        self.lines = if text.is_empty() {
            vec![String::new()]
        } else {
            text.lines().map(|s| s.to_string()).collect()
        };
        self.cursor_line = self.lines.len().saturating_sub(1);
        self.cursor_col = self.lines.last().map(|l| l.len()).unwrap_or(0);
        self.update_scroll();
    }

    /// Handle character input
    pub fn insert_char(&mut self, c: char) {
        if self.disabled {
            return;
        }

        if c == '\n' {
            self.insert_newline();
        } else {
            let line = &mut self.lines[self.cursor_line];
            line.insert(self.cursor_col, c);
            self.cursor_col += 1;
        }
        self.reset_scroll_to_bottom();
    }

    /// Insert a newline at cursor position
    pub fn insert_newline(&mut self) {
        if let Some(max) = self.max_lines {
            if self.lines.len() >= max {
                return;
            }
        }

        let current_line = &self.lines[self.cursor_line];
        let (before, after) = current_line.split_at(self.cursor_col);
        let new_line = after.to_string();
        self.lines[self.cursor_line] = before.to_string();
        self.lines.insert(self.cursor_line + 1, new_line);
        self.cursor_line += 1;
        self.cursor_col = 0;
        self.update_scroll();
        self.reset_scroll_to_bottom();
    }

    /// Handle backspace
    pub fn backspace(&mut self) {
        if self.disabled {
            return;
        }

        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_line];
            line.remove(self.cursor_col - 1);
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            // Merge with previous line
            let current_line = self.lines.remove(self.cursor_line);
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
            self.lines[self.cursor_line].push_str(&current_line);
            self.update_scroll();
        }
        self.reset_scroll_to_bottom();
    }

    /// Handle delete key
    pub fn delete(&mut self) {
        if self.disabled {
            return;
        }

        let line = &self.lines[self.cursor_line];
        if self.cursor_col < line.len() {
            let line = &mut self.lines[self.cursor_line];
            line.remove(self.cursor_col);
        } else if self.cursor_line < self.lines.len() - 1 {
            // Merge with next line
            let next_line = self.lines.remove(self.cursor_line + 1);
            self.lines[self.cursor_line].push_str(&next_line);
        }
        self.reset_scroll_to_bottom();
    }

    /// Move cursor up
    pub fn move_up(&mut self) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            let line_len = self.lines[self.cursor_line].len();
            self.cursor_col = self.cursor_col.min(line_len);
            self.update_scroll();
        }
    }

    /// Move cursor down
    pub fn move_down(&mut self) {
        if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            let line_len = self.lines[self.cursor_line].len();
            self.cursor_col = self.cursor_col.min(line_len);
            self.update_scroll();
        }
    }

    /// Move cursor left
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.lines[self.cursor_line].len();
            self.update_scroll();
        }
    }

    /// Move cursor right
    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor_line].len();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_line < self.lines.len() - 1 {
            self.cursor_line += 1;
            self.cursor_col = 0;
            self.update_scroll();
        }
    }

    /// Update scroll offset to keep cursor visible
    fn update_scroll(&mut self) {
        if self.cursor_line < self.scroll_offset {
            self.scroll_offset = self.cursor_line;
        } else if self.cursor_line >= self.scroll_offset + self.display_lines {
            self.scroll_offset = self.cursor_line - self.display_lines + 1;
        }
    }

    /// Update scroll when typing to ensure we auto-scroll to bottom
    pub fn update_scroll_for_typing(&mut self) {
        // Reset user scroll flag when typing
        self.user_has_scrolled = false;

        // For simplicity, reset to bottom when typing
        // The actual pixel calculation will be done during rendering
        self.scroll_offset = self.lines.len().saturating_sub(self.display_lines);
    }

    /// Clear all text
    pub fn clear(&mut self) {
        self.lines = vec![String::new()];
        self.cursor_line = 0;
        self.cursor_col = 0;
        self.scroll_offset = 0;
        self.selection_start = None;
    }

    /// Render the multi-line text input
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let visible_lines = self
            .lines
            .iter()
            .skip(self.scroll_offset)
            .take(self.display_lines)
            .enumerate();

        let mut line_elements = Vec::new();

        for (idx, line) in visible_lines {
            let actual_line = self.scroll_offset + idx;
            let is_cursor_line = actual_line == self.cursor_line;

            // Build line text with cursor
            let display_text = if is_cursor_line && self.focused {
                let mut text = line.clone();
                if self.cursor_col <= text.len() {
                    text.insert(self.cursor_col, '\u{2502}'); // Cursor character
                } else {
                    text.push('\u{2502}');
                }
                text
            } else if line.is_empty() && actual_line == 0 && self.lines.len() == 1 && !self.focused
            {
                // Show placeholder on first line if empty and not focused
                self.placeholder.clone()
            } else {
                line.clone()
            };

            let text_color =
                if line.is_empty() && actual_line == 0 && self.lines.len() == 1 && !self.focused {
                    LinearRgba::with_components(0.5, 0.5, 0.5, 1.0)
                } else {
                    LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
                };

            // Use Text instead of WrappedText to prevent line expansion
            // Long lines will be clipped, which is better than expanding the input box
            let line_element = Element::new(font, ElementContent::Text(display_text))
                .colors(ElementColors {
                    text: text_color.into(),
                    ..Default::default()
                })
                .padding(BoxDimension {
                    left: Dimension::Pixels(4.0),
                    right: Dimension::Pixels(4.0),
                    top: Dimension::Pixels(2.0),
                    bottom: Dimension::Pixels(2.0),
                })
                .max_width(Some(Dimension::Pixels(250.0))); // Constrain width

            line_elements.push(line_element);
        }

        // Add empty lines if needed to fill display area
        while line_elements.len() < self.display_lines {
            let empty_line_idx = self.scroll_offset + line_elements.len();
            let is_cursor_on_empty_line =
                self.focused && self.cursor_line == empty_line_idx && self.cursor_col == 0;

            let display_text = if is_cursor_on_empty_line {
                "\u{2502}".to_string() // Just cursor on empty line
            } else {
                " ".to_string() // Empty space to maintain height
            };

            line_elements.push(
                Element::new(font, ElementContent::Text(display_text))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension {
                        left: Dimension::Pixels(4.0),
                        right: Dimension::Pixels(4.0),
                        top: Dimension::Pixels(2.0),
                        bottom: Dimension::Pixels(2.0),
                    })
                    .display(DisplayType::Block) // Ensure block display for proper height
                    .min_height(Some(Dimension::Pixels(
                        font.metrics().cell_height.get() as f32
                    ))), // Ensure minimum line height
            );
        }

        // Container with border
        Element::new(font, ElementContent::Children(line_elements))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                border: BorderColor::new(if self.focused {
                    LinearRgba::with_components(0.4, 0.6, 0.9, 0.7)
                } else if self.disabled {
                    LinearRgba::with_components(0.2, 0.2, 0.25, 0.3)
                } else {
                    LinearRgba::with_components(0.3, 0.3, 0.35, 0.5)
                }),
                ..Default::default()
            })
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .padding(BoxDimension {
                left: Dimension::Pixels(8.0),
                right: Dimension::Pixels(8.0),
                top: Dimension::Pixels(6.0),
                bottom: Dimension::Pixels(6.0),
            })
    }

    /// Handle mouse click to position cursor
    pub fn handle_click(&mut self, relative_pos: Point2D<f32>, font: &Rc<LoadedFont>) {
        if self.disabled {
            return;
        }

        // Calculate line height from font metrics
        let line_height = font.metrics().cell_height.get() as f32;

        // Determine which display line was clicked
        let display_line = (relative_pos.y / line_height) as usize;

        if display_line < self.display_lines {
            let actual_line = self.scroll_offset + display_line;
            if actual_line < self.lines.len() {
                self.cursor_line = actual_line;

                // Estimate column position from x coordinate
                // For now, use a simple character width approximation
                let line_text = &self.lines[actual_line];
                // Use cell_width as approximation for average character width
                let char_width = font.metrics().cell_width.get() as f32;
                let estimated_col = (relative_pos.x / char_width) as usize;

                // Convert character position to actual column, handling UTF-8
                let mut col = 0;
                for (idx, _) in line_text.char_indices() {
                    if idx >= estimated_col {
                        break;
                    }
                    col += 1;
                }

                self.cursor_col = col.min(line_text.chars().count());
                self.selection_start = None; // Clear any existing selection
            }
        }
    }

    /// Start text selection at current cursor position
    pub fn start_selection(&mut self) {
        if !self.disabled {
            self.selection_start = Some((self.cursor_line, self.cursor_col));
        }
    }

    /// Update selection end point based on mouse position
    pub fn update_selection(&mut self, relative_pos: Point2D<f32>, font: &Rc<LoadedFont>) {
        if self.disabled || self.selection_start.is_none() {
            return;
        }

        // Similar to handle_click but preserves selection_start
        let line_height = font.metrics().cell_height.get() as f32;
        let display_line = (relative_pos.y / line_height) as usize;

        if display_line < self.display_lines {
            let actual_line = self.scroll_offset + display_line;
            if actual_line < self.lines.len() {
                self.cursor_line = actual_line;

                let line_text = &self.lines[actual_line];
                // Use cell_width as approximation for average character width
                let char_width = font.metrics().cell_width.get() as f32;
                let estimated_col = (relative_pos.x / char_width) as usize;

                let mut col = 0;
                for (idx, _) in line_text.char_indices() {
                    if idx >= estimated_col {
                        break;
                    }
                    col += 1;
                }

                self.cursor_col = col.min(line_text.chars().count());
                self.update_scroll();
            }
        }
    }

    /// Handle mouse wheel events for scrolling
    pub fn handle_wheel(&mut self, delta: i32) -> bool {
        if !self.focused || self.lines.len() <= self.display_lines {
            return false;
        }

        // Mark that user has manually scrolled
        self.user_has_scrolled = true;

        // Update line-based scroll offset for simple scrolling
        let old_offset = self.scroll_offset;

        if delta > 0 {
            // Scroll up
            self.scroll_offset = self.scroll_offset.saturating_sub(1);
        } else {
            // Scroll down
            let max_offset = self.lines.len().saturating_sub(self.display_lines);
            self.scroll_offset = (self.scroll_offset + 1).min(max_offset);
        }

        // Also update pixel offset to match
        // Assume ~20 pixels per line for smooth scrolling
        self.scroll_pixel_offset = self.scroll_offset as f32 * 20.0;

        old_offset != self.scroll_offset
    }

    /// Get the currently selected text
    pub fn get_selected_text(&self) -> Option<String> {
        let (start_line, start_col) = self.selection_start?;
        let (end_line, end_col) = (self.cursor_line, self.cursor_col);

        // Normalize selection direction
        let (start_line, start_col, end_line, end_col) =
            if start_line < end_line || (start_line == end_line && start_col <= end_col) {
                (start_line, start_col, end_line, end_col)
            } else {
                (end_line, end_col, start_line, start_col)
            };

        // Handle single-line selection
        if start_line == end_line {
            let line = &self.lines[start_line];
            let chars: Vec<char> = line.chars().collect();

            if start_col < chars.len() && end_col <= chars.len() && start_col < end_col {
                return Some(chars[start_col..end_col].iter().collect());
            }
            return None;
        }

        // Handle multi-line selection
        let mut selected = String::new();

        // First line (from start_col to end)
        if start_line < self.lines.len() {
            let chars: Vec<char> = self.lines[start_line].chars().collect();
            if start_col < chars.len() {
                selected.push_str(&chars[start_col..].iter().collect::<String>());
                selected.push('\n');
            }
        }

        // Middle lines (full lines)
        for line_idx in (start_line + 1)..end_line {
            if line_idx < self.lines.len() {
                selected.push_str(&self.lines[line_idx]);
                selected.push('\n');
            }
        }

        // Last line (from beginning to end_col)
        if end_line < self.lines.len() && end_col > 0 {
            let chars: Vec<char> = self.lines[end_line].chars().collect();
            if end_col <= chars.len() {
                selected.push_str(&chars[..end_col].iter().collect::<String>());
            }
        }

        if selected.is_empty() {
            None
        } else {
            Some(selected)
        }
    }

    /// Handle keyboard events with modifiers
    pub fn handle_key_event(
        &mut self,
        key: &KeyCode,
        modifiers: KeyModifiers,
    ) -> Result<bool, anyhow::Error> {
        if self.disabled {
            return Ok(false);
        }

        match key {
            // Text navigation with selection support
            KeyCode::LeftArrow => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if self.selection_start.is_none() {
                        self.start_selection();
                    }
                }
                self.move_left();
                if !modifiers.contains(KeyModifiers::SHIFT) {
                    self.selection_start = None;
                }
                Ok(true)
            }
            KeyCode::RightArrow => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if self.selection_start.is_none() {
                        self.start_selection();
                    }
                }
                self.move_right();
                if !modifiers.contains(KeyModifiers::SHIFT) {
                    self.selection_start = None;
                }
                Ok(true)
            }
            KeyCode::UpArrow => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if self.selection_start.is_none() {
                        self.start_selection();
                    }
                }
                self.move_up();
                if !modifiers.contains(KeyModifiers::SHIFT) {
                    self.selection_start = None;
                }
                Ok(true)
            }
            KeyCode::DownArrow => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if self.selection_start.is_none() {
                        self.start_selection();
                    }
                }
                self.move_down();
                if !modifiers.contains(KeyModifiers::SHIFT) {
                    self.selection_start = None;
                }
                Ok(true)
            }
            KeyCode::Home => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if self.selection_start.is_none() {
                        self.start_selection();
                    }
                }
                self.cursor_col = 0;
                if !modifiers.contains(KeyModifiers::SHIFT) {
                    self.selection_start = None;
                }
                Ok(true)
            }
            KeyCode::End => {
                if modifiers.contains(KeyModifiers::SHIFT) {
                    if self.selection_start.is_none() {
                        self.start_selection();
                    }
                }
                self.cursor_col = self.lines[self.cursor_line].chars().count();
                if !modifiers.contains(KeyModifiers::SHIFT) {
                    self.selection_start = None;
                }
                Ok(true)
            }
            // Select all
            KeyCode::Char('a')
                if modifiers.contains(KeyModifiers::CTRL)
                    || modifiers.contains(KeyModifiers::SUPER) =>
            {
                self.selection_start = Some((0, 0));
                self.cursor_line = self.lines.len().saturating_sub(1);
                self.cursor_col = self.lines.last().map(|l| l.chars().count()).unwrap_or(0);
                self.update_scroll();
                Ok(true)
            }
            // Character input
            KeyCode::Char(c) => {
                // Delete selection if any
                if self.selection_start.is_some() {
                    self.delete_selection();
                }
                self.insert_char(*c);
                self.update_scroll_for_typing();
                Ok(true)
            }
            KeyCode::Backspace => {
                if self.selection_start.is_some() {
                    self.delete_selection();
                } else {
                    self.backspace();
                }
                Ok(true)
            }
            KeyCode::Delete => {
                if self.selection_start.is_some() {
                    self.delete_selection();
                } else {
                    self.delete();
                }
                Ok(true)
            }
            KeyCode::Enter => {
                // Enter with Shift inserts newline
                if modifiers.contains(KeyModifiers::SHIFT) {
                    self.insert_newline();
                    self.update_scroll_for_typing();
                    Ok(true)
                } else {
                    // Plain Enter - let parent handle (for send action)
                    Ok(false)
                }
            }
            _ => Ok(false),
        }
    }

    /// Delete the currently selected text
    fn delete_selection(&mut self) {
        if let Some((start_line, start_col)) = self.selection_start {
            let (end_line, end_col) = (self.cursor_line, self.cursor_col);

            // Normalize selection direction
            let (start_line, start_col, end_line, end_col) =
                if start_line < end_line || (start_line == end_line && start_col <= end_col) {
                    (start_line, start_col, end_line, end_col)
                } else {
                    (end_line, end_col, start_line, start_col)
                };

            if start_line == end_line {
                // Single line deletion
                let line = &mut self.lines[start_line];
                let chars: Vec<char> = line.chars().collect();
                if start_col < chars.len() && end_col <= chars.len() && start_col < end_col {
                    *line = chars[..start_col].iter().chain(&chars[end_col..]).collect();
                }
            } else {
                // Multi-line deletion
                let start_line_text = self.lines[start_line]
                    .chars()
                    .take(start_col)
                    .collect::<String>();
                let end_line_text = self.lines[end_line]
                    .chars()
                    .skip(end_col)
                    .collect::<String>();

                // Combine the parts
                self.lines[start_line] = start_line_text + &end_line_text;

                // Remove the lines in between
                for _ in 0..(end_line - start_line) {
                    if start_line + 1 < self.lines.len() {
                        self.lines.remove(start_line + 1);
                    }
                }
            }

            self.cursor_line = start_line;
            self.cursor_col = start_col;
            self.selection_start = None;
            self.update_scroll();
            self.reset_scroll_to_bottom();
        }
    }

    /// Get visible lines for rendering
    pub fn visible_lines(&self) -> impl Iterator<Item = &String> {
        self.lines
            .iter()
            .skip(self.scroll_offset)
            .take(self.display_lines)
    }

    /// Get the first visible line index
    pub fn first_visible_line(&self) -> usize {
        self.scroll_offset
    }

    /// Create style spans for text with selection
    fn create_selection_spans(
        &self,
        text: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> Vec<StyleSpan> {
        let mut spans = vec![];

        // Text before selection (if any)
        if start_byte > 0 {
            spans.push(StyleSpan {
                start: 0,
                end: start_byte,
                colors: ElementColors::default(),
                font: None,
                font_style: None,
            });
        }

        // Selected text with blue background
        spans.push(StyleSpan {
            start: start_byte,
            end: end_byte,
            colors: ElementColors {
                bg: LinearRgba::with_components(0.3, 0.5, 0.8, 0.8).into(),
                text: LinearRgba::with_components(1.0, 1.0, 1.0, 1.0).into(),
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
                colors: ElementColors::default(),
                font: None,
                font_style: None,
            });
        }

        spans
    }

    /// Render with selection highlighting
    pub fn render_with_selection(&self, font: &Rc<LoadedFont>) -> Element {
        let mut line_elements = Vec::new();

        // Debug: Ensure we only process display_lines
        let visible_line_count = self.visible_lines().count();
        if visible_line_count > self.display_lines {
            log::error!(
                "BUG: visible_lines() returned {} lines but display_lines is {}",
                visible_line_count,
                self.display_lines
            );
        }

        for (idx, line_text) in self.visible_lines().enumerate() {
            let actual_line_idx = self.first_visible_line() + idx;
            let is_cursor_line = actual_line_idx == self.cursor_line;

            // Check if this line has any selection
            if let Some((start_line, start_col)) = self.selection_start {
                let (end_line, end_col) = (self.cursor_line, self.cursor_col);

                // Normalize selection direction
                let (start_line, start_col, end_line, end_col) =
                    if start_line < end_line || (start_line == end_line && start_col <= end_col) {
                        (start_line, start_col, end_line, end_col)
                    } else {
                        (end_line, end_col, start_line, start_col)
                    };

                if actual_line_idx >= start_line && actual_line_idx <= end_line {
                    // This line has selection
                    let (sel_start_col, sel_end_col) =
                        if actual_line_idx == start_line && actual_line_idx == end_line {
                            // Selection within single line
                            (start_col, end_col)
                        } else if actual_line_idx == start_line {
                            // Selection starts on this line
                            (start_col, line_text.chars().count())
                        } else if actual_line_idx == end_line {
                            // Selection ends on this line
                            (0, end_col)
                        } else {
                            // Entire line is selected
                            (0, line_text.chars().count())
                        };

                    // Convert column positions to byte offsets
                    let start_byte = line_text
                        .char_indices()
                        .nth(sel_start_col)
                        .map(|(idx, _)| idx)
                        .unwrap_or(line_text.len());
                    let end_byte = line_text
                        .char_indices()
                        .nth(sel_end_col)
                        .map(|(idx, _)| idx)
                        .unwrap_or(line_text.len());

                    // Create styled text with selection
                    if start_byte < end_byte {
                        let spans = self.create_selection_spans(line_text, start_byte, end_byte);

                        // Add cursor if on this line
                        let display_text = if is_cursor_line && self.focused {
                            let mut text = line_text.clone();
                            let cursor_byte = line_text
                                .char_indices()
                                .nth(self.cursor_col)
                                .map(|(idx, _)| idx)
                                .unwrap_or(line_text.len());
                            text.insert_str(cursor_byte, "\u{2502}");
                            text
                        } else {
                            line_text.clone()
                        };

                        line_elements.push(
                            Element::new(
                                font,
                                ElementContent::StyledWrappedText {
                                    text: display_text,
                                    style_spans: spans,
                                },
                            )
                            .padding(BoxDimension {
                                left: Dimension::Pixels(4.0),
                                right: Dimension::Pixels(4.0),
                                top: Dimension::Pixels(2.0),
                                bottom: Dimension::Pixels(2.0),
                            }),
                        );
                        continue;
                    }
                }
            }

            // No selection on this line - render normally
            let display_text = if is_cursor_line && self.focused {
                let mut text = line_text.clone();
                let cursor_byte = line_text
                    .char_indices()
                    .nth(self.cursor_col)
                    .map(|(idx, _)| idx)
                    .unwrap_or(line_text.len());
                text.insert_str(cursor_byte, "\u{2502}");
                text
            } else if line_text.is_empty()
                && actual_line_idx == 0
                && self.lines.len() == 1
                && !self.focused
            {
                // Show placeholder
                self.placeholder.clone()
            } else {
                line_text.clone()
            };

            let text_color = if line_text.is_empty()
                && actual_line_idx == 0
                && self.lines.len() == 1
                && !self.focused
            {
                LinearRgba::with_components(0.5, 0.5, 0.5, 1.0)
            } else {
                LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
            };

            // Use Text instead of WrappedText to prevent line expansion
            line_elements.push(
                Element::new(font, ElementContent::Text(display_text))
                    .colors(ElementColors {
                        text: text_color.into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension {
                        left: Dimension::Pixels(4.0),
                        right: Dimension::Pixels(4.0),
                        top: Dimension::Pixels(2.0),
                        bottom: Dimension::Pixels(2.0),
                    })
                    .max_width(Some(Dimension::Pixels(250.0))), // Constrain width
            );
        }

        // Add empty lines if needed to fill display area
        while line_elements.len() < self.display_lines {
            let empty_line_idx = self.scroll_offset + line_elements.len();
            let is_cursor_on_empty_line =
                self.focused && self.cursor_line == empty_line_idx && self.cursor_col == 0;

            let display_text = if is_cursor_on_empty_line {
                "\u{2502}".to_string() // Just cursor on empty line
            } else {
                " ".to_string() // Empty space to maintain height
            };

            line_elements.push(
                Element::new(font, ElementContent::Text(display_text))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension {
                        left: Dimension::Pixels(4.0),
                        right: Dimension::Pixels(4.0),
                        top: Dimension::Pixels(2.0),
                        bottom: Dimension::Pixels(2.0),
                    })
                    .display(DisplayType::Block) // Ensure block display for proper height
                    .min_height(Some(Dimension::Pixels(
                        font.metrics().cell_height.get() as f32
                    ))), // Ensure minimum line height
            );
        }

        // Check if scrolling is needed (more lines than display_lines)
        let can_scroll_up = self.scroll_offset > 0;
        let can_scroll_down = self.scroll_offset + self.display_lines < self.lines.len();

        // Add visual indicator for scrollable content
        let right_padding = if can_scroll_up || can_scroll_down {
            6.0 // Less padding to show scroll indicator
        } else {
            8.0 // Normal padding
        };

        // Container with border
        let mut container = Element::new(font, ElementContent::Children(line_elements))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                border: BorderColor::new(if self.focused {
                    LinearRgba::with_components(0.4, 0.6, 0.9, 0.7)
                } else if self.disabled {
                    LinearRgba::with_components(0.2, 0.2, 0.25, 0.3)
                } else {
                    LinearRgba::with_components(0.3, 0.3, 0.35, 0.5)
                }),
                ..Default::default()
            })
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .padding(BoxDimension {
                left: Dimension::Pixels(8.0),
                right: Dimension::Pixels(right_padding),
                top: Dimension::Pixels(6.0),
                bottom: Dimension::Pixels(6.0),
            });

        // Add a subtle visual hint when content is scrollable
        if can_scroll_up || can_scroll_down {
            // The border color change already provides feedback
            // The reduced right padding also hints at scrollable content
        }

        container
    }

    /// Render the text content with scissor rect clipping for scrollable area

    /// Handle mouse wheel scrolling
    pub fn handle_wheel_scroll(&mut self, delta: f32, line_height: f32) -> bool {
        let total_height = self.lines.len() as f32 * line_height;
        let viewport_height = self.display_lines as f32 * line_height;

        if total_height <= viewport_height {
            return false; // No scrolling needed
        }

        // Mark that user has manually scrolled
        self.user_has_scrolled = true;

        // Update pixel-based scroll offset
        let old_offset = self.scroll_pixel_offset;
        let max_scroll = (total_height - viewport_height).max(0.0);
        self.scroll_pixel_offset = (self.scroll_pixel_offset - delta * line_height)
            .max(0.0)
            .min(max_scroll);

        // Update line-based scroll offset for compatibility
        self.scroll_offset = (self.scroll_pixel_offset / line_height) as usize;

        old_offset != self.scroll_pixel_offset
    }

    /// Reset scroll position when content changes
    pub fn reset_scroll_to_bottom(&mut self) {
        self.user_has_scrolled = false;
        // scroll_pixel_offset will be recalculated in render_with_scissor
    }
}

/// Button component
#[derive(Debug, Clone)]
pub struct Button {
    /// Button label
    pub label: String,
    /// Whether the button is hovered
    pub hovered: bool,
    /// Whether the button is pressed
    pub pressed: bool,
    /// Whether the button is disabled
    pub disabled: bool,
    /// Button style variant
    pub variant: ButtonVariant,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Danger,
    Ghost,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            hovered: false,
            pressed: false,
            disabled: false,
            variant: ButtonVariant::Primary,
        }
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        if !self.disabled {
            self.hovered = hovered;
        }
    }

    pub fn set_pressed(&mut self, pressed: bool) {
        if !self.disabled {
            self.pressed = pressed;
        }
    }

    /// Render as Element
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let (bg_color, text_color, border_color) = match self.variant {
            ButtonVariant::Primary => {
                let base_color = LinearRgba::with_components(0.2, 0.5, 0.8, 1.0);
                if self.disabled {
                    (
                        LinearRgba::with_components(0.1, 0.1, 0.1, 1.0),
                        LinearRgba::with_components(0.4, 0.4, 0.4, 1.0),
                        LinearRgba::with_components(0.3, 0.3, 0.3, 1.0),
                    )
                } else if self.pressed {
                    (
                        base_color.mul_alpha(0.8),
                        LinearRgba::with_components(1.0, 1.0, 1.0, 1.0),
                        base_color.mul_alpha(0.8),
                    )
                } else if self.hovered {
                    (
                        base_color.mul_alpha(0.9),
                        LinearRgba::with_components(1.0, 1.0, 1.0, 1.0),
                        base_color.mul_alpha(0.9),
                    )
                } else {
                    (
                        base_color,
                        LinearRgba::with_components(1.0, 1.0, 1.0, 1.0),
                        base_color,
                    )
                }
            }
            ButtonVariant::Secondary => {
                if self.disabled {
                    (
                        LinearRgba::with_components(0.05, 0.05, 0.05, 1.0),
                        LinearRgba::with_components(0.4, 0.4, 0.4, 1.0),
                        LinearRgba::with_components(0.3, 0.3, 0.3, 1.0),
                    )
                } else if self.pressed {
                    (
                        LinearRgba::with_components(0.3, 0.3, 0.3, 1.0),
                        LinearRgba::with_components(0.9, 0.9, 0.9, 1.0),
                        LinearRgba::with_components(0.5, 0.5, 0.5, 1.0),
                    )
                } else if self.hovered {
                    (
                        LinearRgba::with_components(0.2, 0.2, 0.2, 1.0),
                        LinearRgba::with_components(0.9, 0.9, 0.9, 1.0),
                        LinearRgba::with_components(0.4, 0.4, 0.4, 1.0),
                    )
                } else {
                    (
                        LinearRgba::with_components(0.05, 0.05, 0.05, 1.0),
                        LinearRgba::with_components(0.9, 0.9, 0.9, 1.0),
                        LinearRgba::with_components(0.4, 0.4, 0.4, 1.0),
                    )
                }
            }
            ButtonVariant::Danger => {
                let base_color = LinearRgba::with_components(0.8, 0.2, 0.2, 1.0);
                if self.disabled {
                    (
                        LinearRgba::with_components(0.1, 0.1, 0.1, 1.0),
                        LinearRgba::with_components(0.4, 0.4, 0.4, 1.0),
                        LinearRgba::with_components(0.3, 0.3, 0.3, 1.0),
                    )
                } else if self.pressed {
                    (
                        base_color.mul_alpha(0.8),
                        LinearRgba::with_components(1.0, 1.0, 1.0, 1.0),
                        base_color.mul_alpha(0.8),
                    )
                } else if self.hovered {
                    (
                        base_color.mul_alpha(0.9),
                        LinearRgba::with_components(1.0, 1.0, 1.0, 1.0),
                        base_color.mul_alpha(0.9),
                    )
                } else {
                    (
                        base_color,
                        LinearRgba::with_components(1.0, 1.0, 1.0, 1.0),
                        base_color,
                    )
                }
            }
            ButtonVariant::Ghost => {
                if self.disabled {
                    (
                        LinearRgba::with_components(0.05, 0.05, 0.05, 0.0),
                        LinearRgba::with_components(0.4, 0.4, 0.4, 1.0),
                        LinearRgba::with_components(0.05, 0.05, 0.05, 0.0),
                    )
                } else if self.pressed {
                    (
                        LinearRgba::with_components(0.3, 0.3, 0.3, 0.2),
                        LinearRgba::with_components(0.9, 0.9, 0.9, 1.0),
                        LinearRgba::with_components(0.05, 0.05, 0.05, 0.0),
                    )
                } else if self.hovered {
                    (
                        LinearRgba::with_components(0.2, 0.2, 0.2, 0.1),
                        LinearRgba::with_components(0.9, 0.9, 0.9, 1.0),
                        LinearRgba::with_components(0.05, 0.05, 0.05, 0.0),
                    )
                } else {
                    (
                        LinearRgba::with_components(0.05, 0.05, 0.05, 0.0),
                        LinearRgba::with_components(0.9, 0.9, 0.9, 1.0),
                        LinearRgba::with_components(0.05, 0.05, 0.05, 0.0),
                    )
                }
            }
        };

        Element::new(font, ElementContent::Text(self.label.clone()))
            .colors(ElementColors {
                border: BorderColor::new(border_color),
                bg: bg_color.into(),
                text: text_color.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Pixels(16.),
                right: Dimension::Pixels(16.),
                top: Dimension::Pixels(8.),
                bottom: Dimension::Pixels(8.),
            })
            .border(BoxDimension {
                left: Dimension::Pixels(1.),
                right: Dimension::Pixels(1.),
                top: Dimension::Pixels(1.),
                bottom: Dimension::Pixels(1.),
            })
            .display(DisplayType::Inline)
    }
}

/// Toggle switch component
#[derive(Debug, Clone)]
pub struct Toggle {
    /// Whether the toggle is on
    pub checked: bool,
    /// Whether the toggle is disabled
    pub disabled: bool,
    /// Whether the toggle is hovered
    pub hovered: bool,
}

impl Toggle {
    pub fn new(checked: bool) -> Self {
        Self {
            checked,
            disabled: false,
            hovered: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        if !self.disabled {
            self.hovered = hovered;
        }
    }

    pub fn toggle(&mut self) {
        if !self.disabled {
            self.checked = !self.checked;
        }
    }

    /// Render as Element
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let indicator_color = if self.disabled {
            LinearRgba::with_components(0.3, 0.3, 0.3, 1.0)
        } else if self.checked {
            LinearRgba::with_components(0.2, 0.8, 0.2, 1.0)
        } else {
            LinearRgba::with_components(0.4, 0.4, 0.4, 1.0)
        };

        let indicator = if self.checked { "●" } else { "○" };
        let text = format!("[{}]", indicator);

        Element::new(font, ElementContent::Text(text))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: InheritableColor::Inherited,
                text: indicator_color.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Pixels(4.),
                right: Dimension::Pixels(4.),
                top: Dimension::Pixels(2.),
                bottom: Dimension::Pixels(2.),
            })
            .display(DisplayType::Inline)
    }
}

/// Dropdown/select component
#[derive(Debug, Clone)]
pub struct Dropdown {
    /// Available options
    pub options: Vec<DropdownOption>,
    /// Currently selected option index
    pub selected: Option<usize>,
    /// Whether the dropdown is open
    pub open: bool,
    /// Whether the dropdown is disabled
    pub disabled: bool,
    /// Placeholder text when nothing selected
    pub placeholder: String,
}

#[derive(Debug, Clone)]
pub struct DropdownOption {
    pub value: String,
    pub label: String,
}

impl Dropdown {
    pub fn new() -> Self {
        Self {
            options: Vec::new(),
            selected: None,
            open: false,
            disabled: false,
            placeholder: "Select...".to_string(),
        }
    }

    pub fn with_options(mut self, options: Vec<DropdownOption>) -> Self {
        self.options = options;
        self
    }

    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn select(&mut self, index: usize) {
        if index < self.options.len() {
            self.selected = Some(index);
            self.open = false;
        }
    }

    pub fn toggle_open(&mut self) {
        if !self.disabled {
            self.open = !self.open;
        }
    }

    pub fn get_selected_value(&self) -> Option<&str> {
        self.selected
            .and_then(|idx| self.options.get(idx))
            .map(|opt| opt.value.as_str())
    }

    /// Render as Element
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let display_text = self
            .selected
            .and_then(|idx| self.options.get(idx))
            .map(|opt| opt.label.as_str())
            .unwrap_or(&self.placeholder);

        let border_color = if self.disabled {
            LinearRgba::with_components(0.3, 0.3, 0.3, 1.0)
        } else if self.open {
            LinearRgba::with_components(0.2, 0.5, 0.8, 1.0)
        } else {
            LinearRgba::with_components(0.4, 0.4, 0.4, 1.0)
        };

        let text_color = if self.disabled {
            LinearRgba::with_components(0.4, 0.4, 0.4, 1.0)
        } else if self.selected.is_none() {
            LinearRgba::with_components(0.5, 0.5, 0.5, 1.0)
        } else {
            LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
        };

        let arrow = if self.open { "▲" } else { "▼" };
        let text = format!("{} {}", display_text, arrow);

        Element::new(font, ElementContent::Text(text))
            .colors(ElementColors {
                border: BorderColor::new(border_color),
                bg: LinearRgba::with_components(0.05, 0.05, 0.05, 1.0).into(),
                text: text_color.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Pixels(8.),
                right: Dimension::Pixels(8.),
                top: Dimension::Pixels(4.),
                bottom: Dimension::Pixels(4.),
            })
            .border(BoxDimension {
                left: Dimension::Pixels(1.),
                right: Dimension::Pixels(1.),
                top: Dimension::Pixels(1.),
                bottom: Dimension::Pixels(1.),
            })
            .display(DisplayType::Block)
            .min_width(Some(Dimension::Pixels(200.)))
    }

    /// Render dropdown options as separate Elements
    pub fn render_options(&self, font: &Rc<LoadedFont>) -> Vec<Element> {
        if !self.open || self.options.is_empty() {
            return vec![];
        }

        self.options
            .iter()
            .enumerate()
            .map(|(idx, option)| {
                let is_selected = self.selected == Some(idx);
                let bg_color = if is_selected {
                    LinearRgba::with_components(0.2, 0.5, 0.8, 1.0)
                } else {
                    LinearRgba::with_components(0.05, 0.05, 0.05, 1.0)
                };
                let text_color = if is_selected {
                    LinearRgba::with_components(1.0, 1.0, 1.0, 1.0)
                } else {
                    LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
                };

                Element::new(font, ElementContent::Text(option.label.clone()))
                    .colors(ElementColors {
                        border: BorderColor::new(LinearRgba::with_components(0.4, 0.4, 0.4, 1.0)),
                        bg: bg_color.into(),
                        text: text_color.into(),
                    })
                    .padding(BoxDimension {
                        left: Dimension::Pixels(8.),
                        right: Dimension::Pixels(8.),
                        top: Dimension::Pixels(4.),
                        bottom: Dimension::Pixels(4.),
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(200.)))
            })
            .collect()
    }
}

/// Slider component
#[derive(Debug, Clone)]
pub struct Slider {
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Current value
    pub value: f64,
    /// Step size
    pub step: f64,
    /// Whether the slider is disabled
    pub disabled: bool,
    /// Whether showing value label
    pub show_value: bool,
}

impl Slider {
    pub fn new(min: f64, max: f64, value: f64) -> Self {
        Self {
            min,
            max,
            value: value.clamp(min, max),
            step: 1.0,
            disabled: false,
            show_value: true,
        }
    }

    pub fn with_step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }

    pub fn set_value(&mut self, value: f64) {
        if !self.disabled {
            self.value = value.clamp(self.min, self.max);
        }
    }

    pub fn increment(&mut self) {
        self.set_value(self.value + self.step);
    }

    pub fn decrement(&mut self) {
        self.set_value(self.value - self.step);
    }

    /// Get normalized position (0.0 to 1.0)
    pub fn get_position(&self) -> f64 {
        (self.value - self.min) / (self.max - self.min)
    }

    /// Render as Element
    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let fill_color = if self.disabled {
            LinearRgba::with_components(0.3, 0.3, 0.3, 1.0)
        } else {
            LinearRgba::with_components(0.2, 0.5, 0.8, 1.0)
        };

        // Simple text representation for now
        let position = self.get_position();
        let filled_width = (20.0 * position) as usize;
        let empty_width = 20 - filled_width;

        let track = format!("{}{}", "█".repeat(filled_width), "░".repeat(empty_width));

        let text = if self.show_value {
            format!("{} {:.1}", track, self.value)
        } else {
            track
        };

        Element::new(font, ElementContent::Text(text))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: InheritableColor::Inherited,
                text: fill_color.into(),
            })
            .padding(BoxDimension {
                left: Dimension::Pixels(8.),
                right: Dimension::Pixels(8.),
                top: Dimension::Pixels(4.),
                bottom: Dimension::Pixels(4.),
            })
            .display(DisplayType::Block)
    }
}

/// Form validation helpers
pub struct FormValidator;

impl FormValidator {
    /// Validate required field
    pub fn required(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            Some("This field is required".to_string())
        } else {
            None
        }
    }

    /// Validate email format
    pub fn email(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None; // Use required() for that
        }

        let email_regex = regex::Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap();
        if !email_regex.is_match(value) {
            Some("Invalid email format".to_string())
        } else {
            None
        }
    }

    /// Validate minimum length
    pub fn min_length(value: &str, min: usize) -> Option<String> {
        if value.len() < min {
            Some(format!("Must be at least {} characters", min))
        } else {
            None
        }
    }

    /// Validate maximum length
    pub fn max_length(value: &str, max: usize) -> Option<String> {
        if value.len() > max {
            Some(format!("Must be at most {} characters", max))
        } else {
            None
        }
    }

    /// Validate numeric value
    pub fn numeric(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None;
        }

        if value.parse::<f64>().is_err() {
            Some("Must be a valid number".to_string())
        } else {
            None
        }
    }

    /// Validate integer value
    pub fn integer(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None;
        }

        if value.parse::<i64>().is_err() {
            Some("Must be a valid integer".to_string())
        } else {
            None
        }
    }

    /// Validate URL format
    pub fn url(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None;
        }

        if url::Url::parse(value).is_err() {
            Some("Invalid URL format".to_string())
        } else {
            None
        }
    }

    /// Validate hostname/IP
    pub fn hostname(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None;
        }

        // Check if it's a valid IP address
        if value.parse::<std::net::IpAddr>().is_ok() {
            return None;
        }

        // Check if it's a valid hostname
        let hostname_regex = regex::Regex::new(r"^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$").unwrap();
        if !hostname_regex.is_match(value) {
            Some("Invalid hostname or IP address".to_string())
        } else {
            None
        }
    }

    /// Validate port number
    pub fn port(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None;
        }

        match value.parse::<u16>() {
            Ok(port) if port > 0 => None,
            _ => Some("Invalid port number (must be 1-65535)".to_string()),
        }
    }

    /// Validate file path exists
    pub fn file_exists(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None;
        }

        if !std::path::Path::new(value).exists() {
            Some("File does not exist".to_string())
        } else {
            None
        }
    }

    /// Validate directory path exists
    pub fn dir_exists(value: &str) -> Option<String> {
        if value.trim().is_empty() {
            return None;
        }

        let path = std::path::Path::new(value);
        if !path.exists() {
            Some("Directory does not exist".to_string())
        } else if !path.is_dir() {
            Some("Path is not a directory".to_string())
        } else {
            None
        }
    }

    /// Combine multiple validators
    pub fn combine(value: &str, validators: &[fn(&str) -> Option<String>]) -> Option<String> {
        for validator in validators {
            if let Some(error) = validator(value) {
                return Some(error);
            }
        }
        None
    }
}

// Color picker and file picker would be more complex and require additional UI infrastructure
// For now, we'll leave placeholders for future implementation

/// Color picker component (placeholder)
#[derive(Debug, Clone)]
pub struct ColorPicker {
    pub color: LinearRgba,
    pub disabled: bool,
}

impl ColorPicker {
    pub fn new(color: LinearRgba) -> Self {
        Self {
            color,
            disabled: false,
        }
    }

    // TODO: Implement color picker UI
}

/// File picker component (placeholder)
#[derive(Debug, Clone)]
pub struct FilePicker {
    pub path: Option<std::path::PathBuf>,
    pub filter: FilePickerFilter,
    pub disabled: bool,
}

#[derive(Debug, Clone)]
pub enum FilePickerFilter {
    All,
    SshKeys,
    Images,
    Documents,
    Custom(Vec<String>), // Extensions
}

impl FilePicker {
    pub fn new() -> Self {
        Self {
            path: None,
            filter: FilePickerFilter::All,
            disabled: false,
        }
    }

    // TODO: Implement file picker UI
}
