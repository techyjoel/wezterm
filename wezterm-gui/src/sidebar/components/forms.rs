//! Form components for sidebar UI
//! Provides text input, buttons, toggles, dropdowns, and other form elements

use crate::color::LinearRgba;
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent,
    InheritableColor,
};
use config::Dimension;
use std::rc::Rc;
use wezterm_font::LoadedFont;

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
