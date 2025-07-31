//! AI assistant sidebar implementation
//!
//! This module provides the main AI sidebar interface for WezTerm, including:
//! - Activity log showing commands, chats, and suggestions
//! - Agent status display (idle, thinking, gathering data, needs approval)
//! - Input field for user interactions
//! - Suggestion cards with "more..." expansion to modals
//! - Modal overlays for expanded content
//!
//! The sidebar manages its own state and rendering, integrating with the
//! terminal window through event handlers and the rendering pipeline.

use super::components::markdown::{CodeBlockContainer, CodeBlockRegistry};
use super::components::{
    Card, CardState, Chip, ChipSize, ChipStyle, MarkdownRenderer, Modal, ModalContent,
    ModalManager, ModalSize, MultilineTextInput, ScrollbarInfo, SuggestionModal,
};
use super::{Sidebar, SidebarConfig, SidebarFonts, SidebarPosition};
use crate::color::LinearRgba;
use crate::sidebar::position_cache::ElementType;
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent, Float,
    InheritableColor, StyleSpan,
};
use crate::termwindow::render::scrollbar_renderer::{ScrollbarOrientation, ScrollbarRenderer};
use crate::termwindow::UIItemType;
use anyhow::Result;
use config::{Dimension, DimensionContext};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use termwiz::input::KeyCode;
use wezterm_font::{FontConfiguration, LoadedFont};
use wezterm_term::KeyModifiers;
use window::{MouseButtons, MouseEvent, MouseEventKind as WMEK, MousePress, PixelUnit, RectF};

// Virtual scrolling constants
const RENDER_MARGIN: f32 = 200.0; // Pixels to render beyond viewport
const WIDTH_CHANGE_THRESHOLD: f32 = 5.0; // Pixels of width change to trigger cache clear
const HEIGHT_CHANGE_HYSTERESIS: f32 = 2.0; // Minimum height change to update cache

// Activity item spacing constants (must match render_activity_item)
const CHAT_ITEM_PADDING: f32 = 12.0; // Padding on all sides
const CHAT_ITEM_BOTTOM_MARGIN: f32 = 8.0; // Bottom margin between chat items
const CHAT_ITEM_BORDER: f32 = 1.0; // Border width
const CHAT_ITEM_HORIZONTAL_MARGIN: f32 = 20.0; // Left margin for user, right margin for AI
const CARD_DEFAULT_MARGIN: f32 = 8.0; // Default card margin (for commands/suggestions)
const SCROLLBAR_SPACE: f32 = 12.0; // Space reserved for scrollbar

/// Tracks height measurement state for activity items
#[derive(Debug, Clone, Default)]
struct HeightTracker {
    /// For tall items - scroll offset when top edge entered viewport
    top_entered_at: Option<f32>,

    /// For tall items - scroll offset when bottom edge entered viewport  
    bottom_entered_at: Option<f32>,

    /// Whether we've seen this item's full height (unclipped)
    seen_full_height: bool,

    /// Final measured height (from rendering or scroll tracking)
    measured_height: Option<f32>,
}

/// Visual anchor for maintaining scroll position stability
#[derive(Debug, Clone)]
struct VisualAnchor {
    /// Index of the anchored item in filtered items
    item_index: usize,
    /// Offset within the item (0 = top of item)
    offset_within_item: f32,
    /// Position on screen where anchor appears (0 = top of viewport)
    screen_position: f32,
}

// Character width estimation for suggestion cards
// This is tuned specifically for the sidebar's font (Roboto)
// Activity log uses 0.6 which is more conservative
const SUGGESTION_CHAR_WIDTH_MULTIPLIER: f32 = 0.4; // Try to get close to 2 full lines (but not beyond)

// Selection rendering constants
const GOAL_CARD_PADDING: f32 = 8.0; // Padding inside goal card
const SELECTION_CHAR_WIDTH: f32 = 8.5; // Approximate character width for selection
const SELECTION_LINE_HEIGHT: f32 = 25.0; // Height of selection rectangle
const SELECTION_VERTICAL_OFFSET: f32 = 4.0; // Offset to align selection with text
const SELECTION_CURSOR_WIDTH: f32 = 2.0; // Width of cursor for zero-width selection

#[derive(Debug, Clone, PartialEq)]
pub enum AgentMode {
    Idle,
    Thinking,
    GatheringData,
    NeedsApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityFilter {
    All,
    Commands,
    Chat,
    Suggestions,
}

/// Tracks text selection state in the sidebar
#[derive(Debug, Clone, Default)]
pub struct SelectionState {
    pub active_selection: Option<SelectionTarget>,
    pub prepared_selection: Option<SelectionTarget>,
    pub is_dragging: bool,
}

#[derive(Debug, Clone)]
pub enum SelectionTarget {
    ActivityItem {
        // Anchor position (where selection started)
        anchor_index: usize,
        anchor_byte: usize,
        // Current position (where selection ends)
        current_index: usize,
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
    ChatInput {
        anchor_line: usize,
        anchor_byte: usize,
        current_line: usize,
        current_byte: usize,
    },
}

impl SelectionState {
    pub fn get_selected_text(&self, sidebar: &AiSidebar) -> Option<String> {
        match &self.active_selection {
            None => None,
            Some(selection) => match selection {
                SelectionTarget::ActivityItem {
                    anchor_index,
                    anchor_byte,
                    current_index,
                    current_byte,
                } => {
                    // Handle single-item or multi-item selection
                    if anchor_index == current_index {
                        // Single item selection
                        sidebar.activity_log.get(*anchor_index).and_then(|item| {
                            let text = get_item_text(item);
                            let start = anchor_byte.min(current_byte);
                            let end = anchor_byte.max(current_byte);
                            text.get(*start..*end).map(|s| s.to_string())
                        })
                    } else {
                        // Multi-item selection
                        let start_index = anchor_index.min(current_index);
                        let end_index = anchor_index.max(current_index);
                        let mut selected_text = String::new();
                        
                        for index in *start_index..=*end_index {
                            if let Some(item) = sidebar.activity_log.get(index) {
                                let text = get_item_text(item);
                                
                                if index == *start_index && index == *anchor_index {
                                    // First item, from anchor_byte to end
                                    if let Some(partial) = text.get(*anchor_byte..) {
                                        selected_text.push_str(partial);
                                    }
                                } else if index == *start_index {
                                    // First item, from current_byte to end
                                    if let Some(partial) = text.get(*current_byte..) {
                                        selected_text.push_str(partial);
                                    }
                                } else if index == *end_index && index == *anchor_index {
                                    // Last item, from start to anchor_byte
                                    if let Some(partial) = text.get(..*anchor_byte) {
                                        selected_text.push_str(partial);
                                    }
                                } else if index == *end_index {
                                    // Last item, from start to current_byte
                                    if let Some(partial) = text.get(..*current_byte) {
                                        selected_text.push_str(partial);
                                    }
                                } else {
                                    // Middle items, entire text
                                    selected_text.push_str(text);
                                }
                                
                                // Add newline between items
                                if index < *end_index {
                                    selected_text.push('\n');
                                }
                            }
                        }
                        
                        if selected_text.is_empty() {
                            None
                        } else {
                            Some(selected_text)
                        }
                    }
                }
                SelectionTarget::Suggestion {
                    anchor_byte,
                    current_byte,
                } => {
                    if let Some(suggestion) = &sidebar.current_suggestion {
                        let start = anchor_byte.min(current_byte);
                        let end = anchor_byte.max(current_byte);
                        suggestion.content.get(*start..*end).map(|s| s.to_string())
                    } else {
                        None
                    }
                }
                SelectionTarget::Goal {
                    anchor_byte,
                    current_byte,
                } => {
                    if let Some(goal) = &sidebar.current_goal {
                        let start = anchor_byte.min(current_byte);
                        let end = anchor_byte.max(current_byte);
                        goal.text.get(*start..*end).map(|s| s.to_string())
                    } else {
                        None
                    }
                }
                SelectionTarget::ChatInput {
                    anchor_line,
                    anchor_byte,
                    current_line,
                    current_byte,
                } => {
                    // Get selected text from chat input
                    let lines = &sidebar.chat_input.lines;

                    if *anchor_line == *current_line {
                        // Single line selection
                        if let Some(line) = lines.get(*anchor_line) {
                            let start = anchor_byte.min(current_byte);
                            let end = anchor_byte.max(current_byte);
                            line.get(*start..*end).map(|s| s.to_string())
                        } else {
                            None
                        }
                    } else {
                        // Multi-line selection
                        let start_line = anchor_line.min(current_line);
                        let end_line = anchor_line.max(current_line);
                        let mut selected = String::new();

                        for (idx, line) in lines.iter().enumerate() {
                            if idx >= *start_line && idx <= *end_line {
                                if idx == *start_line {
                                    // First line - from byte offset to end
                                    let start_byte = if *anchor_line == *start_line {
                                        *anchor_byte
                                    } else {
                                        *current_byte
                                    };
                                    if let Some(text) = line.get(start_byte..) {
                                        selected.push_str(text);
                                    }
                                } else if idx == *end_line {
                                    // Last line - from start to byte offset
                                    let end_byte = if *anchor_line == *end_line {
                                        *anchor_byte
                                    } else {
                                        *current_byte
                                    };
                                    if let Some(text) = line.get(..end_byte) {
                                        selected.push_str(text);
                                    }
                                } else {
                                    // Middle lines - entire line
                                    selected.push_str(line);
                                }

                                // Add newline between lines (except after last line)
                                if idx < *end_line {
                                    selected.push('\n');
                                }
                            }
                        }

                        if selected.is_empty() {
                            None
                        } else {
                            Some(selected)
                        }
                    }
                }
            },
        }
    }

    pub fn clear(&mut self) {
        self.active_selection = None;
        self.prepared_selection = None;
        self.is_dragging = false;
    }
}

/// Extract plain text from an activity item for selection
fn get_item_text(item: &ActivityItem) -> &str {
    match item {
        ActivityItem::Chat { message, .. } => message,
        ActivityItem::Command {
            command, output, ..
        } => output.as_deref().unwrap_or(command),
        ActivityItem::Suggestion { content, .. } => content,
        ActivityItem::Goal { text, .. } => text,
    }
}

/// Calculate character positions for hit testing
fn calculate_char_positions(text: &str, font: &Rc<LoadedFont>) -> Vec<(f32, f32, usize)> {
    let mut positions = Vec::new();
    // The font metrics report cell_width=20px but actual rendered width is ~10px
    // This is because:
    // 1. Roboto is proportional, not monospace, so cell_width is misleading
    // 2. Font size reduction of 1pt is applied (12pt -> 11pt)
    // Based on logs showing "3.5 characters too far to the left", we need ~50% of metric width
    let base_char_width = font.metrics().cell_width.get() as f32;
    let char_width = base_char_width * 0.5; // Empirically determined from logs

    // Diagnostic logging for font metrics
    // log::warn!("[FONT_METRICS] calculate_char_positions: font_style={:?}, base_cell_width={}, adjusted_width={}, cell_height={}, descender={}",
    //     font.style(),
    //     base_char_width,
    //     char_width,
    //     font.metrics().cell_height.get(),
    //     font.metrics().descender.get()
    // );

    let mut x = 0.0;
    let mut byte_offset = 0;

    for ch in text.chars() {
        // More accurate character width estimation for Roboto proportional font
        let ch_width = if ch.is_ascii_alphabetic() {
            if ch.is_ascii_uppercase() {
                char_width * 1.2 // Uppercase letters are wider
            } else {
                char_width // Lowercase letters
            }
        } else if ch.is_ascii_digit() {
            char_width * 0.9 // Numbers are slightly narrower
        } else if ch == ' ' {
            char_width * 0.4 // Spaces are much narrower in proportional fonts
        } else if ch == '.' || ch == ',' || ch == ':' || ch == ';' {
            char_width * 0.3 // Punctuation is very narrow
        } else if ch.is_ascii_punctuation() {
            char_width * 0.5 // Other punctuation
        } else {
            char_width * 1.5 // Non-ASCII characters (emoji, etc)
        };

        positions.push((x, x + ch_width, byte_offset));

        x += ch_width;
        byte_offset += ch.len_utf8();
    }

    let display_text = if text.len() < 50 {
        text.to_string()
    } else {
        format!("{}...", &text[..47])
    };
    log::warn!(
        "[FONT_METRICS] Calculated {} positions for text '{}' (len={}), total_width={}",
        positions.len(),
        display_text,
        text.len(),
        x
    );

    positions
}

/// Create style spans for text with selection
fn create_selection_spans(text: &str, start_byte: usize, end_byte: usize) -> Vec<StyleSpan> {
    let mut spans = vec![];

    log::debug!(
        "Creating selection spans: text_len={}, start_byte={}, end_byte={}",
        text.len(),
        start_byte,
        end_byte
    );

    // Text before selection (if any)
    if start_byte > 0 {
        spans.push(StyleSpan {
            start: 0,
            end: start_byte,
            colors: ElementColors {
                text: InheritableColor::Color(LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)), // Normal text color
                ..ElementColors::default()
            },
            font: None,
            font_style: None,
        });
    }

    // Selected text with blue background
    spans.push(StyleSpan {
        start: start_byte,
        end: end_byte,
        colors: ElementColors {
            bg: InheritableColor::Color(LinearRgba::with_components(0.2, 0.4, 0.7, 1.0)), // Blue selection with full opacity
            text: InheritableColor::Color(LinearRgba::with_components(1.0, 1.0, 1.0, 1.0)), // White text
            ..ElementColors::default()
        },
        font: None,
        font_style: None,
    });

    // Text after selection (if any)
    if end_byte < text.len() {
        spans.push(StyleSpan {
            start: end_byte,
            end: text.len(),
            colors: ElementColors {
                text: InheritableColor::Color(LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)), // Normal text color
                ..ElementColors::default()
            },
            font: None,
            font_style: None,
        });
    }

    spans
}

#[derive(Debug, Clone)]
pub enum ActivityItem {
    Command {
        id: String,
        command: String,
        output: Option<String>,
        pane_id: Option<String>,
        status: CommandStatus,
        timestamp: SystemTime,
        expanded: bool,
    },
    Chat {
        id: String,
        message: String,
        is_user: bool,
        timestamp: SystemTime,
    },
    Suggestion {
        id: String,
        title: String,
        content: String,
        timestamp: SystemTime,
        is_current: bool,
    },
    Goal {
        id: String,
        text: String,
        timestamp: SystemTime,
        is_current: bool,
        is_confirmed: bool,
    },
}

#[derive(Debug, Clone)]
pub enum CommandStatus {
    Running,
    Success,
    Failed(i32),
}

pub struct CurrentGoal {
    text: String,
    is_ai_inferred: bool,
    is_confirmed: bool,
    is_editing: bool,
    edit_text: String,
}

#[derive(Clone)]
pub struct CurrentSuggestion {
    pub title: String,
    pub content: String,
    pub has_action: bool,
    pub action_type: Option<String>, // "run", "dismiss", etc
}

/// Main AI assistant sidebar implementation
///
/// Manages the state and rendering of the AI sidebar, including:
/// - Activity log with filtering
/// - Agent status and modes
/// - User input handling
/// - Suggestion display with modal expansion
/// - Scroll state and interaction
pub struct AiSidebar {
    config: SidebarConfig,
    visible: bool,
    width: u16,

    // UI State
    agent_mode: AgentMode,
    agent_mode_enabled: bool,
    high_risk_mode_enabled: bool,
    pub activity_filter: ActivityFilter,

    // Data
    pub current_goal: Option<CurrentGoal>,
    pub current_suggestion: Option<CurrentSuggestion>,
    activity_log: Vec<ActivityItem>,

    // UI Components
    chat_input: MultilineTextInput,

    // Height caching for virtual scrolling
    activity_log_height_cache: HashMap<String, f32>,

    // Height tracking state for each item
    height_trackers: HashMap<String, HeightTracker>,

    // Last known width for cache invalidation
    activity_log_last_width: Option<f32>,

    // Visible range for virtual scrolling
    activity_log_visible_range: Range<usize>,
    // Chat input colors for filled rectangle rendering
    chat_input_bg_color: LinearRgba,
    chat_input_border_color: LinearRgba,

    // Scrollbar info for external rendering
    activity_log_scrollbar: Option<ScrollbarInfo>,

    // Scrollbar renderer for handling events
    activity_log_scrollbar_renderer: Option<ScrollbarRenderer>,

    // Scrollbar bounds for hit testing
    activity_log_scrollbar_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,

    // Scroll state
    activity_log_scroll_offset: f32,

    // Visual anchor for maintaining position during height changes
    visual_anchor: Option<VisualAnchor>,

    // UI element bounds for hit testing
    filter_chip_bounds: Vec<(ActivityFilter, euclid::Rect<f32, window::PixelUnit>)>,
    more_link_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,

    // Sidebar position for coordinate conversion
    sidebar_x_position: f32,

    // Modal management
    pub modal_manager: ModalManager,

    // Code block registry for horizontal scrolling
    pub code_block_registry: Option<CodeBlockRegistry>,

    // Text selection state
    pub selection_state: SelectionState,

    // Position tracking for text selection
    position_cache: crate::sidebar::position_cache::TextPositionCache,
    item_positions: HashMap<usize, crate::sidebar::position_cache::ItemPositionData>,
    coordinate_transform: crate::sidebar::position_cache::CoordinateTransform,

    // Track bounds for hit testing
    activity_item_bounds: HashMap<usize, euclid::Rect<f32, window::PixelUnit>>,
    suggestion_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,
    goal_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,

    // Last known window height for mouse event handling
    last_viewport_height: Option<f32>,

    // Chat input bounds for scrollbar positioning
    chat_input_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,

    // Goal text character positions for accurate selection
    goal_char_positions: Option<Vec<(f32, f32, usize)>>,
}

impl AiSidebar {
    /// Start selection at the given byte offset for an activity item
    pub fn start_activity_item_selection(&mut self, index: usize, byte_offset: usize) {
        log::debug!(
            "start_activity_item_selection: index={}, byte_offset={}",
            index,
            byte_offset
        );
        if index < self.activity_log.len() {
            self.selection_state.active_selection = Some(SelectionTarget::ActivityItem {
                anchor_index: index,
                anchor_byte: byte_offset,
                current_index: index,
                current_byte: byte_offset,
            });
            self.selection_state.is_dragging = true;
            log::debug!("  Selection started successfully");
        } else {
            log::debug!(
                "  Index {} out of range (activity_log.len()={})",
                index,
                self.activity_log.len()
            );
        }
    }

    /// Start selection at the given byte offset for the suggestion
    pub fn start_suggestion_selection(&mut self, byte_offset: usize) {
        if self.current_suggestion.is_some() {
            self.selection_state.active_selection = Some(SelectionTarget::Suggestion {
                anchor_byte: byte_offset,
                current_byte: byte_offset,
            });
            self.selection_state.is_dragging = true;
        }
    }

    /// Start selection at the given byte offset for the goal
    pub fn start_goal_selection(&mut self, byte_offset: usize) {
        if self.current_goal.is_some() {
            self.selection_state.active_selection = Some(SelectionTarget::Goal {
                anchor_byte: byte_offset,
                current_byte: byte_offset,
            });
            self.selection_state.is_dragging = true;
        }
    }

    /// Start selection at the given position for an activity log item
    pub fn start_activity_log_selection(&mut self, item_index: usize, byte_offset: usize) {
        if self.activity_log.get(item_index).is_some() {
            self.selection_state.active_selection = Some(SelectionTarget::ActivityItem {
                anchor_index: item_index,
                anchor_byte: byte_offset,
                current_index: item_index,
                current_byte: byte_offset,
            });
            self.selection_state.is_dragging = true;
        }
    }

    /// Update selection during drag
    pub fn update_selection_drag(&mut self, byte_offset: usize) {
        if !self.selection_state.is_dragging {
            return;
        }

        // Update the current byte offset for the active selection
        match &mut self.selection_state.active_selection {
            Some(SelectionTarget::ActivityItem { current_byte, current_index, .. }) => {
                // This method only updates within the same item
                // For multi-item selection, use update_activity_log_selection_drag
                *current_byte = byte_offset;
            }
            Some(SelectionTarget::Suggestion { current_byte, .. }) => {
                *current_byte = byte_offset;
            }
            Some(SelectionTarget::Goal { current_byte, .. }) => {
                *current_byte = byte_offset;
            }
            Some(SelectionTarget::ChatInput {
                current_line,
                current_byte,
                ..
            }) => {
                // For chat input, we need to update both line and byte based on the drag position
                // This will be handled by a separate method that knows the visual position
            }
            None => {}
        }
    }

    /// Update activity log selection during drag, handling crossing item boundaries
    pub fn update_activity_log_selection_drag(&mut self, item_index: usize, byte_offset: usize) {
        if !self.selection_state.is_dragging {
            return;
        }

        // Check if we have an active activity item selection
        if let Some(SelectionTarget::ActivityItem { 
            anchor_index, 
            anchor_byte,
            .. 
        }) = &self.selection_state.active_selection {
            // Update the selection to span from anchor to current position
            // This properly handles selection across multiple items
            self.selection_state.active_selection = Some(SelectionTarget::ActivityItem {
                anchor_index: *anchor_index,
                anchor_byte: *anchor_byte,
                current_index: item_index,
                current_byte: byte_offset,
            });
        }
    }

    /// Start selection in chat input
    pub fn start_chat_input_selection(&mut self, line: usize, byte_offset: usize) {
        self.selection_state.active_selection = Some(SelectionTarget::ChatInput {
            anchor_line: line,
            anchor_byte: byte_offset,
            current_line: line,
            current_byte: byte_offset,
        });
        self.selection_state.is_dragging = true;
    }

    /// Update chat input selection during drag
    pub fn update_chat_input_selection(&mut self, line: usize, byte_offset: usize) {
        if !self.selection_state.is_dragging {
            return;
        }

        if let Some(SelectionTarget::ChatInput {
            current_line,
            current_byte,
            ..
        }) = &mut self.selection_state.active_selection
        {
            *current_line = line;
            *current_byte = byte_offset;
        }
    }

    /// Check if currently selecting text
    pub fn is_selecting(&self) -> bool {
        self.selection_state.is_dragging
    }

    /// Prepare for potential selection (on mouse down)
    /// Returns true if UI should be invalidated
    pub fn prepare_selection(&mut self, target: SelectionTarget) -> bool {
        // Check if clicking on existing selection to deselect
        if let Some(active) = &self.selection_state.active_selection {
            // If clicking within the same target type, clear selection
            let should_clear = match (active, &target) {
                (SelectionTarget::Goal { .. }, SelectionTarget::Goal { .. }) => true,
                (
                    SelectionTarget::ActivityItem { anchor_index: a, .. },
                    SelectionTarget::ActivityItem { anchor_index: b, .. },
                ) => a == b,
                (SelectionTarget::Suggestion { .. }, SelectionTarget::Suggestion { .. }) => true,
                (SelectionTarget::ChatInput { .. }, SelectionTarget::ChatInput { .. }) => true,
                _ => false,
            };

            if should_clear {
                self.selection_state.clear();
                // Don't return - allow starting a new selection after clearing
            }
        }

        // Store the potential selection but don't activate it yet
        self.selection_state.prepared_selection = Some(target);
        // Clear any existing selection if not already cleared
        let had_selection = self.selection_state.active_selection.is_some();
        self.selection_state.active_selection = None;
        self.selection_state.is_dragging = false;

        // Return true if we cleared an existing selection
        had_selection
    }

    /// Activate the prepared selection (on drag start)
    pub fn activate_prepared_selection(&mut self) {
        if let Some(prepared) = self.selection_state.prepared_selection.take() {
            self.selection_state.active_selection = Some(prepared);
            self.selection_state.is_dragging = true;
            // prepared_selection is already cleared by take()
        } else {
        }
    }

    /// End selection
    pub fn end_selection(&mut self) {
        self.selection_state.is_dragging = false;
    }

    /// Clear all selection state
    /// Returns true if there was a selection to clear
    pub fn clear_selection(&mut self) -> bool {
        let had_selection = self.selection_state.active_selection.is_some()
            || self.selection_state.prepared_selection.is_some();
        self.selection_state.clear();
        had_selection
    }

    /// Clear selection if activity log items change
    pub fn clear_selection_if_invalid(&mut self) {
        if let Some(SelectionTarget::ActivityItem { anchor_index, current_index, .. }) =
            &self.selection_state.active_selection
        {
            if *anchor_index >= self.activity_log.len() || *current_index >= self.activity_log.len() {
                self.selection_state.clear();
            }
        }
    }

    /// Estimate text position for hit testing
    fn estimate_text_position(
        &self,
        item_bounds: &euclid::Rect<f32, window::PixelUnit>,
        click_x: f32,
        text: &str,
        font: &Rc<LoadedFont>,
    ) -> usize {
        let relative_x = (click_x - item_bounds.origin.x).max(0.0);

        // For MVP, use a simple approach with character iteration
        let mut accumulated_width = 0.0;
        let mut byte_offset = 0;

        // Use cell width for character width approximation
        let char_width = font.metrics().cell_width.get() as f32;

        for ch in text.chars() {
            let ch_width = if ch.is_ascii() {
                char_width
            } else {
                char_width * 1.5 // Rough estimate for non-ASCII
            };

            if accumulated_width + ch_width / 2.0 > relative_x {
                break;
            }

            accumulated_width += ch_width;
            byte_offset += ch.len_utf8();
        }

        byte_offset
    }

    pub fn new(config: SidebarConfig) -> Self {
        Self {
            width: config.width,
            visible: config.show_on_startup,
            config,
            agent_mode: AgentMode::Idle,
            agent_mode_enabled: false,
            high_risk_mode_enabled: false,
            activity_filter: ActivityFilter::All,
            current_goal: None,
            current_suggestion: None,
            activity_log: Vec::new(),
            chat_input: MultilineTextInput::new(2).with_placeholder("Type a message..."),
            activity_log_height_cache: HashMap::new(),
            height_trackers: HashMap::new(),
            activity_log_last_width: None,
            activity_log_visible_range: 0..0,
            activity_log_scrollbar: None,
            activity_log_scrollbar_renderer: None,
            activity_log_scrollbar_bounds: None,
            activity_log_scroll_offset: 0.0,
            visual_anchor: None,
            filter_chip_bounds: Vec::new(),
            more_link_bounds: None,
            sidebar_x_position: 0.0,
            modal_manager: ModalManager::new(),
            code_block_registry: Some(Arc::new(Mutex::new(HashMap::new()))),
            selection_state: SelectionState::default(),
            position_cache: crate::sidebar::position_cache::TextPositionCache::new(100),
            item_positions: HashMap::new(),
            coordinate_transform: crate::sidebar::position_cache::CoordinateTransform {
                sidebar_x: 0.0,
                sidebar_y: 0.0,
                sidebar_width: 0.0,
                sidebar_height: 0.0,
            },
            activity_item_bounds: HashMap::new(),
            suggestion_bounds: None,
            goal_bounds: None,
            chat_input_bg_color: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0),
            chat_input_border_color: LinearRgba::with_components(0.3, 0.3, 0.35, 0.5),
            last_viewport_height: None,
            chat_input_bounds: None,
            goal_char_positions: None,
        }
    }

    // Mock data for development
    pub fn populate_mock_data(&mut self) {
        // Set a current goal
        let goal_text = "Fix the build errors in the project".to_string();
        log::debug!(
            "GOAL TEXT DEBUG: Setting mock goal text: '{}', len={}",
            goal_text,
            goal_text.len()
        );
        self.current_goal = Some(CurrentGoal {
            text: goal_text,
            is_ai_inferred: true,
            is_confirmed: false,
            is_editing: false,
            edit_text: String::new(),
        });

        // Set a current suggestion with very long content to test scrolling
        let test_content = r#"It looks like the linker couldn't find OpenSSL. This is a common issue when building projects that depend on OpenSSL for cryptographic functionality. Let me provide a comprehensive guide to resolving this issue.

## Quick Solution

Run the following command to install OpenSSL:

```bash
brew install openssl@3
```

## If That Doesn't Work

You may need to set environment variables to help the build system find OpenSSL:

```bash
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl@3/lib/pkgconfig"
export LDFLAGS="-L/opt/homebrew/opt/openssl@3/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl@3/include"
```

## Common Issues

1. **Wrong OpenSSL version**: Some projects require openssl@1.1 instead of openssl@3
2. **Multiple OpenSSL installations**: Check `brew list | grep openssl` to see all versions
3. **Architecture mismatch**: On M1 Macs, ensure you're using the right architecture
4. **Missing pkg-config**: Install with `brew install pkg-config`
5. **Incorrect paths**: Verify paths with `brew --prefix openssl@3`

## Detailed Troubleshooting Steps

### Step 1: Check Current Installation
First, let's check what OpenSSL versions you have installed:

```bash
brew list | grep openssl
ls -la /opt/homebrew/opt/ | grep openssl
which openssl
openssl version
```

### Step 2: Clean Installation
If you have conflicts, clean up first:

```bash
brew uninstall --ignore-dependencies openssl@3
brew uninstall --ignore-dependencies openssl@1.1
brew cleanup
```

### Step 3: Fresh Install
Install the required version:

```bash
brew install openssl@3
brew link openssl@3 --force
```

### Step 4: Verify Installation
Check that everything is properly installed:

```bash
brew test openssl@3
pkg-config --libs openssl
```

### Step 5: Configure Your Shell
Add these to your shell configuration file (~/.zshrc or ~/.bashrc):

```bash
# OpenSSL Configuration
export PATH="/opt/homebrew/opt/openssl@3/bin:$PATH"
export LDFLAGS="-L/opt/homebrew/opt/openssl@3/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl@3/include"
export PKG_CONFIG_PATH="/opt/homebrew/opt/openssl@3/lib/pkgconfig"
```

### Step 6: Alternative Solutions

#### Using MacPorts
If Homebrew doesn't work, try MacPorts:

```bash
sudo port install openssl
sudo port select --set openssl openssl3
```

#### Building from Source
As a last resort, build OpenSSL from source:

```bash
wget https://www.openssl.org/source/openssl-3.0.7.tar.gz
tar -xf openssl-3.0.7.tar.gz
cd openssl-3.0.7
./config --prefix=/usr/local/openssl --openssldir=/usr/local/openssl
make
sudo make install
```

## Platform-Specific Notes

### macOS Monterey and Later
Apple has deprecated OpenSSL in favor of their own crypto libraries. You may need to:

1. Disable System Integrity Protection (not recommended)
2. Use a different crypto library
3. Explicitly specify OpenSSL paths in your build configuration

### M1/M2 Mac Considerations
On Apple Silicon, paths differ:
- Intel: `/usr/local/opt/openssl@3`
- Apple Silicon: `/opt/homebrew/opt/openssl@3`

## Related Issues
- libssl-dev on Linux: `sudo apt-get install libssl-dev`
- Windows: Use vcpkg or download prebuilt binaries
- Docker: Add `RUN apk add --no-cache openssl-dev` to Dockerfile

This should resolve most OpenSSL-related build issues. If problems persist, check your project's specific requirements.

If you're still having issues:

1. Clean your build directory: `make clean`
2. Check your PATH: `echo $PATH`
3. Verify OpenSSL installation: `brew info openssl@3`
4. Try linking manually: `brew link openssl@3 --force`

## References

- [Homebrew OpenSSL Formula](https://formulae.brew.sh/formula/openssl@3)
- [Common macOS linking issues](https://github.com/openssl/openssl/issues)

This should resolve your OpenSSL linking error. If problems persist, check your project's specific build documentation."#;

        self.current_suggestion = Some(CurrentSuggestion {
            title: "Install missing dependency".to_string(),
            content: test_content.to_string(),
            has_action: true,
            action_type: Some("run".to_string()),
        });

        // Add some activity items
        let now = SystemTime::now();
        self.activity_log.push(ActivityItem::Command {
            id: "cmd1".to_string(),
            command: "make (~/project)".to_string(),
            output: Some("Error: OpenSSL not found".to_string()),
            pane_id: Some("pane1".to_string()),
            status: CommandStatus::Failed(1),
            timestamp: now - Duration::from_secs(60),
            expanded: true, // Make it expanded to see if that's the tall content
        });

        self.activity_log.push(ActivityItem::Chat {
            id: "chat1".to_string(),
            message: "I'm trying to compile my Rust project but getting linker errors about OpenSSL. I've tried installing it before but it doesn't seem to be working. Can you help me understand what's going wrong and how to fix it properly?".to_string(),
            is_user: true,
            timestamp: now - Duration::from_secs(30),
        });

        // Add AI response with long markdown content to test text wrapping
        self.activity_log.push(ActivityItem::Chat {
            id: "chat2".to_string(),
            message: r#"I see you're getting an **OpenSSL error**. This is a very common issue when building projects that depend on OpenSSL for cryptographic functionality. Let me provide you with a comprehensive guide to resolve this issue on macOS.

## Quick Solution (Try This First)

The fastest way to resolve this is usually:

1. First, *check* if OpenSSL is installed:
   ```bash
   brew list openssl
   brew list | grep openssl
   ```

2. If **not installed**, run:
   ```bash
   brew install openssl@3
   # or for older projects:
   brew install openssl@1.1
   ```

3. Then set the environment variables:
   ```bash
   export OPENSSL_DIR=$(brew --prefix openssl)
   export PKG_CONFIG_PATH="$OPENSSL_DIR/lib/pkgconfig"
   export LDFLAGS="-L$OPENSSL_DIR/lib"
   export CPPFLAGS="-I$OPENSSL_DIR/include"
   # This is a very long line that should definitely trigger horizontal scrolling in the code block - it contains many characters and should exceed the width of the sidebar
   ```

4. Try running `make` again.

## Detailed Troubleshooting

If the quick solution doesn't work, here are more comprehensive steps:

### Step 1: Verify Your System
First, let's understand your environment:
```bash
# Check macOS version
sw_vers -productVersion

# Check architecture (Intel vs Apple Silicon)
uname -m

# Check Homebrew installation
brew --version
brew config
```

### Step 2: Clean Up Existing Installations
Sometimes conflicts arise from multiple OpenSSL installations:
```bash
# List all OpenSSL installations
brew list | grep openssl
ls -la /usr/local/opt/ | grep openssl
ls -la /opt/homebrew/opt/ | grep openssl

# If you have conflicts, uninstall all versions
brew uninstall --ignore-dependencies openssl@3
brew uninstall --ignore-dependencies openssl@1.1
brew uninstall --ignore-dependencies openssl
```

### Step 3: Install the Correct Version
Different projects require different OpenSSL versions:
```bash
# For modern projects (OpenSSL 3.x)
brew install openssl@3

# For older projects (OpenSSL 1.1)
brew install openssl@1.1

# Force link if needed
brew link openssl@3 --force
```

### Step 4: Configure pkg-config
The pkg-config tool helps compilers find libraries:
```bash
# Install pkg-config if missing
brew install pkg-config

# Verify it can find OpenSSL
pkg-config --modversion openssl
pkg-config --libs openssl
pkg-config --cflags openssl
```

### Alternative Solution
If the above doesn't work, you might need to:
```bash
# Install pkg-config
brew install pkg-config

# Or try using the system's built-in LibreSSL
export LDFLAGS="-L/usr/lib"
export CPPFLAGS="-I/usr/include"
```

## Platform-Specific Considerations

### Apple Silicon (M1/M2) Macs
Paths differ on Apple Silicon:
- Intel Macs: `/usr/local/opt/openssl`
- Apple Silicon: `/opt/homebrew/opt/openssl`

### macOS Ventura and Later
Apple has deprecated OpenSSL in favor of their own crypto libraries, which can cause additional complications.

This comprehensive guide should resolve most OpenSSL linking issues on macOS!"#.to_string(),
            is_user: false,
            timestamp: now - Duration::from_secs(20),
        });

        self.activity_log.push(ActivityItem::Chat {
            id: "chat3".to_string(),
            message: "Great! That worked. Now I'm seeing some warnings about deprecated functions."
                .to_string(),
            is_user: true,
            timestamp: now - Duration::from_secs(10),
        });

        // Add a Python example with indentation to test code block rendering
        self.activity_log.push(ActivityItem::Chat {
            id: "chat4".to_string(),
            message: r#"Here's a Python example showing proper error handling with indentation:

```python
def process_data(filename):
    """Process data from a file with proper error handling."""
    try:
        with open(filename, 'r') as file:
            data = file.read()
            # Process each line
            for line in data.splitlines():
                if line.strip():  # Skip empty lines
                    result = parse_line(line)
                    if result:
                        yield result
    except FileNotFoundError:
        print(f"Error: File '{filename}' not found")
        return None
    except PermissionError:
        print(f"Error: Permission denied for '{filename}'")
        return None
    finally:
        print("Processing complete")
```

This example demonstrates:
- Function definition with docstring
- Context manager (`with` statement)
- Nested indentation levels (up to 5 levels deep)
- Error handling with multiple `except` blocks
- The `finally` clause for cleanup"#
                .to_string(),
            is_user: false,
            timestamp: now - Duration::from_secs(5),
        });

        // Add more mock items to test scrolling
        for i in 0..20 {
            if i % 3 == 0 {
                self.activity_log.push(ActivityItem::Command {
                    id: format!("cmd{}", i + 10),
                    command: format!("test command {}", i),
                    output: Some(format!("Output for command {}", i)),
                    pane_id: Some("pane1".to_string()),
                    status: if i % 2 == 0 {
                        CommandStatus::Success
                    } else {
                        CommandStatus::Failed(1)
                    },
                    timestamp: now - Duration::from_secs(300 + i * 60),
                    expanded: false,
                });
            } else {
                self.activity_log.push(ActivityItem::Chat {
                    id: format!("chat{}", i + 10),
                    message: format!(
                        "Test message {} from {}",
                        i,
                        if i % 2 == 0 { "user" } else { "AI" }
                    ),
                    is_user: i % 2 == 0,
                    timestamp: now - Duration::from_secs(300 + i * 60),
                });
            }
        }

        self.agent_mode = AgentMode::Thinking;

        // Clear code block registry since we've replaced all content
        self.clear_code_block_registry();
    }

    fn render_header(&self, fonts: &SidebarFonts) -> Element {
        let title = Element::new(
            &fonts.heading,
            ElementContent::Text("CLiBuddy AI".to_string()),
        )
        .colors(ElementColors {
            text: LinearRgba::with_components(0.95, 0.95, 0.95, 1.0).into(),
            ..Default::default()
        })
        .padding(BoxDimension {
            left: Dimension::Pixels(16.0),
            top: Dimension::Pixels(12.0),
            bottom: Dimension::Pixels(12.0),
            right: Dimension::Pixels(16.0),
        });

        Element::new(&fonts.heading, ElementContent::Children(vec![title]))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.08, 0.08, 0.1, 1.0).into(),
                ..Default::default()
            })
            .border(BoxDimension {
                bottom: Dimension::Pixels(1.0),
                ..Default::default()
            })
            .colors(ElementColors {
                border: BorderColor::new(LinearRgba::with_components(0.2, 0.2, 0.25, 0.5)),
                bg: LinearRgba::with_components(0.08, 0.08, 0.1, 1.0).into(),
                ..Default::default()
            })
    }

    fn render_filter_chips(&mut self, fonts: &SidebarFonts) -> Element {
        let filters = vec![
            ("All", ActivityFilter::All),
            ("Commands", ActivityFilter::Commands),
            ("Chat", ActivityFilter::Chat),
            ("Suggestions", ActivityFilter::Suggestions),
        ];

        let chips: Vec<Element> = filters
            .into_iter()
            .map(|(label, filter)| {
                let is_selected = self.activity_filter == filter;
                let style = if is_selected {
                    ChipStyle::Primary
                } else {
                    ChipStyle::Default
                };

                Chip::new(label.to_string())
                    .with_style(style)
                    .with_size(ChipSize::Small)
                    .clickable(true)
                    .selected(is_selected)
                    .with_item_type(crate::termwindow::UIItemType::SidebarFilterChip(filter))
                    .render(&fonts.body)
            })
            .collect();

        Element::new(&fonts.body, ElementContent::Children(chips))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(8.0),
            })
    }

    fn render_status_chip(&self, fonts: &SidebarFonts) -> Element {
        let (label, style, icon) = match self.agent_mode {
            AgentMode::Idle => ("Idle", ChipStyle::Default, "○"),
            AgentMode::Thinking => ("Thinking", ChipStyle::Info, "◐"),
            AgentMode::GatheringData => ("Gathering Data", ChipStyle::Warning, "◑"),
            AgentMode::NeedsApproval => ("Needs Approval", ChipStyle::Error, "⚠"),
        };

        let chip = Chip::new(label.to_string())
            .with_style(style)
            .with_size(ChipSize::Medium)
            .with_icon(icon.to_string())
            .render(&fonts.body);

        Element::new(&fonts.body, ElementContent::Children(vec![chip]))
            .display(DisplayType::Block)
            .padding(BoxDimension {
                left: Dimension::Pixels(16.0),
                right: Dimension::Pixels(16.0),
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(4.0),
            })
    }

    fn render_current_goal(&self, fonts: &SidebarFonts) -> Option<Element> {
        let goal = self.current_goal.as_ref()?;

        // Goal text
        let goal_text = if goal.is_editing {
            // Show edit input
            Element::new(
                &fonts.body,
                ElementContent::Text(format!("{}_", &goal.edit_text)),
            )
            .colors(ElementColors {
                text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                bg: LinearRgba::with_components(0.15, 0.15, 0.17, 1.0).into(),
                ..Default::default()
            })
            .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        } else {
            // Check if this goal has a selection
            let selection = match &self.selection_state.active_selection {
                Some(SelectionTarget::Goal {
                    anchor_byte,
                    current_byte,
                }) => Some((
                    *anchor_byte.min(current_byte),
                    *anchor_byte.max(current_byte),
                )),
                _ => None,
            };

            // Always use WrappedText to avoid layout changes
            log::debug!(
                "GOAL TEXT DEBUG: Rendering goal text: '{}', len={}",
                goal.text,
                goal.text.len()
            );
            let elem = Element::new(&fonts.body, ElementContent::WrappedText(goal.text.clone()));

            // Calculate available width for goal text
            let sidebar_width = self.width as f32;
            let goal_content_width = sidebar_width - 40.0; // Account for padding

            log::debug!(
                "Goal width calculation: sidebar_width={}, content_width={}, has_selection={}",
                sidebar_width,
                goal_content_width,
                selection.is_some()
            );

            // Don't pre-calculate positions - they'll be extracted after rendering
            elem.item_type(UIItemType::GoalText {
                char_positions: Vec::new(), // Will be populated after rendering
            })
            .colors(ElementColors {
                text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                ..Default::default()
            })
            .max_width(Some(Dimension::Pixels(goal_content_width)))
            .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        };

        // Action buttons
        let mut actions = vec![];

        if goal.is_ai_inferred && !goal.is_confirmed && !goal.is_editing {
            let confirm_btn = Chip::new("✓".to_string())
                .with_style(ChipStyle::Success)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            actions.push(confirm_btn);
        }

        if !goal.is_editing {
            let edit_btn = Chip::new("✎".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            actions.push(edit_btn);
        } else {
            let save_btn = Chip::new("Save".to_string())
                .with_style(ChipStyle::Primary)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            let cancel_btn = Chip::new("Cancel".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Small)
                .clickable(true)
                .render(&fonts.body);
            actions.push(save_btn);
            actions.push(cancel_btn);
        }

        let card = Card::new()
            .with_title("Current Goal".to_string())
            .with_content(vec![goal_text])
            .with_actions(actions)
            .pass_through_events(true) // Allow child UIItemTypes to be detected
            .render(&fonts.heading);

        Some(
            Element::new(&fonts.body, ElementContent::Children(vec![card]))
                .display(DisplayType::Block)
                .padding(BoxDimension {
                    left: Dimension::Pixels(16.0),
                    right: Dimension::Pixels(16.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                }),
        )
    }

    fn render_current_suggestion(&mut self, fonts: &SidebarFonts) -> Option<Element> {
        let suggestion = self.current_suggestion.as_ref()?;

        // Clear previous more link bounds
        self.more_link_bounds = None;

        // Check if content would exceed 2 lines when wrapped
        const MAX_LINES: usize = 2;

        // Get approximate width available for text in the suggestion card
        // Sidebar: 16px padding each side = 32px
        // Card: 8px margin each side = 16px
        // Content container: 8px padding each side = 16px
        // Total: 32 + 16 + 16 = 64px
        let available_width = (self.width as f32) - 64.0;

        // Use our wrapping estimation to determine if we need truncation
        let estimated_lines =
            self.estimate_wrapped_lines(&suggestion.content, available_width, fonts);
        let needs_more_link = estimated_lines > MAX_LINES;

        let mut content_elements = vec![];

        if needs_more_link {
            // Truncate to fit within 2 lines using shared function
            let font_metrics = fonts.body.metrics();
            let avg_char_width =
                font_metrics.cell_height.get() as f32 * SUGGESTION_CHAR_WIDTH_MULTIPLIER;

            // Use shared truncation function
            let truncated_text = crate::termwindow::box_model::truncate_to_wrapped_lines(
                &suggestion.content,
                available_width,
                avg_char_width,
                MAX_LINES,
            );

            // Add ellipsis
            let display_text = format!("{}...", truncated_text);

            // Use plain text for truncated content
            content_elements.push(
                Element::new(&fonts.body, ElementContent::WrappedText(display_text))
                    .colors(ElementColors {
                        text: LinearRgba(0.9, 0.9, 0.9, 1.0).into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_height(Some(Dimension::Pixels(
                        2.0 * fonts.body.metrics().cell_height.get() as f32,
                    ))), // Fixed height for 2 lines
            );
        } else {
            // Check if this suggestion has a selection
            let selection = match &self.selection_state.active_selection {
                Some(SelectionTarget::Suggestion {
                    anchor_byte,
                    current_byte,
                }) => Some((
                    *anchor_byte.min(current_byte),
                    *anchor_byte.max(current_byte),
                )),
                _ => None,
            };

            // For short content, still use fixed height
            let elem = if let Some((start, end)) = selection {
                let spans = create_selection_spans(&suggestion.content, start, end);
                Element::new(
                    &fonts.body,
                    ElementContent::StyledWrappedText {
                        text: suggestion.content.clone(),
                        style_spans: spans,
                    },
                )
            } else {
                Element::new(
                    &fonts.body,
                    ElementContent::WrappedText(suggestion.content.clone()),
                )
            };

            // Calculate available width for suggestion text
            let sidebar_width = self.width as f32;
            let suggestion_content_width = sidebar_width - 40.0; // Account for padding

            log::debug!(
                "Suggestion width calculation: sidebar_width={}, content_width={}, has_selection={}",
                sidebar_width, suggestion_content_width, selection.is_some()
            );

            content_elements.push(
                elem.item_type(UIItemType::SuggestionText {
                    char_positions: calculate_char_positions(&suggestion.content, &fonts.body),
                })
                .colors(ElementColors {
                    text: LinearRgba(0.9, 0.9, 0.9, 1.0).into(),
                    ..Default::default()
                })
                .display(DisplayType::Block)
                .max_width(Some(Dimension::Pixels(suggestion_content_width)))
                .min_height(Some(Dimension::Pixels(
                    2.0 * fonts.body.metrics().cell_height.get() as f32,
                ))), // Fixed height for 2 lines
            );
        }

        let content_container =
            Element::new(&fonts.body, ElementContent::Children(content_elements))
                .display(DisplayType::Block)
                .padding(BoxDimension::new(Dimension::Pixels(8.0)));

        let mut actions = vec![];

        // Create a container for the action buttons
        let mut left_actions = vec![];
        let mut right_actions = vec![];

        if suggestion.has_action {
            let run_btn = Chip::new("▶ Run".to_string())
                .with_style(ChipStyle::Success)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::SuggestionRunButton)
                .render(&fonts.body);
            let dismiss_btn = Chip::new("✕ Dismiss".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::SuggestionDismissButton)
                .render(&fonts.body);

            left_actions.push(run_btn);
            left_actions.push(
                Element::new(&fonts.body, ElementContent::Text(" ".to_string()))
                    .min_width(Some(Dimension::Pixels(8.0))),
            );
            left_actions.push(dismiss_btn);
        }

        // Add "Show more" button on the right if needed
        if needs_more_link {
            let show_more_btn = Chip::new("Show more".to_string())
                .with_style(ChipStyle::Info)
                .with_size(ChipSize::Medium)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::ShowMoreButton(
                    "current".to_string(),
                ))
                .render(&fonts.body);
            right_actions.push(show_more_btn);
        }

        // Create the action row with left and right alignment
        if !left_actions.is_empty() || !right_actions.is_empty() {
            // Use a flex-like approach with float for right alignment
            if !left_actions.is_empty() {
                for action in left_actions {
                    actions.push(action);
                }
            }

            if !right_actions.is_empty() {
                // Right-align the show more button using float
                for action in right_actions {
                    actions.push(action.float(Float::Right));
                }
            }
        }

        let card = Card::new()
            .with_title(suggestion.title.clone())
            .with_content(vec![content_container])
            .with_actions(actions)
            .render(&fonts.heading);

        Some(
            Element::new(&fonts.body, ElementContent::Children(vec![card]))
                .display(DisplayType::Block)
                .padding(BoxDimension {
                    left: Dimension::Pixels(16.0),
                    right: Dimension::Pixels(16.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                }),
        )
    }

    pub fn render_activity_item(
        &self,
        item: &ActivityItem,
        fonts: &SidebarFonts,
        item_index: usize,
        palette: &wezterm_term::color::ColorPalette,
    ) -> Element {
        match item {
            ActivityItem::Command {
                command,
                output,
                status,
                expanded,
                ..
            } => {
                let status_icon = match status {
                    CommandStatus::Running => "◐",
                    CommandStatus::Success => "✓",
                    CommandStatus::Failed(_) => "✕",
                };

                let status_color = match status {
                    CommandStatus::Running => LinearRgba::with_components(0.5, 0.7, 1.0, 1.0),
                    CommandStatus::Success => LinearRgba::with_components(0.4, 0.8, 0.4, 1.0),
                    CommandStatus::Failed(_) => LinearRgba::with_components(0.9, 0.4, 0.4, 1.0),
                };

                let mut content = vec![Element::new(
                    &fonts.body,
                    ElementContent::Text(format!("{} {}", status_icon, command)),
                )
                .colors(ElementColors {
                    text: status_color.into(),
                    ..Default::default()
                })];

                if *expanded && output.is_some() {
                    content.push(
                        Element::new(
                            &fonts.body,
                            ElementContent::Text(output.as_ref().unwrap().clone()),
                        )
                        .colors(ElementColors {
                            text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
                            ..Default::default()
                        })
                        .padding(BoxDimension {
                            left: Dimension::Pixels(4.0),
                            top: Dimension::Pixels(4.0),
                            ..Default::default()
                        }),
                    );
                }

                Card::new().with_content(content).render(&fonts.body)
            }
            ActivityItem::Chat {
                message, is_user, ..
            } => {
                // Calculate available width for chat content
                let sidebar_width = self.width as f32;
                let content_width = sidebar_width
                    - CHAT_ITEM_HORIZONTAL_MARGIN
                    - (CHAT_ITEM_PADDING * 2.0)
                    - (CHAT_ITEM_BORDER * 2.0)
                    - SCROLLBAR_SPACE;

                let bg_color = if *is_user {
                    LinearRgba::with_components(0.1, 0.3, 0.5, 0.3)
                } else {
                    LinearRgba::with_components(0.15, 0.15, 0.17, 1.0)
                };

                // Check if this message has a selection
                let selection = match &self.selection_state.active_selection {
                    Some(SelectionTarget::ActivityItem {
                        anchor_index,
                        anchor_byte,
                        current_index,
                        current_byte,
                    }) if *anchor_index == item_index || *current_index == item_index => {
                        // Determine selection bounds for this item
                        if *anchor_index == *current_index && *anchor_index == item_index {
                            // Single item selection
                            Some((
                                *anchor_byte.min(current_byte),
                                *anchor_byte.max(current_byte),
                            ))
                        } else if *anchor_index.min(current_index) == item_index {
                            // This is the first item in multi-item selection
                            let byte_start = if *anchor_index == item_index {
                                *anchor_byte
                            } else {
                                *current_byte
                            };
                            Some((byte_start, usize::MAX)) // Select to end
                        } else if *anchor_index.max(current_index) == item_index {
                            // This is the last item in multi-item selection
                            let byte_end = if *anchor_index == item_index {
                                *anchor_byte
                            } else {
                                *current_byte
                            };
                            Some((0, byte_end)) // Select from start
                        } else if item_index > *anchor_index.min(current_index) 
                               && item_index < *anchor_index.max(current_index) {
                            // This is a middle item - select entire text
                            Some((0, usize::MAX))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };

                // Render message content with markdown if it's from AI
                let content = if *is_user {
                    // User messages - always use StyledWrappedText for consistency
                    let spans = if let Some((start, end)) = selection {
                        create_selection_spans(message, start, end)
                    } else {
                        // No selection - create a single span with default style
                        vec![StyleSpan {
                            start: 0,
                            end: message.len(),
                            colors: ElementColors {
                                text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                                ..Default::default()
                            },
                            font: None,
                            font_style: None,
                        }]
                    };

                    Element::new(
                        &fonts.body,
                        ElementContent::StyledWrappedText {
                            text: message.clone(),
                            style_spans: spans,
                        },
                    )
                    .item_type(UIItemType::ActivityItemText {
                        index: item_index,
                        char_positions: calculate_char_positions(message, &fonts.body),
                    })
                    .max_width(Some(Dimension::Pixels(content_width)))
                } else {
                    // AI messages - if there's a selection, render as plain text with selection
                    // Otherwise use markdown rendering
                    if let Some((start, end)) = selection {
                        let spans = create_selection_spans(message, start, end);
                        Element::new(
                            &fonts.body,
                            ElementContent::StyledWrappedText {
                                text: message.clone(),
                                style_spans: spans,
                            },
                        )
                        .item_type(UIItemType::ActivityItemText {
                            index: item_index,
                            char_positions: calculate_char_positions(message, &fonts.body),
                        })
                        .max_width(Some(Dimension::Pixels(content_width)))
                    } else {
                        // AI messages use markdown rendering with code font support
                        // Need to add width constraint for proper text wrapping
                        let sidebar_width = self.width as f32;
                        // Calculate available width accounting for all padding/margins:
                        // - Activity log container: no explicit padding
                        // - Chat message margin: CHAT_ITEM_HORIZONTAL_MARGIN on one side
                        // - Chat message padding: CHAT_ITEM_PADDING * 2
                        // - Chat message border: CHAT_ITEM_BORDER * 2
                        // - Scrollbar space: SCROLLBAR_SPACE
                        let spacing = CHAT_ITEM_HORIZONTAL_MARGIN
                            + (CHAT_ITEM_PADDING * 2.0)
                            + (CHAT_ITEM_BORDER * 2.0)
                            + SCROLLBAR_SPACE;
                        let content_width = sidebar_width - spacing;
                        log::debug!(
                            "Rendering markdown in activity log: sidebar_width={}, content_width={}",
                            sidebar_width,
                            content_width
                        );

                        // Use registry if available for horizontal scrolling support
                        let mut elem = if let Some(ref registry) = self.code_block_registry {
                            MarkdownRenderer::render_with_fonts_registry_and_palette(
                                message,
                                fonts,
                                Some(content_width),
                                Arc::clone(registry),
                                &format!("activity_{}", item_index),
                                palette,
                            )
                        } else {
                            MarkdownRenderer::render_with_fonts(message, fonts, Some(content_width))
                        }
                        .max_width(Some(Dimension::Pixels(content_width)));

                        // Add item type for click handling
                        elem = elem.item_type(UIItemType::ActivityItemText {
                            index: item_index,
                            char_positions: calculate_char_positions(message, &fonts.body),
                        });
                        elem
                    }
                };

                Element::new(&fonts.body, ElementContent::Children(vec![content]))
                    .display(DisplayType::Block)
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(CHAT_ITEM_PADDING)))
                    .margin(BoxDimension {
                        left: if *is_user {
                            Dimension::Pixels(CHAT_ITEM_HORIZONTAL_MARGIN)
                        } else {
                            Dimension::Pixels(0.0)
                        },
                        right: if *is_user {
                            Dimension::Pixels(0.0)
                        } else {
                            Dimension::Pixels(CHAT_ITEM_HORIZONTAL_MARGIN)
                        },
                        bottom: Dimension::Pixels(CHAT_ITEM_BOTTOM_MARGIN),
                        ..Default::default()
                    })
                    .border(BoxDimension::new(Dimension::Pixels(CHAT_ITEM_BORDER)))
                    .colors(ElementColors {
                        border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 0.5)),
                        bg: bg_color.into(),
                        ..Default::default()
                    })
            }
            ActivityItem::Suggestion { title, content, .. } => {
                // Add width constraint for proper text wrapping
                let sidebar_width = self.width as f32;
                // Calculate available width for suggestion card content:
                // - Card margin: 8px each side = 16px
                // - Card padding: 12px each side = 24px
                // - Card border: 1px each side = 2px
                // - Scrollbar space: ~12px
                // Total: 16 + 24 + 2 + 12 = 54px
                let content_width = sidebar_width - 54.0;
                let markdown_content = if let Some(ref registry) = self.code_block_registry {
                    MarkdownRenderer::render_with_fonts_registry_and_palette(
                        content,
                        fonts,
                        Some(content_width),
                        Arc::clone(registry),
                        &format!("suggestion_{}", item_index),
                        palette,
                    )
                } else {
                    MarkdownRenderer::render_with_fonts(content, fonts, Some(content_width))
                };

                Card::new()
                    .with_title(format!("Past: {}", title))
                    .with_content(vec![
                        markdown_content.max_width(Some(Dimension::Pixels(content_width)))
                    ])
                    .render(&fonts.heading)
            }
            ActivityItem::Goal { text, .. } => {
                Element::new(&fonts.body, ElementContent::Text(format!("Goal: {}", text)))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.8, 0.8, 0.8, 1.0).into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(8.0)))
            }
        }
    }

    /// Get filtered activity items based on current filter
    fn render_activity_log(
        &mut self,
        fonts: &SidebarFonts,
        available_height: f32,
        available_width: f32,
        palette: &wezterm_term::color::ColorPalette,
    ) -> Element {
        // Check if width has changed and invalidate cache if needed
        if let Some(last_width) = self.activity_log_last_width {
            let width_change = (last_width - available_width).abs();
            if width_change > WIDTH_CHANGE_THRESHOLD {
                log::info!(
                    "Significant width change from {} to {} (delta: {}), clearing height cache",
                    last_width,
                    available_width,
                    width_change
                );
                self.activity_log_height_cache.clear();
                self.height_trackers.clear();
            } else if width_change > 0.1 {
                log::trace!(
                    "Minor width change: {} -> {} (delta: {}), keeping cache",
                    last_width,
                    available_width,
                    width_change
                );
            }
        }
        self.activity_log_last_width = Some(available_width);

        // Filter items based on current filter
        let filtered_items: Vec<(usize, &ActivityItem)> = self
            .activity_log
            .iter()
            .enumerate()
            .filter(|(_, item)| match self.activity_filter {
                ActivityFilter::All => true,
                ActivityFilter::Commands => matches!(item, ActivityItem::Command { .. }),
                ActivityFilter::Chat => matches!(item, ActivityItem::Chat { .. }),
                ActivityFilter::Suggestions => matches!(item, ActivityItem::Suggestion { .. }),
            })
            .collect();

        let filtered_count = filtered_items.len();
        log::debug!(
            "Rendering activity log: {} total items, {} filtered items",
            self.activity_log.len(),
            filtered_count
        );

        // Get actual font metrics for accurate height calculations
        let font_metrics = fonts.body.metrics();
        let line_height = font_metrics.cell_height.get() as f32;

        // Calculate visible range based on scroll offset
        const BUFFER_ITEMS: usize = 3; // Render 3 items above and below viewport
        let viewport_start = self.activity_log_scroll_offset;
        let viewport_end = self.activity_log_scroll_offset + available_height;

        let mut first_visible: Option<usize> = None;
        let mut last_visible: Option<usize> = None;
        let mut current_y = 0.0;

        // Find which items are actually visible in the viewport
        for (idx, (orig_idx, item)) in filtered_items.iter().enumerate() {
            let item_id = match item {
                ActivityItem::Command { id, .. } => id.clone(),
                ActivityItem::Chat { id, .. } => id.clone(),
                ActivityItem::Suggestion { id, .. } => id.clone(),
                ActivityItem::Goal { id, .. } => id.clone(),
            };

            let item_height = self.get_activity_item_height(item, line_height, available_width);
            let item_start = current_y;
            let item_end = current_y + item_height;

            // Check if this item overlaps with the actual viewport
            let overlaps_viewport = item_end > viewport_start && item_start < viewport_end;

            // Enhanced debug logging for items near the viewport
            if idx < 3
                || idx >= filtered_items.len() - 3
                || (item_end >= viewport_start - 200.0 && item_start <= viewport_end + 200.0)
            {
                log::debug!(
                    "[VSCROLL] Item {} (idx {}): y={:.0}-{:.0} (h={:.0}), viewport={:.0}-{:.0}, overlaps={}",
                    item_id, idx, item_start, item_end, item_height, viewport_start, viewport_end, overlaps_viewport
                );
            }

            // Special logging for items we expect but don't see
            if idx >= 5 && idx <= 10 {
                log::debug!(
                    "[VSCROLL] DEBUG Item {} (idx {}): start={:.0}, end={:.0}, height={:.0}",
                    item_id,
                    idx,
                    item_start,
                    item_end,
                    item_height
                );
            }

            // Track height measurement for tall items using scroll positions
            if item_height >= available_height
                || !self
                    .height_trackers
                    .get(&item_id)
                    .map(|t| t.seen_full_height)
                    .unwrap_or(false)
            {
                let tracker = self.height_trackers.entry(item_id.clone()).or_default();

                // Track when top edge enters viewport
                if overlaps_viewport && tracker.top_entered_at.is_none() {
                    tracker.top_entered_at = Some(self.activity_log_scroll_offset);
                    log::debug!(
                        "Item {} top entered viewport at scroll offset {}",
                        item_id,
                        self.activity_log_scroll_offset
                    );
                }

                // Track when bottom edge becomes visible
                if tracker.top_entered_at.is_some() && item_end <= viewport_end {
                    if tracker.bottom_entered_at.is_none() {
                        tracker.bottom_entered_at = Some(self.activity_log_scroll_offset);

                        // Calculate height from scroll distance
                        let top_offset = tracker.top_entered_at.unwrap();
                        let bottom_offset = tracker.bottom_entered_at.unwrap();

                        // Scroll tracking is disabled - it was incorrectly adding viewport height
                        log::debug!(
                            "[VSCROLL] Scroll tracking DISABLED for {} - scroll_distance={:.0}px",
                            item_id,
                            bottom_offset - top_offset
                        );
                    }
                }
            }

            // Check if any part of the item overlaps with the actual viewport
            if item_end > viewport_start && item_start < viewport_end {
                if first_visible.is_none() {
                    first_visible = Some(idx);
                }
                last_visible = Some(idx);
            }

            current_y = item_end; // Use item_end to be consistent

            // Note: We used to have an optimization here to stop scanning early,
            // but it was causing issues with calculating total height and finding all items.
            // We need to scan all items to get accurate total height.
        }

        // Log the scan result with more detail
        let total_content_height = current_y;
        log::debug!(
            "[VSCROLL] Scan complete: total_height={:.0}, viewport={:.0}-{:.0}, first_visible={:?}, last_visible={:?}",
            total_content_height, viewport_start, viewport_end, first_visible, last_visible
        );

        // DEBUG: Check if we can theoretically scroll to see all content
        let theoretical_max_scroll = (total_content_height - available_height).max(0.0);
        if self.activity_log_scroll_offset > theoretical_max_scroll - 10.0 {
            log::info!(
                "[VSCROLL] Near bottom: scroll={:.0}, max={:.0}, last_item_bottom={:.0}, viewport_bottom={:.0}",
                self.activity_log_scroll_offset, theoretical_max_scroll, total_content_height,
                self.activity_log_scroll_offset + available_height
            );
        }

        // Apply consistent pixel-based buffer around the visible items

        let (start_idx, end_idx) = if let (Some(first), Some(last)) = (first_visible, last_visible)
        {
            // Find start index by going backwards from first_visible
            // Always include at least one item before visible range, even if it's very tall
            let mut start = first;
            let mut accumulated_before = 0.0;
            let mut items_before = 0;

            while start > 0 && (accumulated_before < RENDER_MARGIN || items_before == 0) {
                start -= 1;
                items_before += 1;
                if let Some((_, item)) = filtered_items.get(start) {
                    let item_height =
                        self.get_activity_item_height(item, line_height, available_width);
                    accumulated_before += item_height;
                }
            }

            // Find end index by going forward from last_visible
            // Always include at least one item after visible range, even if it's very tall
            let mut end = last + 1;
            let mut accumulated_after = 0.0;
            let mut items_after = 0;
            while end < filtered_items.len()
                && (accumulated_after < RENDER_MARGIN || items_after == 0)
            {
                if let Some((_, item)) = filtered_items.get(end) {
                    accumulated_after +=
                        self.get_activity_item_height(item, line_height, available_width);
                }
                end += 1;
                items_after += 1;
            }

            log::debug!(
                "[VSCROLL] Pixel-based buffer: {}..{} (first_vis={}, last_vis={}, before={:.0}px, after={:.0}px)",
                start, end, first, last, accumulated_before, accumulated_after
            );

            (start, end)
        } else {
            // This should never happen if our heights are correct
            log::error!(
                "[VSCROLL] CRITICAL: No visible items found! viewport={:.0}-{:.0}, total_height={:.0}, item_count={}",
                viewport_start, viewport_end, current_y, filtered_items.len()
            );

            // Handle empty list case
            if filtered_items.is_empty() {
                (0, 0)
            } else {
                // Just show first few items as a fallback
                let count = BUFFER_ITEMS.min(filtered_items.len());
                (0, count)
            }
        };

        self.activity_log_visible_range = start_idx..end_idx;

        // DEBUG: Enhanced visible range logging
        log::debug!(
            "[VSCROLL] Visible range: {:?} ({}..{}), First visible: {:?}, Last visible: {:?}",
            self.activity_log_visible_range,
            start_idx,
            end_idx,
            first_visible,
            last_visible
        );

        log::debug!(
            "[VSCROLL] Rendering {} items (indices {}..{}) of {} total, viewport: {:.0}-{:.0} pixels",
            end_idx - start_idx,
            start_idx,
            end_idx,
            filtered_items.len(),
            viewport_start,
            viewport_end
        );

        // Only render visible items
        let mut rendered_items: Vec<Element> = Vec::new();

        // Calculate Y offset for items before our render range
        // IMPORTANT: We need the offset to start_idx (what we're actually rendering),
        // not first_visible (what's in viewport), to position content correctly
        let mut y_offset_before_visible = 0.0;
        for idx in 0..start_idx {
            if let Some((orig_idx, item)) = filtered_items.get(idx) {
                y_offset_before_visible +=
                    self.get_activity_item_height(item, line_height, available_width);
            }
        }

        log::debug!(
            "[VSCROLL] y_offset_before_visible={:.0} (sum of {} items before start_idx={})",
            y_offset_before_visible,
            start_idx,
            start_idx
        );

        // Render visible items
        for idx in self.activity_log_visible_range.clone() {
            if let Some((orig_idx, item)) = filtered_items.get(idx) {
                let mut element = self.render_activity_item(item, fonts, *orig_idx, palette);

                // Attach cached height if available
                let item_id = match item {
                    ActivityItem::Command { id, .. } => id.clone(),
                    ActivityItem::Chat { id, .. } => id.clone(),
                    ActivityItem::Suggestion { id, .. } => id.clone(),
                    ActivityItem::Goal { id, .. } => id.clone(),
                };
                if let Some(height) = self.activity_log_height_cache.get(&item_id) {
                    element = element.with_computed_height(*height);
                }

                rendered_items.push(element);
            }
        }

        log::debug!(
            "[VSCROLL] Rendering {} visible items (of {} total), visible range: {:?}",
            rendered_items.len(),
            filtered_items.len(),
            self.activity_log_visible_range.clone()
        );

        // Calculate total content height
        let total_content_height =
            self.calculate_total_activity_log_height(&filtered_items, line_height, available_width);

        // Log height information
        log::debug!(
            "Total content height: {} pixels, scroll_offset: {}, max valid scroll: {}",
            total_content_height,
            self.activity_log_scroll_offset,
            (total_content_height - available_height).max(0.0)
        );

        // Before updating total height, calculate visual anchor if heights are changing
        let old_height = self
            .activity_log_scrollbar
            .as_ref()
            .map(|s| s.content_height)
            .unwrap_or(0.0);

        let height_changing = (old_height - total_content_height).abs() > 1.0;

        // TEMPORARILY DISABLED: Visual anchor system to fix scrolling jumps
        // if height_changing {
        //     // Calculate anchor before any changes
        //     self.visual_anchor = self.calculate_visual_anchor(
        //         &filtered_items,
        //         line_height,
        //         available_width,
        //         available_height
        //     );
        //
        //     log::info!(
        //         "Total content height changing: {} -> {} (delta: {}, cache size: {})",
        //         old_height,
        //         total_content_height,
        //         total_content_height - old_height,
        //         self.activity_log_height_cache.len()
        //     );
        // }

        // DEBUG: Log comprehensive state information
        log::debug!(
            "[VSCROLL] Total items: {}, Filtered: {}, Total height: {:.0}px, Scroll: {:.0}px, Viewport: {:.0}px, Max scroll: {:.0}px",
            self.activity_log.len(),
            filtered_items.len(),
            total_content_height,
            self.activity_log_scroll_offset,
            available_height,
            (total_content_height - available_height).max(0.0)
        );

        // Debug: Show what's at the end of the list
        if let Some((idx, last_item)) = filtered_items.last() {
            let last_height =
                self.get_activity_item_height(last_item, line_height, available_width);
            let last_id = match last_item {
                ActivityItem::Command { id, .. } => id,
                ActivityItem::Chat { id, .. } => id,
                ActivityItem::Suggestion { id, .. } => id,
                ActivityItem::Goal { id, .. } => id,
            };
            let is_cached = self.activity_log_height_cache.contains_key(last_id);
            log::debug!(
                "[VSCROLL] Last item: {} (idx={}, height={:.0}px, cached={}), can_reach_end={}",
                last_id,
                idx,
                last_height,
                is_cached,
                self.activity_log_scroll_offset + available_height >= total_content_height - 10.0
            );

            // Check last few items to see if they have cached heights
            let last_5_uncached = filtered_items
                .iter()
                .rev()
                .take(5)
                .filter(|(_, item)| {
                    let id = match item {
                        ActivityItem::Command { id, .. } => id,
                        ActivityItem::Chat { id, .. } => id,
                        ActivityItem::Suggestion { id, .. } => id,
                        ActivityItem::Goal { id, .. } => id,
                    };
                    !self.activity_log_height_cache.contains_key(id)
                })
                .count();
            if last_5_uncached > 0 {
                log::debug!(
                    "[VSCROLL] {} of last 5 items are using estimated heights (never been visible)",
                    last_5_uncached
                );

                // Show which specific items are uncached
                let uncached_info: Vec<String> = filtered_items
                    .iter()
                    .rev()
                    .take(5)
                    .filter_map(|(idx, item)| {
                        let id = match item {
                            ActivityItem::Command { id, .. } => id,
                            ActivityItem::Chat { id, .. } => id,
                            ActivityItem::Suggestion { id, .. } => id,
                            ActivityItem::Goal { id, .. } => id,
                        };
                        if !self.activity_log_height_cache.contains_key(id) {
                            Some(format!("{} (idx={})", id, idx))
                        } else {
                            None
                        }
                    })
                    .collect();
                log::debug!("[VSCROLL] Uncached items: {:?}", uncached_info);
            }
        }

        // Ensure scroll offset is within valid bounds
        let max_valid_scroll = (total_content_height - available_height).max(0.0);
        if self.activity_log_scroll_offset > max_valid_scroll {
            log::warn!(
                "Scroll offset {} exceeds max valid scroll {}, clamping",
                self.activity_log_scroll_offset,
                max_valid_scroll
            );
            self.activity_log_scroll_offset = max_valid_scroll;
        }

        // Update scrollbar state
        let scrollbar_info = ScrollbarInfo {
            should_show: total_content_height > available_height,
            thumb_position: if total_content_height > available_height {
                self.activity_log_scroll_offset / (total_content_height - available_height)
            } else {
                0.0
            },
            thumb_size: (available_height / total_content_height).min(1.0).max(0.1),
            content_height: total_content_height,
            viewport_height: available_height,
            scroll_offset: self.activity_log_scroll_offset,
            total_items: filtered_items.len(),
            viewport_items: self.activity_log_visible_range.len(),
        };

        self.activity_log_scrollbar = Some(scrollbar_info.clone());

        // Update scrollbar renderer
        if scrollbar_info.should_show {
            match &mut self.activity_log_scrollbar_renderer {
                Some(renderer) => {
                    renderer.update(
                        total_content_height,
                        available_height,
                        self.activity_log_scroll_offset,
                    );
                }
                None => {
                    self.activity_log_scrollbar_renderer = Some(ScrollbarRenderer::new_vertical(
                        total_content_height,
                        available_height,
                        self.activity_log_scroll_offset,
                        20.0, // min thumb size
                    ));
                }
            }
        } else {
            self.activity_log_scrollbar_renderer = None;
        }

        // Create scrollable container with only visible elements
        let margin_top = -self.activity_log_scroll_offset + y_offset_before_visible;
        log::debug!(
            "[VSCROLL] Content positioning: scroll_offset={:.0}, y_offset_before_visible={:.0}, margin_top={:.0}, start_idx={}, first_visible={:?}",
            self.activity_log_scroll_offset,
            y_offset_before_visible,
            margin_top,
            start_idx,
            first_visible
        );

        let content_area = Element::new(&fonts.body, ElementContent::Children(rendered_items))
            .display(DisplayType::Block)
            .margin(BoxDimension {
                top: Dimension::Pixels(margin_top),
                ..Default::default()
            });

        // Create viewport container with fixed height and clipping
        let viewport = Element::new(&fonts.body, ElementContent::Children(vec![content_area]))
            .display(DisplayType::Block)
            .min_height(Some(Dimension::Pixels(available_height)));

        // Log diagnostics when content might be invisible
        if margin_top < -5000.0 || self.activity_log_scroll_offset > total_content_height {
            log::debug!(
                "Potential visibility issue: margin_top={}, scroll_offset={}, total_height={}, viewport_height={}",
                margin_top,
                self.activity_log_scroll_offset,
                total_content_height,
                available_height
            );
        }

        viewport
    }

    /// Get the number of display lines for chat input
    pub fn get_chat_input_display_lines(&self) -> usize {
        self.chat_input.display_lines
    }

    /// Render just the text content of the chat input (to be clipped by scissor rect)
    fn render_chat_input_text(
        &mut self,
        font: &Rc<LoadedFont>,
        width: f32,
        viewport_height: f32,
    ) -> Element {
        let line_height = font.metrics().cell_height.get() as f32;
        // Use consistent 1.1x multiplier for line spacing
        // TODO: Extract line spacing multiplier (1.1) to a constant - used throughout codebase
        let line_height_with_spacing = line_height * 1.1;

        // Use visual line count if available, otherwise fall back to logical lines
        let line_count = if self.chat_input.visual_line_count > 0 {
            self.chat_input.visual_line_count
        } else {
            self.chat_input.lines.len()
        };
        let total_height = line_count as f32 * line_height_with_spacing;

        log::debug!(
            "render_chat_input_text: width={:.1}, viewport_height={:.1}, line_height={:.1}, line_height_with_spacing={:.1}, lines={}, focused={}",
            width, viewport_height, line_height, line_height_with_spacing, self.chat_input.lines.len(), self.chat_input.focused
        );

        // Calculate scroll position
        let max_scroll = (total_height - viewport_height).max(0.0);
        let scroll_offset = if !self.chat_input.user_has_scrolled {
            // Auto-scroll to bottom when typing
            max_scroll
        } else {
            self.chat_input.scroll_pixel_offset.min(max_scroll)
        };

        // Update scroll offset
        self.chat_input.scroll_pixel_offset = scroll_offset;

        log::debug!(
            "Chat input scroll state: offset={:.1}, max={:.1}, lines={}, viewport_lines={}",
            scroll_offset,
            max_scroll,
            self.chat_input.lines.len(),
            self.chat_input.display_lines
        );

        // Combine all lines into a single text string with newlines
        let mut combined_text = String::new();
        let is_placeholder = self.chat_input.lines.len() == 1
            && self.chat_input.lines[0].is_empty()
            && !self.chat_input.focused;

        if is_placeholder {
            combined_text = self.chat_input.placeholder.clone();
        } else {
            for (idx, line) in self.chat_input.lines.iter().enumerate() {
                if idx > 0 {
                    combined_text.push('\n');
                }
                combined_text.push_str(line);
            }
        }

        let text_color = if is_placeholder {
            LinearRgba::with_components(0.5, 0.5, 0.5, 1.0) // Gray for placeholder
        } else {
            LinearRgba::with_components(0.9, 0.9, 0.9, 1.0) // Light gray for typed text
        };

        // Create a single WrappedText element containing all lines
        Element::new(font, ElementContent::WrappedText(combined_text))
            .colors(ElementColors {
                text: text_color.into(),
                bg: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0).into(), // Transparent background
                ..Default::default()
            })
            .max_width(Some(Dimension::Pixels(width)))
            .display(DisplayType::Block)
            // Use negative margin to implement scrolling
            .margin(BoxDimension {
                top: Dimension::Pixels(-scroll_offset),
                ..Default::default()
            })
        // DO NOT add UIItemType here - clicks should be handled by the container at z-index 14
        // DO NOT set zindex here - it's set via LayoutContext during compute_element
    }

    fn render_chat_input(&mut self, fonts: &SidebarFonts) -> Element {
        // Use activity log pattern: background rendered separately as filled rectangle
        let send_button_width = 60.0;
        let horizontal_padding = 32.0; // 16px left + 16px right
        let spacing = 8.0;
        let input_width = self.width as f32 - horizontal_padding - send_button_width - spacing;

        // Calculate dimensions
        let line_height = fonts.body.metrics().cell_height.get() as f32;
        // Add 10% for line spacing to accommodate 2 full lines
        let viewport_height = self.chat_input.display_lines as f32 * line_height * 1.1;
        let container_height = viewport_height + 14.0; // +14 for padding

        // Colors are set during focus changes, not during render

        // Initialize empty line_positions - these will be populated with exact glyph positions
        // during compute_element when track_cluster=true for sidebar text
        let line_positions = Vec::new();

        // Create a container with proper bounds for mouse event handling
        // Use an empty Children element instead of empty text to ensure proper dimensions
        let input_container = Element::new(&fonts.body, ElementContent::Children(vec![]))
            .display(DisplayType::Block)
            .min_width(Some(Dimension::Pixels(input_width)))
            .max_width(Some(Dimension::Pixels(input_width)))
            .min_height(Some(Dimension::Pixels(container_height)))
            .colors(ElementColors {
                // Transparent background to ensure the element has bounds
                bg: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0).into(),
                ..Default::default()
            })
            .item_type(UIItemType::ChatInput { line_positions })
            .zindex(14); // Container for mouse event handling

        // Send button
        let send_button = Chip::new("Send".to_string())
            .with_style(ChipStyle::Primary)
            .with_size(ChipSize::Medium)
            .clickable(true)
            .render(&fonts.body)
            .zindex(14);

        // Create horizontal layout
        Element::new(
            &fonts.body,
            ElementContent::Children(vec![
                input_container,
                send_button.margin(BoxDimension {
                    left: Dimension::Pixels(spacing),
                    ..Default::default()
                }),
            ]),
        )
        .display(DisplayType::Block)
        .padding(BoxDimension {
            left: Dimension::Pixels(16.0),
            right: Dimension::Pixels(16.0),
            top: Dimension::Pixels(8.0),
            bottom: Dimension::Pixels(16.0),
        })
        .zindex(14)
    }

    /// Render the chat input text content (to be rendered with scissor rect)
    pub fn render_chat_input_content(&mut self, fonts: &SidebarFonts, width: f32) -> Element {
        // Calculate dimensions
        let line_height = fonts.body.metrics().cell_height.get() as f32;
        // Add 10% for line spacing to match scissor rect height
        let viewport_height = self.chat_input.display_lines as f32 * line_height * 1.1;

        // Return the text content directly (no wrapper needed)
        self.render_chat_input_text(&fonts.body, width, viewport_height)
    }

    /// Calculate cursor position for rendering
    pub fn get_cursor_position(&self, font: &Rc<LoadedFont>) -> Option<(f32, f32)> {
        if !self.chat_input.focused {
            return None;
        }
        let line_height = font.metrics().cell_height.get() as f32;
        let line_height_with_spacing = line_height * 1.1;

        // First, calculate the document byte offset for the cursor position
        let mut cursor_document_byte_offset = 0;

        // Add bytes from all lines before the cursor line
        for (line_idx, line) in self.chat_input.lines.iter().enumerate() {
            if line_idx < self.chat_input.cursor_line {
                cursor_document_byte_offset += line.len() + 1; // +1 for newline
            } else if line_idx == self.chat_input.cursor_line {
                // Add bytes up to cursor column in current line
                let byte_in_line = line
                    .char_indices()
                    .nth(self.chat_input.cursor_col)
                    .map(|(idx, _)| idx)
                    .unwrap_or(line.len());
                cursor_document_byte_offset += byte_in_line;
                break;
            }
        }

        log::debug!(
            "Cursor at logical line {}, col {} = document byte offset {}, glyph_positions available: {}, visual_lines={}",
            self.chat_input.cursor_line,
            self.chat_input.cursor_col,
            cursor_document_byte_offset,
            !self.chat_input.exact_glyph_positions.is_empty(),
            self.chat_input.exact_glyph_positions.len()
        );

        // Now find which visual line contains this byte offset
        let mut visual_line_idx = None;
        let mut cursor_x = 0.0;

        for (vis_line_idx, visual_line_positions) in
            self.chat_input.exact_glyph_positions.iter().enumerate()
        {
            if visual_line_positions.is_empty() {
                continue;
            }

            // Check if this visual line contains our byte offset
            let first_byte = visual_line_positions
                .first()
                .map(|(_, _, b)| *b)
                .unwrap_or(0);
            let last_byte = visual_line_positions
                .last()
                .map(|(_, _, b)| *b)
                .unwrap_or(0);

            log::debug!(
                "Visual line {}: byte range {} - {}, cursor looking for {}, contains={}",
                vis_line_idx,
                first_byte,
                last_byte,
                cursor_document_byte_offset,
                cursor_document_byte_offset >= first_byte
                    && cursor_document_byte_offset <= last_byte + 1
            );

            if cursor_document_byte_offset >= first_byte
                && cursor_document_byte_offset <= last_byte + 1
            {
                // Found the visual line containing our cursor
                visual_line_idx = Some(vis_line_idx);

                // Find X position within this visual line
                if cursor_document_byte_offset == first_byte {
                    // Cursor at start of visual line
                    cursor_x = 0.0;
                } else {
                    // Find position within line
                    let mut found = false;
                    let mut prev_end = 0.0;

                    for (x_start, x_end, byte_offset) in visual_line_positions.iter() {
                        if *byte_offset == cursor_document_byte_offset {
                            // Cursor is exactly at this glyph's position - put it at the start
                            cursor_x = *x_start;
                            found = true;
                            break;
                        } else if *byte_offset > cursor_document_byte_offset {
                            // We've passed the cursor position - use the end of the previous glyph
                            cursor_x = prev_end;
                            found = true;
                            break;
                        }
                        prev_end = *x_end;
                    }

                    // If we didn't find a position (cursor at end of line), use the last glyph's end
                    if !found && !visual_line_positions.is_empty() {
                        cursor_x = visual_line_positions.last().unwrap().1;
                    }
                }

                break;
            }
        }

        // Calculate Y position based on visual line index
        let y = if let Some(vis_line_idx) = visual_line_idx {
            vis_line_idx as f32 * line_height_with_spacing - self.chat_input.scroll_pixel_offset
        } else {
            // Fallback: use logical line if we couldn't find visual line
            log::warn!(
                "Could not find visual line for cursor at byte offset {}, positions_count={}, visual_lines={}",
                cursor_document_byte_offset,
                self.chat_input.exact_glyph_positions.len(),
                self.chat_input.visual_line_count
            );
            self.chat_input.cursor_line as f32 * line_height_with_spacing
                - self.chat_input.scroll_pixel_offset
        };

        // Use exact positions if we found them, otherwise fallback
        let x = if visual_line_idx.is_some() {
            cursor_x
        } else {
            // Fallback to character width estimation if exact positions not available
            let line = &self.chat_input.lines[self.chat_input.cursor_line];
            let text_before_cursor: String =
                line.chars().take(self.chat_input.cursor_col).collect();
            if text_before_cursor.is_empty() {
                0.0
            } else {
                // Use font metrics to estimate character widths
                let base_char_width = font.metrics().cell_width.get() as f32;
                let char_width = base_char_width * 0.5; // Proportional font adjustment
                let mut x_pos = 0.0;
                for ch in text_before_cursor.chars() {
                    // Character width estimation
                    let ch_width = if ch.is_ascii_alphabetic() {
                        if ch.is_ascii_uppercase() {
                            char_width * 1.2 // Uppercase letters are wider
                        } else {
                            char_width // Lowercase letters
                        }
                    } else if ch.is_ascii_digit() {
                        char_width * 0.9 // Numbers are slightly narrower
                    } else if ch == ' ' {
                        char_width * 0.4 // Spaces are narrow
                    } else if ch.is_ascii_punctuation() {
                        char_width * 0.5 // Punctuation varies
                    } else {
                        char_width // Default for other characters
                    };
                    x_pos += ch_width;
                }
                x_pos
            }
        };

        log::debug!(
            "Cursor position: line={}, col={}, x={:.1}, y={:.1}, visual_line={:?}, exact_positions_available={}",
            self.chat_input.cursor_line,
            self.chat_input.cursor_col,
            x,
            y,
            visual_line_idx,
            !self.chat_input.exact_glyph_positions.is_empty()
        );

        Some((x, y))
    }
    /// Get chat input background color for filled rectangle rendering
    pub fn get_chat_input_bg_color(&self) -> LinearRgba {
        self.chat_input_bg_color
    }

    /// Get chat input bounds for scrollbar positioning
    pub fn get_chat_input_bounds(&self) -> Option<euclid::Rect<f32, window::PixelUnit>> {
        self.chat_input_bounds.clone()
    }

    /// Set chat input bounds for scrollbar positioning
    pub fn set_chat_input_bounds(&mut self, bounds: euclid::Rect<f32, window::PixelUnit>) {
        self.chat_input_bounds = Some(bounds);
    }

    /// Set activity item bounds for selection rendering
    pub fn set_activity_item_bounds(
        &mut self,
        index: usize,
        bounds: euclid::Rect<f32, window::PixelUnit>,
    ) {
        self.activity_item_bounds.insert(index, bounds);
    }

    /// Store position data for an activity item
    pub fn store_item_positions(
        &mut self,
        index: usize,
        position_data: crate::sidebar::position_cache::ItemPositionData,
    ) {
        self.item_positions.insert(index, position_data);
    }

    /// Update coordinate transform for the sidebar
    pub fn update_coordinate_transform(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.coordinate_transform = crate::sidebar::position_cache::CoordinateTransform {
            sidebar_x: x,
            sidebar_y: y,
            sidebar_width: width,
            sidebar_height: height,
        };
    }

    /// Perform hierarchical hit testing on activity log items
    pub fn hit_test_activity_log(
        &self,
        window_point: euclid::Point2D<f32, window::PixelUnit>,
    ) -> Option<crate::sidebar::position_cache::HitResult> {
        use crate::sidebar::position_cache::{ItemCoord, ViewportCoord, WindowCoord};

        // 1. Window → Viewport transformation
        let viewport_point = self
            .coordinate_transform
            .window_to_viewport(WindowCoord(window_point));

        // 2. Find which item was hit
        for (index, item_data) in &self.item_positions {
            if let Some(item_viewport_y) = item_data.viewport_y {
                // Check if point is within item bounds vertically
                let item_height = item_data.position_tree.bounds.size.height;
                if viewport_point.0.y >= item_viewport_y
                    && viewport_point.0.y < item_viewport_y + item_height
                {
                    // 3. Viewport → Item transformation
                    let item_point = self
                        .coordinate_transform
                        .viewport_to_item(viewport_point, item_viewport_y);

                    // 4. Hit test within item (item-relative coordinates)
                    if let Some(position) = self.hit_test_item(&item_data.position_tree, item_point)
                    {
                        // Validate the position if we have access to the item's text
                        if let Some(item) = self.activity_log.get(*index) {
                            let text = match item {
                                ActivityItem::Chat { message, .. } => message,
                                ActivityItem::Command { command, output, .. } => output.as_deref().unwrap_or(command),
                                ActivityItem::Suggestion { content, .. } => content,
                                ActivityItem::Goal { text, .. } => text,
                            };
                            if let Err(e) = position.validate(text) {
                                log::warn!("Invalid position from hit test: {}", e);
                                continue; // Skip this invalid position
                            }
                        }
                        
                        return Some(crate::sidebar::position_cache::HitResult {
                            item_index: *index,
                            position_in_item: position,
                        });
                    }
                }
            }
        }
        None
    }

    /// Hit test within a position tree
    fn hit_test_item(
        &self,
        position_tree: &crate::sidebar::position_cache::PositionTree,
        point: crate::sidebar::position_cache::ItemCoord,
    ) -> Option<crate::sidebar::position_cache::ItemPosition> {
        use crate::sidebar::position_cache::{ElementType, ItemPosition, TextAffinity};

        // Check if point is within this element's bounds
        if !position_tree.bounds.contains(point.0) {
            return None;
        }

        // Transform to element-relative coordinates
        let element_point = self
            .coordinate_transform
            .item_to_element(point, &position_tree.bounds);

        // Handle different element types
        match &position_tree.element_type {
            ElementType::CodeBlock { padding, .. } => {
                // Adjust for code block padding
                let adjusted = element_point.0 - euclid::Vector2D::new(*padding, *padding);
                self.hit_test_text_positions(
                    &position_tree.text_positions,
                    euclid::Point2D::new(adjusted.x, adjusted.y),
                    &position_tree.element_type,
                )
            }
            ElementType::ListItem {
                indent,
                marker_width,
                ..
            } => {
                // Account for list indentation and marker
                let adjusted = element_point.0 - euclid::Vector2D::new(indent + marker_width, 0.0);
                // First check children (list item content)
                for child in &position_tree.children {
                    if let Some(hit) = self.hit_test_item(child, point) {
                        return Some(hit);
                    }
                }
                // Then check own text
                self.hit_test_text_positions(
                    &position_tree.text_positions,
                    euclid::Point2D::new(adjusted.x, adjusted.y),
                    &position_tree.element_type,
                )
            }
            ElementType::InlineCode { padding, .. } => {
                // Handle inline code padding
                let adjusted = element_point.0 - euclid::Vector2D::new(*padding, 0.0);
                self.hit_test_text_positions(
                    &position_tree.text_positions,
                    euclid::Point2D::new(adjusted.x, adjusted.y),
                    &position_tree.element_type,
                )
            }
            _ => {
                // For other elements, check children first
                for child in &position_tree.children {
                    if let Some(hit) = self.hit_test_item(child, point) {
                        return Some(hit);
                    }
                }
                // Then check own text positions
                self.hit_test_text_positions(
                    &position_tree.text_positions,
                    element_point.0,
                    &position_tree.element_type,
                )
            }
        }
    }

    /// Hit test within text positions
    fn hit_test_text_positions(
        &self,
        positions: &[crate::sidebar::position_cache::TextPosition],
        point: euclid::Point2D<f32, window::PixelUnit>,
        element_type: &crate::sidebar::position_cache::ElementType,
    ) -> Option<crate::sidebar::position_cache::ItemPosition> {
        use crate::sidebar::position_cache::{ItemPosition, TextAffinity};

        // Find the line containing the y coordinate
        let line_positions: Vec<_> = positions
            .iter()
            .filter(|p| {
                // Check if point is within line height
                // Use a reasonable default that matches our text rendering
                let line_height = match element_type {
                    ElementType::Paragraph { line_height, .. } => *line_height,
                    ElementType::Heading { font_size, .. } => font_size * 1.2,
                    ElementType::CodeBlock { line_height, .. } => *line_height,
                    _ => 20.0, // Default line height
                };
                point.y >= p.y && point.y < p.y + line_height
            })
            .collect();

        if line_positions.is_empty() {
            return None;
        }

        // Find the closest position on the line
        let mut best_position = None;
        let mut best_distance = f32::MAX;

        for pos in &line_positions {
            if point.x >= pos.x_start && point.x < pos.x_end {
                // Point is within this glyph
                let mid = (pos.x_start + pos.x_end) / 2.0;
                if point.x < mid {
                    return Some(ItemPosition {
                        byte_offset: pos.byte_offset,
                        affinity: TextAffinity::Leading,
                    });
                } else {
                    // Find the next position if available
                    let next_offset = line_positions
                        .iter()
                        .find(|p| p.byte_offset > pos.byte_offset)
                        .map(|p| p.byte_offset)
                        .unwrap_or(pos.byte_offset + 1); // Approximate
                    return Some(ItemPosition {
                        byte_offset: next_offset,
                        affinity: TextAffinity::Leading,
                    });
                }
            }

            // Track closest position for edge cases
            let dist_to_start = (point.x - pos.x_start).abs();
            let dist_to_end = (point.x - pos.x_end).abs();

            if dist_to_start < best_distance {
                best_distance = dist_to_start;
                best_position = Some(ItemPosition {
                    byte_offset: pos.byte_offset,
                    affinity: TextAffinity::Leading,
                });
            }

            if dist_to_end < best_distance {
                best_distance = dist_to_end;
                // Use next byte offset if available
                let next_offset = line_positions
                    .iter()
                    .find(|p| p.byte_offset > pos.byte_offset)
                    .map(|p| p.byte_offset)
                    .unwrap_or(pos.byte_offset + 1);
                best_position = Some(ItemPosition {
                    byte_offset: next_offset,
                    affinity: TextAffinity::Leading,
                });
            }
        }

        best_position
    }

    /// Get chat input border color
    pub fn get_chat_input_border_color(&self) -> LinearRgba {
        self.chat_input_border_color
    }

    /// Update chat input with exact glyph positions from rendering
    pub fn set_chat_input_glyph_positions(&mut self, positions: Vec<Vec<(f32, f32, usize)>>) {
        // Update visual line count based on actual wrapped lines
        self.chat_input.visual_line_count = positions.len();
        self.chat_input.exact_glyph_positions = positions;
    }

    /// Get the exact glyph positions for chat input
    pub fn get_chat_input_glyph_positions(&self) -> &Vec<Vec<(f32, f32, usize)>> {
        &self.chat_input.exact_glyph_positions
    }

    /// Get the active selection if any
    pub fn get_active_selection(&self) -> Option<&SelectionTarget> {
        self.selection_state.active_selection.as_ref()
    }

    /// Calculate selection rectangles for the given selection target
    pub fn calculate_selection_rectangles(
        &self,
        selection: &SelectionTarget,
    ) -> Vec<euclid::Rect<f32, window::PixelUnit>> {
        let mut rects = Vec::new();

        match selection {
            SelectionTarget::ActivityItem {
                anchor_index,
                anchor_byte,
                current_index,
                current_byte,
            } => {
                // TODO: Implement multi-item selection rendering
                // For now, only render selection for single item
                if anchor_index == current_index {
                    // Get the activity item bounds
                    if let Some(bounds) = self.activity_item_bounds.get(anchor_index) {
                        if let Some(item) = self.activity_log.get(*anchor_index) {
                            let start_byte = anchor_byte.min(current_byte);
                            let end_byte = anchor_byte.max(current_byte);

                        // TEMPORARY: Show selection even for zero-width (for debugging)
                        if start_byte == end_byte {}

                        // Always show selection rectangle for debugging (was: if start_byte != end_byte)
                        if true {
                            // For activity items, we need better selection rectangle calculation
                            let line_height = 20.0; // Approximate line height

                            // Get the message text to estimate selection position
                            let text = match item {
                                ActivityItem::Chat { message, .. } => message,
                                ActivityItem::Command { command, .. } => command,
                                ActivityItem::Suggestion { content, .. } => content,
                                ActivityItem::Goal { text, .. } => text,
                            };

                            // Calculate more accurate selection rectangles
                            // Account for padding inside the activity item
                            let padding = if matches!(item, ActivityItem::Chat { .. }) {
                                CHAT_ITEM_PADDING
                            } else {
                                8.0 // Default card padding
                            };

                            // For now, use character-based approximation
                            // TODO: Use exact glyph positions when available
                            let char_width = 8.5; // Approximate character width

                            // Calculate approximate x positions for selection
                            let start_char = text.chars().take(*start_byte).count();
                            let end_char = text.chars().take(*end_byte).count();

                            let start_x =
                                bounds.origin.x + padding + (start_char as f32 * char_width);
                            let mut end_x =
                                bounds.origin.x + padding + (end_char as f32 * char_width);

                            // Ensure we don't exceed the bounds
                            let max_x = bounds.origin.x + bounds.size.width - padding;
                            end_x = end_x.min(max_x);

                            // For zero-width selections, show a cursor-width rectangle
                            let width = if start_byte == end_byte {
                                2.0 // Cursor width
                            } else {
                                end_x - start_x
                            };

                            let rect = euclid::rect(
                                start_x,
                                bounds.origin.y + padding,
                                width,
                                line_height,
                            );

                            // log::debug!("SELECTION DEBUG: Creating rectangle at x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                            //     rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);

                            rects.push(rect);

                            log::debug!(
                                "Created selection rectangle for activity item {}: x={}, y={}, w={}, h={}, start_char={}, end_char={}",
                                anchor_index,
                                start_x,
                                bounds.origin.y + padding,
                                end_x - start_x,
                                line_height,
                                start_char,
                                end_char
                            );
                        }
                    }
                    } else {
                        log::debug!("No bounds found for activity item {}", anchor_index);
                    }
                }
            }
            SelectionTarget::Suggestion {
                anchor_byte,
                current_byte,
            } => {
                if let Some(bounds) = &self.suggestion_bounds {
                    let start = anchor_byte.min(current_byte);
                    let end = anchor_byte.max(current_byte);
                    if start != end {
                        if let Some(suggestion) = self.get_current_suggestion() {
                            // Calculate more accurate selection rectangles
                            let padding = 8.0; // Suggestion card padding
                            let char_width = 8.5; // Approximate character width
                            let line_height = 20.0;

                            // Calculate character positions
                            let start_char = suggestion.content.chars().take(*start).count();
                            let end_char = suggestion.content.chars().take(*end).count();

                            let start_x =
                                bounds.origin.x + padding + (start_char as f32 * char_width);
                            let end_x = bounds.origin.x + padding + (end_char as f32 * char_width);

                            // Ensure we don't exceed bounds
                            let max_x = bounds.origin.x + bounds.size.width - padding;
                            let end_x = end_x.min(max_x);

                            rects.push(euclid::rect(
                                start_x,
                                bounds.origin.y + padding,
                                end_x - start_x,
                                line_height,
                            ));
                        }
                    }
                }
            }
            SelectionTarget::Goal {
                anchor_byte,
                current_byte,
            } => {
                // log::debug!("SELECTION DEBUG: Goal selection - bounds available={}",
                //     self.goal_bounds.is_some());

                if let Some(bounds) = &self.goal_bounds {
                    // log::debug!("SELECTION DEBUG: Goal bounds: x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                    //     bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height);

                    let start = anchor_byte.min(current_byte);
                    let end = anchor_byte.max(current_byte);

                    // TEMPORARY: Show selection even for zero-width (for debugging)
                    if start == end {
                        // log::debug!("SELECTION DEBUG: Zero-width goal selection at byte {}", start);
                    }

                    // Always show selection rectangle for debugging (was: if start != end)
                    if true {
                        if let Some(goal) = &self.current_goal {
                            // Use actual glyph positions if available
                            let (start_x, end_x) = if let Some(positions) =
                                &self.goal_char_positions
                            {
                                // Find x positions for the byte offsets
                                let mut start_x_pos = bounds.origin.x + GOAL_CARD_PADDING;
                                let mut end_x_pos = start_x_pos;

                                // Find start position
                                for &(x_start, _, byte_offset) in positions {
                                    if byte_offset == *start {
                                        start_x_pos = bounds.origin.x + GOAL_CARD_PADDING + x_start;
                                        break;
                                    }
                                }

                                // Find end position
                                if start == end {
                                    end_x_pos = start_x_pos;
                                } else {
                                    // Look for the position just before the end byte
                                    for &(x_start, x_end, byte_offset) in positions {
                                        if byte_offset < *end {
                                            // This character is included in the selection
                                            end_x_pos = bounds.origin.x + GOAL_CARD_PADDING + x_end;
                                        }
                                    }
                                }

                                (start_x_pos, end_x_pos)
                            } else {
                                // Fallback to character-based calculation
                                let start_char = goal.text.chars().take(*start).count();
                                let end_char = goal.text.chars().take(*end).count();

                                let start_x = bounds.origin.x
                                    + GOAL_CARD_PADDING
                                    + (start_char as f32 * SELECTION_CHAR_WIDTH);
                                let end_x = bounds.origin.x
                                    + GOAL_CARD_PADDING
                                    + (end_char as f32 * SELECTION_CHAR_WIDTH);

                                (start_x, end_x)
                            };

                            // Ensure we don't exceed bounds
                            let max_x = bounds.origin.x + bounds.size.width - GOAL_CARD_PADDING;
                            let end_x = end_x.min(max_x);

                            // For zero-width selections, show a cursor-width rectangle
                            let width = if start == end {
                                SELECTION_CURSOR_WIDTH
                            } else {
                                end_x - start_x
                            };

                            let rect = euclid::rect(
                                start_x,
                                bounds.origin.y + GOAL_CARD_PADDING + SELECTION_VERTICAL_OFFSET,
                                width,
                                SELECTION_LINE_HEIGHT,
                            );

                            // log::debug!("SELECTION DEBUG: Creating goal rectangle at x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                            //     rect.origin.x, rect.origin.y, rect.size.width, rect.size.height);

                            rects.push(rect);
                        }
                    }
                } else {
                    // log::warn!("SELECTION DEBUG: No goal bounds available!");
                }
            }
            SelectionTarget::ChatInput {
                anchor_line,
                anchor_byte,
                current_line,
                current_byte,
            } => {
                // log::debug!("SELECTION DEBUG: ChatInput selection - bounds available={}, exact_positions count={}",
                //     self.chat_input_bounds.is_some(),
                //     self.chat_input.exact_glyph_positions.len());

                if let Some(bounds) = &self.chat_input_bounds {
                    // log::debug!("SELECTION DEBUG: ChatInput bounds: x={:.1}, y={:.1}, w={:.1}, h={:.1}",
                    //     bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height);
                    if !self.chat_input.exact_glyph_positions.is_empty() {
                        // Calculate selection rectangles for each line
                        let start_line = anchor_line.min(current_line);
                        let end_line = anchor_line.max(current_line);

                        let line_height = 20.0; // TODO: Get from font metrics
                        let line_height_with_spacing = line_height * 1.1;

                        // Account for padding and border
                        let text_padding = 8.0;
                        let border_thickness = 1.0;
                        let vertical_padding = 6.0;
                        let text_offset_x = border_thickness + text_padding;
                        let text_offset_y = border_thickness + vertical_padding;

                        log::debug!(
                            "ChatInput selection: bounds={:?}, start_line={}, end_line={}",
                            bounds,
                            start_line,
                            end_line
                        );

                        for line_idx in *start_line..=*end_line {
                            if line_idx < self.chat_input.exact_glyph_positions.len()
                                && line_idx < self.chat_input.lines.len()
                            {
                                let line_positions =
                                    &self.chat_input.exact_glyph_positions[line_idx];
                                let line_text = &self.chat_input.lines[line_idx];

                                log::debug!(
                                    "Line {}: positions count={}, text len={}",
                                    line_idx,
                                    line_positions.len(),
                                    line_text.len()
                                );

                                // Calculate x range for this line
                                let (x_start, x_end) =
                                    if line_idx == *start_line && line_idx == *end_line {
                                        // Selection within single line
                                        let start_byte = if *anchor_line == *start_line {
                                            *anchor_byte
                                        } else {
                                            *current_byte
                                        };
                                        let end_byte = if *anchor_line == *start_line {
                                            *current_byte
                                        } else {
                                            *anchor_byte
                                        };
                                        let min_byte = start_byte.min(end_byte);
                                        let max_byte = start_byte.max(end_byte);

                                        log::debug!(
                                            "Single line selection: min_byte={}, max_byte={}",
                                            min_byte,
                                            max_byte
                                        );

                                        // For chat input, byte offsets are character indices
                                        let x_start = self
                                            .find_x_for_byte_offset(&line_positions, min_byte)
                                            .unwrap_or(0.0);
                                        let x_end = self
                                            .find_x_for_byte_offset(&line_positions, max_byte)
                                            .unwrap_or_else(|| {
                                                // If we can't find the position, use the last glyph's end position
                                                line_positions
                                                    .last()
                                                    .map(|(_, end, _)| *end)
                                                    .unwrap_or(100.0)
                                            });
                                        (x_start, x_end)
                                    } else if line_idx == *start_line {
                                        // First line of multi-line selection
                                        let start_byte = if *anchor_line == *start_line {
                                            *anchor_byte
                                        } else {
                                            *current_byte
                                        };
                                        let x_start = self
                                            .find_x_for_byte_offset(&line_positions, start_byte)
                                            .unwrap_or(0.0);
                                        let x_end = line_positions
                                            .last()
                                            .map(|(_, end, _)| *end)
                                            .unwrap_or(100.0);
                                        (x_start, x_end)
                                    } else if line_idx == *end_line {
                                        // Last line of multi-line selection
                                        let end_byte = if *anchor_line == *end_line {
                                            *anchor_byte
                                        } else {
                                            *current_byte
                                        };
                                        let x_end = self
                                            .find_x_for_byte_offset(&line_positions, end_byte)
                                            .unwrap_or_else(|| {
                                                line_positions
                                                    .last()
                                                    .map(|(_, end, _)| *end)
                                                    .unwrap_or(100.0)
                                            });
                                        (0.0, x_end)
                                    } else {
                                        // Middle line - select entire line
                                        let x_end = line_positions
                                            .last()
                                            .map(|(_, end, _)| *end)
                                            .unwrap_or(100.0);
                                        (0.0, x_end)
                                    };

                                log::debug!(
                                    "Line {} selection x_start={}, x_end={}",
                                    line_idx,
                                    x_start,
                                    x_end
                                );

                                // Adjust for scroll offset
                                let y_offset = line_idx as f32 * line_height_with_spacing
                                    - self.chat_input.scroll_pixel_offset;

                                // Only add rectangle if it's visible
                                if y_offset + line_height > 0.0 && y_offset < bounds.size.height {
                                    let rect = euclid::rect(
                                        bounds.origin.x + text_offset_x + x_start,
                                        bounds.origin.y + text_offset_y + y_offset,
                                        x_end - x_start,
                                        line_height,
                                    );
                                    log::debug!("Adding selection rect: {:?}", rect);
                                    rects.push(rect);
                                }
                            }
                        }
                    }
                }
            }
        }

        rects
    }

    /// Find x-coordinate for a byte offset in glyph positions
    fn find_x_for_byte_offset(
        &self,
        positions: &[(f32, f32, usize)],
        byte_offset: usize,
    ) -> Option<f32> {
        log::debug!(
            "find_x_for_byte_offset: looking for byte_offset={}",
            byte_offset
        );

        // Handle start of line
        if byte_offset == 0 && !positions.is_empty() {
            // Return the start position of the first glyph
            return Some(positions[0].0);
        }

        // For chat input, byte_offset is actually a character index
        // The positions are stored as (x_start, x_end, char_index)
        // Find the position that matches this character index
        for (i, (x_start, x_end, char_idx)) in positions.iter().enumerate() {
            log::debug!(
                "  Position[{}]: x=({:.1}, {:.1}), char_idx={}",
                i,
                x_start,
                x_end,
                char_idx
            );

            if *char_idx == byte_offset {
                // Exact match - return start of this character
                return Some(*x_start);
            } else if byte_offset > 0 && i > 0 && *char_idx > byte_offset {
                // We've passed the target position - it's between previous and current
                let prev = &positions[i - 1];
                // Return end of previous character
                return Some(prev.1);
            }
        }

        // If byte offset is past all glyphs, return end of last glyph
        if let Some(last) = positions.last() {
            if byte_offset >= last.2 {
                return Some(last.1); // End of last character
            }
        }

        log::debug!(
            "  No matching position found for byte_offset={}",
            byte_offset
        );
        None
    }

    /// Set the bounds for the suggestion card
    pub fn set_suggestion_bounds(&mut self, bounds: euclid::Rect<f32, window::PixelUnit>) {
        self.suggestion_bounds = Some(bounds);
    }

    /// Set the bounds for the goal card
    pub fn set_goal_bounds(&mut self, bounds: euclid::Rect<f32, window::PixelUnit>) {
        self.goal_bounds = Some(bounds);
    }

    /// Get the bounds for the goal card
    pub fn get_goal_bounds(&self) -> Option<&euclid::Rect<f32, window::PixelUnit>> {
        self.goal_bounds.as_ref()
    }

    /// Store extracted character positions for goal text
    pub fn store_goal_positions(&mut self, positions: Vec<(f32, f32, usize)>) {
        self.goal_char_positions = Some(positions);
    }

    /// Get the stored goal character positions
    pub fn get_goal_positions(&self) -> Option<&Vec<(f32, f32, usize)>> {
        self.goal_char_positions.as_ref()
    }

    /// Find byte offset from x coordinate using stored character positions
    fn find_byte_offset_from_positions(&self, x: f32, positions: &[(f32, f32, usize)]) -> usize {
        // Handle click before first character
        if x <= 0.0 || positions.is_empty() {
            // log::debug!("SELECTION DEBUG: find_byte_offset x={} returning 0 (before first char)", x);
            return 0;
        }

        // Log position range for debugging
        if let (Some(first), Some(last)) = (positions.first(), positions.last()) {
            // log::debug!("SELECTION DEBUG: find_byte_offset x={}, positions range: x={}..{}, bytes={}..{}",
            //     x, first.0, last.1, first.2, last.2);
        }

        // Find the character containing this x position
        for (idx, &(x_start, x_end, byte_offset)) in positions.iter().enumerate() {
            if x >= x_start && x < x_end {
                // Determine if click is closer to start or end of character
                let mid = (x_start + x_end) / 2.0;
                if x < mid {
                    // log::debug!("SELECTION DEBUG: x={} in char {} ({}..{}), closer to start, returning byte={}",
                    //     x, idx, x_start, x_end, byte_offset);
                    return byte_offset;
                } else {
                    // Return position after this character
                    // Find the next character's byte offset
                    if idx + 1 < positions.len() {
                        let next_byte = positions[idx + 1].2;
                        // log::debug!("SELECTION DEBUG: x={} in char {} ({}..{}), closer to end, returning next byte={}",
                        //     x, idx, x_start, x_end, next_byte);
                        return next_byte;
                    } else {
                        // This is the last character - need to calculate proper end position
                        // We need the actual text to determine the correct byte offset after this character
                        if let Some(goal) = &self.current_goal {
                            let text = &goal.text;
                            // Find how many bytes this character takes
                            let char_start = text
                                .char_indices()
                                .find(|(offset, _)| *offset == byte_offset)
                                .map(|(_, ch)| ch);

                            if let Some(ch) = char_start {
                                return byte_offset + ch.len_utf8();
                            }
                        }
                        // Fallback: assume single byte
                        return byte_offset + 1;
                    }
                }
            }
        }

        // Click after last character - return end of text
        if let Some((_, _, last_byte_offset)) = positions.last() {
            // Need to find the actual end of the text
            if let Some(goal) = &self.current_goal {
                let text = &goal.text;
                return text.len(); // Return the actual byte length of the text
            }
            // Fallback
            return last_byte_offset + 1;
        }
        0
    }

    pub fn render_activity_log_content(
        &mut self,
        fonts: &SidebarFonts,
        window_height: f32,
        palette: &wezterm_term::color::ColorPalette,
    ) -> Element {
        // Performance Note: This method is called on every paint frame, causing markdown
        // to be re-rendered 60+ times per second. Future optimizations could include:
        // 1. Caching rendered Elements (requires making Element Send+Sync)
        // 2. Only rendering visible items (viewport culling)
        // 3. Detecting when content/theme/size hasn't changed
        // 4. Moving markdown parsing to a background thread
        // For now, we rely on the efficiency of the markdown parser and renderer.
        // Get the dynamic bounds for the activity log
        let bounds = self
            .get_activity_log_bounds(window_height)
            .unwrap_or_else(|| {
                euclid::rect(16.0, 200.0, self.width as f32 - 32.0, window_height - 320.0)
            });

        // The activity log height is the bounds height
        let available_for_log = bounds.size.height;
        let available_width = bounds.size.width;

        // Render the activity log content
        let activity_log =
            self.render_activity_log(fonts, available_for_log, available_width, palette);

        // Wrap in a container with background color
        let container = Element::new(&fonts.body, ElementContent::Children(vec![activity_log]))
            .display(DisplayType::Block)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.03, 0.03, 0.035, 1.0).into(), // Slightly lighter than sidebar
                ..Default::default()
            })
            .min_width(Some(Dimension::Pixels(bounds.size.width)))
            .min_height(Some(Dimension::Pixels(bounds.size.height)));

        container
    }

    pub fn render_content(&mut self, fonts: &SidebarFonts, window_height: f32) -> Element {
        let mut children = vec![];

        // Fixed height elements at top
        // Header
        children.push(self.render_header(fonts));

        // Status chip
        children.push(self.render_status_chip(fonts));

        // Filter chips
        children.push(self.render_filter_chips(fonts));

        // Current goal card
        if let Some(goal_element) = self.render_current_goal(fonts) {
            children.push(goal_element);
        }

        // Current suggestion card
        if let Some(suggestion_element) = self.render_current_suggestion(fonts) {
            children.push(suggestion_element);
        }

        // Use the already calculated bounds
        let bounds = self
            .get_activity_log_bounds(window_height)
            .unwrap_or_else(|| {
                euclid::rect(16.0, 200.0, self.width as f32 - 32.0, window_height - 320.0)
            });

        // The spacer should fill the remaining space in the window
        // Total height = sum of all components
        // We already have: header + status + filters + goal + suggestion = bounds.origin.y
        // We need: spacer + chat_input = window_height - bounds.origin.y
        // Chat input needs: 2 lines (~60px) + padding (16px top + 16px bottom) + some margin
        let chat_input_height = 120.0; // Increased to properly show 2 lines with padding
        let spacer_height = (window_height - bounds.origin.y - chat_input_height).max(0.0);

        log::debug!(
            "Sidebar layout: window_height={}, content_above_log={}, spacer_height={}, chat_height={}",
            window_height, bounds.origin.y, spacer_height, chat_input_height
        );

        // Skip the activity log here - it will be rendered separately at a different z-index
        // Add a transparent spacer to maintain layout
        children.push(
            Element::new(&fonts.body, ElementContent::Text(String::new()))
                .display(DisplayType::Block)
                .min_height(Some(Dimension::Pixels(spacer_height)))
                // Completely transparent - no background
                .colors(ElementColors {
                    bg: LinearRgba::with_components(0.0, 0.0, 0.0, 0.0).into(),
                    ..Default::default()
                }),
        );

        // Fixed height chat input at bottom
        children.push(self.render_chat_input(fonts));

        // Container - transparent so the hole works
        Element::new(&fonts.heading, ElementContent::Children(children))
            .display(DisplayType::Block)
            .min_width(Some(Dimension::Pixels(self.width as f32)))
            .min_height(Some(Dimension::Pixels(window_height)))
    }

    pub fn handle_filter_click(&mut self, filter: ActivityFilter) {
        if self.activity_filter != filter {
            // Clear height trackers when filter changes as item indices will change
            self.height_trackers.clear();
            // Keep height cache as the heights are still valid for the same items
            log::debug!("Filter changed to {:?}, cleared height trackers", filter);
        }
        self.activity_filter = filter;
    }

    pub fn handle_goal_confirm(&mut self) {
        if let Some(goal) = &mut self.current_goal {
            goal.is_confirmed = true;
        }
    }

    pub fn handle_goal_edit_toggle(&mut self) {
        if let Some(goal) = &mut self.current_goal {
            goal.is_editing = !goal.is_editing;
            if goal.is_editing {
                goal.edit_text = goal.text.clone();
            }
        }
    }

    /// Estimate how many lines text will wrap to given available width
    fn estimate_wrapped_lines(
        &self,
        text: &str,
        available_width: f32,
        fonts: &SidebarFonts,
    ) -> usize {
        // Get font metrics for accurate estimation
        let font_metrics = fonts.body.metrics();
        let avg_char_width =
            font_metrics.cell_height.get() as f32 * SUGGESTION_CHAR_WIDTH_MULTIPLIER;

        // Use the shared utility function (integer version)
        crate::termwindow::box_model::estimate_wrapped_line_count(
            text,
            available_width,
            avg_char_width,
        )
    }

    pub fn handle_goal_save(&mut self) {
        if let Some(goal) = &mut self.current_goal {
            goal.text = goal.edit_text.clone();
            goal.is_editing = false;
            goal.is_ai_inferred = false;
            goal.is_confirmed = true;
        }
    }

    pub fn handle_suggestion_run(&mut self) {
        // Would trigger command execution
        println!("Running suggestion command...");
    }

    pub fn handle_suggestion_dismiss(&mut self) {
        self.current_suggestion = None;
    }

    pub fn handle_chat_input(&mut self, c: char) {
        self.chat_input.insert_char(c);
    }

    pub fn handle_chat_send(&mut self) {
        let text = self.chat_input.get_text();
        if !text.trim().is_empty() {
            self.activity_log.push(ActivityItem::Chat {
                id: format!("chat_{}", self.activity_log.len()),
                message: text,
                is_user: true,
                timestamp: SystemTime::now(),
            });
            self.chat_input.clear();
            // Clear code block registry since content has changed
            self.clear_code_block_registry();
        }
    }

    /// Set the scrollbar bounds for hit testing
    pub fn set_scrollbar_bounds(&mut self, bounds: euclid::Rect<f32, window::PixelUnit>) {
        log::debug!(
            "Setting scrollbar bounds: origin=({}, {}), size=({}, {})",
            bounds.origin.x,
            bounds.origin.y,
            bounds.size.width,
            bounds.size.height
        );
        self.activity_log_scrollbar_bounds = Some(bounds);
    }

    /// Get the bounds of the activity log viewport for clipping
    pub fn get_activity_log_bounds(
        &self,
        window_height: f32,
    ) -> Option<euclid::Rect<f32, window::PixelUnit>> {
        // Calculate dynamic positions based on ACTUAL rendered heights:
        // Header: 58px
        let mut top = 58.0;

        // Status chip
        top += 52.0;

        // Filter chips
        top += 55.0;

        // Add goal card height if present
        if self.current_goal.is_some() {
            top += 201.0;
        }

        // Add suggestion card height if present
        if self.current_suggestion.is_some() {
            // Setting to match visual observation
            top += 201.0;
        }

        // Add padding between last card and activity log for visual separation
        top += 10.0; // Increased for better visual separation

        // Bottom calculation
        // Leave space for chat input at the bottom
        let bottom = window_height - 120.0; // Match chat_input_height in render_content
        let left = 16.0; // Padding
        let right = self.width as f32 - 16.0; // Right padding for scrollbar

        log::debug!(
            "Activity log bounds: top={}, bottom={}, left={}, right={}, height={}",
            top,
            bottom,
            left,
            right,
            bottom - top
        );

        Some(euclid::rect(left, top, right - left, bottom - top))
    }

    /// Check if a mouse event is within the scrollbar bounds
    fn is_scrollbar_event(&self, event: &MouseEvent) -> bool {
        if let Some(bounds) = &self.activity_log_scrollbar_bounds {
            let point = euclid::point2(event.coords.x as f32, event.coords.y as f32);
            let contains = bounds.contains(point);
            log::debug!(
                "Checking scrollbar bounds: point=({}, {}), bounds=({}, {}, {}, {}), contains={}",
                point.x,
                point.y,
                bounds.origin.x,
                bounds.origin.y,
                bounds.size.width,
                bounds.size.height,
                contains
            );
            contains
        } else {
            log::debug!("No scrollbar bounds set");
            false
        }
    }

    /// Update sidebar position for mouse event handling
    pub fn update_sidebar_position(&mut self, sidebar_x: f32) {
        // Store sidebar position for mouse event handling
        self.sidebar_x_position = sidebar_x;
    }

    /// Check which filter chip was clicked based on coordinates
    fn get_clicked_filter(&self, event: &MouseEvent, sidebar_x: f32) -> Option<ActivityFilter> {
        // Check if click is in the filter chip area (approximate Y range)
        // Header: 58px, Status chip: 52px = 110px top
        // Filter chips height: ~55px, so range is 110-165
        let y = event.coords.y as f32;
        if y < 110.0 || y > 165.0 {
            log::debug!("Click Y {} outside filter range 110-165", y);
            return None;
        }

        // Convert window X coordinate to sidebar-relative X
        let relative_x = event.coords.x as f32 - sidebar_x;

        // The chips are laid out starting at x=16 within the sidebar
        // Approximate widths: All(35), Commands(75), Chat(40), Suggestions(85)
        // With 8px spacing between chips
        let base_x = 16.0;
        if relative_x < base_x {
            return None;
        }

        let x = relative_x - base_x;
        log::debug!("Filter chip click: relative_x={}, x={}", relative_x, x);

        // Updated measurements based on actual chip sizes
        // Small chips have ~6px padding each side + text width
        if x < 47.0 {
            // "All" chip (~35px text + 12px padding)
            Some(ActivityFilter::All)
        } else if x < 142.0 {
            // 47 + 8 + 87 ("Commands" ~75px + 12px)
            Some(ActivityFilter::Commands)
        } else if x < 202.0 {
            // 142 + 8 + 52 ("Chat" ~40px + 12px)
            Some(ActivityFilter::Chat)
        } else if x < 299.0 {
            // 202 + 8 + 97 ("Suggestions" ~85px + 12px)
            Some(ActivityFilter::Suggestions)
        } else {
            None
        }
    }

    pub fn show_suggestion_modal(&mut self, suggestion: CurrentSuggestion) {
        let modal = Modal {
            id: "suggestion_modal".to_string(),
            size: ModalSize::FillSidebar,
            content: Box::new(SuggestionModal::new(suggestion)),
            animation_state: crate::sidebar::components::modal::ModalAnimationState::Opening,
            close_on_click_outside: true,
            close_on_escape: true,
            position: None,
        };
        self.modal_manager.show(modal);
    }

    pub fn close_modal(&mut self) {
        self.modal_manager.close();
    }

    pub fn get_current_suggestion(&self) -> Option<&CurrentSuggestion> {
        self.current_suggestion.as_ref()
    }

    /// Clear code block registry when content changes completely
    pub fn clear_code_block_registry(&mut self) {
        if let Some(ref registry) = self.code_block_registry {
            if let Ok(mut reg) = registry.lock() {
                reg.clear();
            }
        }
    }

    pub fn render_modals(&mut self, fonts: &SidebarFonts, window_height: f32) -> Vec<Element> {
        // Get sidebar bounds
        let sidebar_bounds = euclid::rect(
            self.sidebar_x_position,
            0.0,
            self.width as f32,
            window_height,
        );

        // Get window bounds (we'll need to pass this from the parent)
        // For now, use a reasonable default
        let window_bounds = euclid::rect(
            0.0,
            0.0,
            self.sidebar_x_position + self.width as f32 + 100.0, // Approximate window width
            window_height,
        );

        self.modal_manager.render(
            sidebar_bounds,
            window_bounds,
            fonts,
            self.code_block_registry.clone(),
        )
    }
}

impl Sidebar for AiSidebar {
    fn render(&mut self, fonts: &SidebarFonts, window_height: f32) -> Element {
        // Store window height for mouse event handling
        self.last_viewport_height = Some(window_height);
        self.render_content(fonts, window_height)
    }

    fn get_scrollbars(&self) -> super::SidebarScrollbars {
        // Calculate chat input scrollbar info - show if there's scrollable content
        // (not just when focused, to provide visual feedback)
        let chat_input_scrollbar = {
            // Calculate using actual font metrics
            // The logs show line_height is 20px, with 1.1x multiplier = 22px
            let line_height = 20.0;
            // TODO: Extract line spacing multiplier (1.1) to a constant - used throughout codebase
            let line_height_with_spacing = line_height * 1.1;
            let viewport_height = self.chat_input.display_lines as f32 * line_height_with_spacing;

            // Use visual line count if available (from text wrapping), otherwise fall back to logical lines
            let line_count = if self.chat_input.visual_line_count > 0 {
                self.chat_input.visual_line_count
            } else {
                self.chat_input.lines.len()
            };
            let total_height = line_count as f32 * line_height_with_spacing;

            log::debug!(
                "Chat input scrollbar calc: focused={}, logical_lines={}, visual_lines={}, display_lines={}, total_height={}, viewport_height={}, needs_scrollbar={}",
                self.chat_input.focused,
                self.chat_input.lines.len(),
                self.chat_input.visual_line_count,
                self.chat_input.display_lines,
                total_height,
                viewport_height,
                total_height > viewport_height
            );

            if total_height > viewport_height {
                // Calculate scroll position
                let visible_range = self.chat_input.scroll_pixel_offset / total_height;
                let thumb_size = (viewport_height / total_height).min(1.0);

                let info = ScrollbarInfo {
                    should_show: true,
                    thumb_position: visible_range,
                    thumb_size,
                    content_height: total_height,
                    viewport_height,
                    scroll_offset: self.chat_input.scroll_pixel_offset,
                    total_items: self.chat_input.lines.len(), // For compatibility
                    viewport_items: self.chat_input.display_lines, // For compatibility
                };

                log::debug!(
                    "Chat input scrollbar created: should_show={}, thumb_position={}, thumb_size={}",
                    info.should_show, info.thumb_position, info.thumb_size
                );

                Some(info)
            } else {
                None
            }
        };

        super::SidebarScrollbars {
            activity_log: self.activity_log_scrollbar.clone(),
            chat_input: chat_input_scrollbar,
        }
    }

    fn get_width(&self) -> u16 {
        self.width
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    fn get_position(&self) -> SidebarPosition {
        SidebarPosition::Right
    }

    fn set_width(&mut self, width: u16) {
        self.width = width;
    }

    fn handle_mouse_event(&mut self, event: &MouseEvent) -> Result<bool> {
        log::debug!(
            "AI sidebar handle_mouse_event: {:?} at ({}, {})",
            event.kind,
            event.coords.x,
            event.coords.y
        );

        // Handle modal events first - if modal is active, it captures ALL events
        if self.modal_manager.is_active() {
            let sidebar_bounds = euclid::rect(
                self.sidebar_x_position,
                0.0,
                self.width as f32,
                1000.0, // Use a reasonable default height
            );
            // Always let modal handle the event when it's active
            let handled = self.modal_manager.handle_mouse_event(event, sidebar_bounds);
            // For scroll wheel events, always return true when modal is active to prevent
            // the activity log from scrolling behind the modal
            if matches!(event.kind, WMEK::VertWheel(_)) {
                return Ok(true);
            }
            if handled {
                return Ok(true);
            }
        }

        // Code block horizontal scrolling has been removed - using line wrapping instead

        // Show more button is now handled via UIItemType

        // Handle text selection drag during Move events
        if let WMEK::Move = &event.kind {
            // Check if we're currently dragging a selection
            if self.selection_state.is_dragging && event.mouse_buttons == MouseButtons::LEFT {
                // log::debug!("SELECTION DEBUG: Handling drag Move event in sidebar");

                // Determine which selection target we're dragging
                if let Some(selection) = &self.selection_state.active_selection {
                    match selection {
                        SelectionTarget::Goal { anchor_byte, .. } => {
                            // Handle goal text drag directly in sidebar since mouse may be outside UIItem bounds
                            if let Some(bounds) = self.goal_bounds {
                                let relative_x =
                                    event.coords.x as f32 - bounds.origin.x - GOAL_CARD_PADDING;

                                // log::debug!("SELECTION DEBUG: Goal drag at relative_x={}", relative_x);

                                // Use stored real positions to find byte offset
                                if let Some(positions) = &self.goal_char_positions {
                                    let current_byte =
                                        self.find_byte_offset_from_positions(relative_x, positions);
                                    // log::debug!("SELECTION DEBUG: Calculated byte_offset={} from relative_x={}", current_byte, relative_x);

                                    self.update_selection_drag(current_byte);
                                    return Ok(true); // Event handled
                                } else {
                                    // log::debug!("SELECTION DEBUG: No character positions available for goal text");
                                }
                            } else {
                                // log::debug!("SELECTION DEBUG: No goal bounds available");
                            }
                        }
                        SelectionTarget::ActivityItem { anchor_index, .. } => {
                            // Activity item drag is handled by the mouse event handler
                            // which calls update_activity_log_selection_drag
                        }
                        SelectionTarget::Suggestion { .. } => {
                            // TODO: Handle suggestion drag
                            // log::debug!("SELECTION DEBUG: Suggestion drag - not yet implemented");
                        }
                        SelectionTarget::ChatInput { .. } => {
                            // Chat input has its own handling
                        }
                    }
                }
            }
        }

        // Log current bounds for debugging
        if let WMEK::Press(MousePress::Left) = &event.kind {
            log::debug!("Left click at ({}, {})", event.coords.x, event.coords.y);
            log::debug!("Filter chip bounds:");
            for (filter, bounds) in &self.filter_chip_bounds {
                log::debug!(
                    "  {:?}: x={}, y={}, w={}, h={}",
                    filter,
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.size.width,
                    bounds.size.height
                );
            }
        }

        // Handle scroll wheel events
        if let WMEK::VertWheel(amount) = &event.kind {
            log::debug!(
                "Scroll wheel event: amount={}, has_renderer={}",
                amount,
                self.activity_log_scrollbar_renderer.is_some()
            );

            // Check if we have a scrollbar renderer to get scroll metrics
            if let Some(renderer) = &self.activity_log_scrollbar_renderer {
                let scroll_speed = 20.0; // Pixels per scroll step (roughly 1 line)
                let scroll_amount = scroll_speed * (*amount as f32).abs();

                let old_offset = self.activity_log_scroll_offset;
                let new_offset = if *amount > 0 {
                    // Scroll up
                    (self.activity_log_scroll_offset - scroll_amount).max(0.0)
                } else {
                    // Scroll down
                    self.activity_log_scroll_offset + scroll_amount
                };

                // Constrain to valid range using actual content metrics
                let max_scroll = (renderer.total_size() - renderer.viewport_size()).max(0.0);
                self.activity_log_scroll_offset = new_offset.clamp(0.0, max_scroll);

                let actually_scrolled = (self.activity_log_scroll_offset - old_offset).abs() > 0.1;
                log::debug!(
                    "Scroll wheel: old_offset={}, new_offset={}, max_scroll={}, amount={}, scroll_amount={}, actually_moved={}",
                    old_offset, self.activity_log_scroll_offset, max_scroll, amount, scroll_amount, actually_scrolled
                );

                // Clear activity item bounds when scrolling - they'll be repopulated on next render
                if actually_scrolled {
                    self.activity_item_bounds.clear();
                }

                // Return true to consume the event since we're over the activity log
                return Ok(true);
            } else {
                log::debug!("No scrollbar renderer for scroll wheel");
                // Still over activity log but no scrollbar - don't consume
                return Ok(false);
            }
        }

        // Handle text selection drag
        if let WMEK::Move = event.kind {
            if self.selection_state.is_dragging {
                // Drag handling is done externally where font is available
                // Just mark that we're handling the drag
                return Ok(true);
            }
        }

        // Handle mouse release to end selection
        if let WMEK::Release(MousePress::Left) = event.kind {
            self.selection_state.is_dragging = false;
        }

        // Check if we need to handle scrollbar events
        // Always process mouse events if the scrollbar is currently being dragged,
        // even if the mouse is outside the scrollbar bounds
        let should_handle_scrollbar = if let Some(renderer) = &self.activity_log_scrollbar_renderer
        {
            renderer.state().is_dragging || self.is_scrollbar_event(event)
        } else {
            false
        };

        if should_handle_scrollbar {
            if let Some(renderer) = &mut self.activity_log_scrollbar_renderer {
                if let Some(bounds) = &self.activity_log_scrollbar_bounds {
                    // Handle the mouse event with the scrollbar renderer
                    if let Some(new_scroll_offset) = renderer.handle_mouse_event(event, *bounds) {
                        // Update scroll position with proper bounds checking
                        let max_scroll =
                            (renderer.total_size() - renderer.viewport_size()).max(0.0);
                        self.activity_log_scroll_offset = new_scroll_offset.clamp(0.0, max_scroll);

                        // Clear visual anchor when user interacts with scrollbar
                        self.visual_anchor = None;

                        // Clear activity item bounds when scrolling - they'll be repopulated on next render
                        self.activity_item_bounds.clear();

                        log::debug!(
                            "Scrollbar updated scroll offset to: {} (max: {})",
                            self.activity_log_scroll_offset,
                            max_scroll
                        );
                        return Ok(true);
                    }
                    return Ok(renderer.state().is_dragging);
                }
            }
        }

        // Filter chip clicks are now handled through UIItemType
        // Just return false to let the UIItem system handle it
        Ok(false)
    }

    fn handle_key_event(&mut self, key: &KeyCode, modifiers: KeyModifiers) -> Result<bool> {
        log::debug!(
            "AI sidebar received key event: {:?} with modifiers: {:?}",
            key,
            modifiers
        );

        // Handle modal keyboard events first
        if self.modal_manager.is_active() {
            log::debug!("Modal is active, forwarding key to modal manager");
            if self.modal_manager.handle_key_event(*key, modifiers) {
                return Ok(true);
            }
        }

        // Handle chat input keyboard events when it has focus
        if self.chat_input.focused {
            log::debug!(
                "Chat input has focus, handling key event: {:?} with modifiers: {:?}",
                key,
                modifiers
            );

            // Special handling for certain keys
            match key {
                KeyCode::Escape => {
                    // Escape unfocuses the chat input, returning focus to terminal
                    self.chat_input.focused = false;
                    self.chat_input_border_color = LinearRgba::with_components(0.3, 0.3, 0.35, 0.5);
                    return Ok(true);
                }
                KeyCode::Enter => {
                    // Check if Shift is held
                    if modifiers.contains(KeyModifiers::SHIFT) {
                        // Shift+Enter should insert a newline - let MultilineTextInput handle it
                        let result = self.chat_input.handle_key_event(key, modifiers);
                        log::debug!(
                            "MultilineTextInput.handle_key_event (Shift+Enter) returned: {:?}",
                            result
                        );
                        return result;
                    } else {
                        // Enter without shift sends the message
                        if !self.chat_input.get_text().trim().is_empty() {
                            self.handle_chat_send();
                        }
                        return Ok(true);
                    }
                }
                _ => {
                    // Let MultilineTextInput handle all other keys
                    let result = self.chat_input.handle_key_event(key, modifiers);
                    log::debug!("MultilineTextInput.handle_key_event returned: {:?}", result);
                    return result;
                }
            }
        }

        // If neither modal nor chat input has focus, don't capture keyboard events
        // This allows the terminal to maintain focus by default
        Ok(false)
    }

    fn has_keyboard_focus(&self) -> bool {
        self.modal_manager.is_active() || self.chat_input.focused
    }

    fn clear_focus(&mut self) {
        // Clear chat input focus
        self.chat_input.focused = false;
        self.chat_input_border_color = LinearRgba::with_components(0.3, 0.3, 0.35, 0.5);
        // Note: We don't clear modal focus here as modals should handle their own dismissal
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

impl AiSidebar {
    /// Check if keyboard input should be routed to this sidebar
    pub fn has_keyboard_focus(&self) -> bool {
        self.modal_manager.is_active() || self.chat_input.focused
    }

    /// Check if the chat input specifically has focus
    pub fn has_input_focus(&self) -> bool {
        self.chat_input.focused
    }

    /// Check if a modal is currently active
    pub fn has_modal_active(&self) -> bool {
        self.modal_manager.is_active()
    }

    /// Set focus to the chat input
    pub fn focus_chat_input(&mut self) {
        self.chat_input.focused = true;
        self.chat_input_border_color = LinearRgba::with_components(0.4, 0.6, 0.9, 0.7);
    }

    /// Handle click on chat input with position (simplified without font access)
    pub fn handle_chat_input_click_simple(
        &mut self,
        click_x: f32,
        click_y: f32,
        bounds: &euclid::Rect<f32, euclid::UnknownUnit>,
    ) {
        // Focus the input
        self.focus_chat_input();

        // Calculate relative position within the chat input text area
        let text_padding = 8.0;
        let border_thickness = 1.0;
        let vertical_padding = 6.0;

        let relative_x = click_x - bounds.origin.x - border_thickness - text_padding;
        let relative_y = click_y - bounds.origin.y - border_thickness - vertical_padding;

        // Only position cursor if click is within the text area
        if relative_x >= 0.0 && relative_y >= 0.0 {
            // Use estimated font metrics WITH 1.1x multiplier to match rendering
            let line_height = 20.0 * 1.1; // Match the rendering multiplier
            let char_width = 8.5; // More accurate monospace char width for ~12pt font
            self.chat_input
                .handle_click_position(relative_x, relative_y, line_height, char_width);
        }
    }

    /// Handle click on chat input with pre-calculated character positions
    pub fn handle_chat_input_click_with_positions(
        &mut self,
        relative_x: f32,
        relative_y: f32,
        line_positions: &Vec<Vec<(f32, f32, usize)>>,
        is_drag: bool,
        shift_held: bool,
    ) {
        log::debug!(
            "handle_chat_input_click_with_positions: relative_x={}, relative_y={}, line_positions_count={}, is_drag={}, shift_held={}",
            relative_x, relative_y, line_positions.len(), is_drag, shift_held
        );

        // Debug: log what positions we received
        for (line_idx, line_pos) in line_positions.iter().enumerate().take(2) {
            log::debug!("  Line {} has {} positions", line_idx, line_pos.len());
            for (i, &(x_start, x_end, byte_offset)) in line_pos.iter().enumerate().take(5) {
                log::debug!(
                    "    Pos[{}]: x=({:.1}, {:.1}), byte_offset={}",
                    i,
                    x_start,
                    x_end,
                    byte_offset
                );
            }
        }

        // Store the exact positions for cursor positioning
        self.chat_input.exact_glyph_positions = line_positions.clone();

        // Focus the input
        self.focus_chat_input();

        // Account for padding inside the chat input
        // Container has 8px top padding, and we need to account for that
        let container_padding_top = 8.0;
        let text_padding = 4.0; // Per-line text element padding
        let adjusted_x = relative_x - text_padding;
        let adjusted_y = relative_y - container_padding_top - 2.0; // Container + per-line padding

        log::debug!(
            "Adjusted coordinates: x={}, y={}, text_padding={}",
            adjusted_x,
            adjusted_y,
            text_padding
        );

        // Only position cursor if click is within the text area
        if adjusted_x >= 0.0 && adjusted_y >= 0.0 {
            // Use actual line height with spacing
            let line_height_with_spacing = 20.0 * 1.1; // 22px as shown in logs

            // Calculate which line was clicked (accounting for scroll offset)
            let clicked_line = ((adjusted_y + self.chat_input.scroll_pixel_offset)
                / line_height_with_spacing) as usize;

            log::debug!(
                "Line calculation: adjusted_y={}, scroll_offset={}, line_height={}, clicked_line={}",
                adjusted_y, self.chat_input.scroll_pixel_offset, line_height_with_spacing, clicked_line
            );

            if clicked_line < line_positions.len() {
                // Find character position in the visual line using exact glyph positions
                let line_glyph_positions = &line_positions[clicked_line];

                log::debug!(
                    "Visual line {}: glyph_positions_count={}",
                    clicked_line,
                    line_glyph_positions.len()
                );

                // Debug log the glyph positions
                if line_glyph_positions.len() > 0 {
                    log::debug!(
                        "First glyph position: ({}, {}), Last glyph position: ({}, {})",
                        line_glyph_positions[0].0,
                        line_glyph_positions[0].1,
                        line_glyph_positions.last().unwrap().0,
                        line_glyph_positions.last().unwrap().1
                    );
                }

                // Find which character was clicked using exact glyph positions
                let clicked_document_byte_offset = if line_glyph_positions.is_empty() {
                    0
                } else {
                    // Find the glyph that contains the click position
                    let mut found_offset = None;
                    for (x_start, x_end, byte_offset) in line_glyph_positions.iter() {
                        if adjusted_x < *x_start {
                            // Click is before this glyph
                            found_offset = Some(*byte_offset);
                            break;
                        } else if adjusted_x >= *x_start && adjusted_x <= *x_end {
                            // Click is within this glyph - decide if it's closer to start or end
                            let mid = (*x_start + *x_end) / 2.0;
                            if adjusted_x < mid {
                                found_offset = Some(*byte_offset);
                            } else {
                                // Find the next glyph's byte offset
                                let next_idx = line_glyph_positions
                                    .iter()
                                    .position(|(s, _, _)| *s == *x_start)
                                    .and_then(|idx| line_glyph_positions.get(idx + 1))
                                    .map(|(_, _, next_offset)| *next_offset);
                                found_offset = next_idx.or(Some(*byte_offset + 1));
                            }
                            break;
                        }
                    }

                    // If click is past all glyphs, position at end of visual line
                    if found_offset.is_none() {
                        if let Some((_, _, last_offset)) = line_glyph_positions.last() {
                            // Use the last offset + 1 for "after last character"
                            found_offset = Some(*last_offset + 1);
                        }
                    }

                    found_offset.unwrap_or(0)
                };

                // Now map the document byte offset to logical line and column
                let mut current_byte = 0;
                let mut found_logical_position = false;
                let mut logical_line = 0;
                let mut logical_col = 0;

                for (logical_line_idx, line_text) in self.chat_input.lines.iter().enumerate() {
                    let line_start = current_byte;
                    let line_end = current_byte + line_text.len();

                    if clicked_document_byte_offset >= line_start
                        && clicked_document_byte_offset <= line_end
                    {
                        // Found the logical line containing this byte offset
                        let line_relative_byte = clicked_document_byte_offset - line_start;

                        // Convert byte offset to character index
                        let char_index = line_text
                            .char_indices()
                            .take_while(|(byte_idx, _)| *byte_idx < line_relative_byte)
                            .count();

                        log::debug!(
                            "[CLICK_DEBUG] Mapped click to logical line {}, char {}, doc_byte_offset={}",
                            logical_line_idx,
                            char_index,
                            clicked_document_byte_offset
                        );

                        logical_line = logical_line_idx;
                        logical_col = char_index;
                        found_logical_position = true;
                        break;
                    }

                    current_byte = line_end + 1; // +1 for newline
                }

                if found_logical_position {
                    // Handle selection logic
                    if is_drag {
                        // Update selection during drag
                        self.update_chat_input_selection(logical_line, logical_col);
                    } else if shift_held {
                        // Extend selection with shift+click
                        if let Some(SelectionTarget::ChatInput { .. }) =
                            &self.selection_state.active_selection
                        {
                            // Update current position to extend selection
                            self.update_chat_input_selection(logical_line, logical_col);
                        } else {
                            // Start new selection from cursor to clicked position
                            let cursor_line = self.chat_input.cursor_line;
                            let cursor_col = self.chat_input.cursor_col;
                            self.start_chat_input_selection(cursor_line, cursor_col);
                            self.update_chat_input_selection(logical_line, logical_col);
                        }
                    } else {
                        // Regular click - clear selection and move cursor
                        self.selection_state.clear();
                        self.chat_input.cursor_line = logical_line;
                        self.chat_input.cursor_col = logical_col;

                        // Start potential selection (for drag)
                        self.selection_state.prepared_selection =
                            Some(SelectionTarget::ChatInput {
                                anchor_line: logical_line,
                                anchor_byte: logical_col,
                                current_line: logical_line,
                                current_byte: logical_col,
                            });
                    }
                } else {
                    log::warn!(
                        "[CLICK_DEBUG] Could not map document byte offset {} to logical line",
                        clicked_document_byte_offset
                    );
                }
            } else {
                log::warn!(
                    "[CLICK_DEBUG] Click outside valid lines: clicked_line={}, total_lines={}",
                    clicked_line,
                    self.chat_input.lines.len()
                );
            }
        } else {
            log::warn!(
                "[CLICK_DEBUG] Click outside text area: adjusted_x={}, adjusted_y={}",
                adjusted_x,
                adjusted_y
            );
        }
    }

    /// Handle mouse wheel events for chat input (simplified without font access)
    pub fn handle_chat_input_wheel_simple(&mut self, amount: i16) -> bool {
        // Convert wheel amount to pixel delta
        // Negative amount means scroll up, positive means scroll down
        let line_height = 20.0 * 1.1; // Estimated line height with rendering multiplier
        let delta = amount as f32 * 3.0; // Multiply for smoother scrolling
        self.chat_input.handle_wheel_scroll(delta, line_height)
    }

    /// Handle copy operation (Ctrl+C / Cmd+C)
    pub fn handle_copy(&mut self, window: &dyn window::WindowOps) -> bool {
        // Check if chat input has focus and selection
        if self.chat_input.focused {
            if let Some(text) = self.chat_input.get_selected_text() {
                window.set_clipboard(window::Clipboard::Clipboard, text);
                return true;
            }
        }

        // Check sidebar selection state
        if let Some(text) = self.selection_state.get_selected_text(self) {
            window.set_clipboard(window::Clipboard::Clipboard, text);
            return true;
        }

        false
    }

    /// Get the vertical spacing (padding + margin + border) for an activity item
    fn get_activity_item_spacing(item: &ActivityItem) -> f32 {
        match item {
            ActivityItem::Chat { .. } => {
                // Top padding + bottom padding + bottom margin + top border + bottom border
                CHAT_ITEM_PADDING * 2.0 + CHAT_ITEM_BOTTOM_MARGIN + CHAT_ITEM_BORDER * 2.0
            }
            ActivityItem::Command { .. }
            | ActivityItem::Suggestion { .. }
            | ActivityItem::Goal { .. } => {
                // Cards have default margin on all sides but we only count vertical
                CARD_DEFAULT_MARGIN * 2.0 // Top and bottom margin
            }
        }
    }

    /// Check if any animations need frame updates
    pub fn needs_animation_frame(&self) -> bool {
        // For now, activity log scrollbar doesn't have animations since auto_hide is false
        // Only check modal animations
        false
    }

    /// Get a mutable reference to the modal manager for animation updates
    pub fn modal_manager_mut(&mut self) -> &mut ModalManager {
        &mut self.modal_manager
    }

    /// Get the height of an activity item (cached or estimated)
    fn get_activity_item_height(
        &self,
        item: &ActivityItem,
        line_height: f32,
        available_width: f32,
    ) -> f32 {
        let id = match item {
            ActivityItem::Command { id, .. } => id.clone(),
            ActivityItem::Chat { id, .. } => id.clone(),
            ActivityItem::Suggestion { id, .. } => id.clone(),
            ActivityItem::Goal { id, .. } => id.clone(),
        };

        if let Some(cached_height) = self.activity_log_height_cache.get(&id) {
            let estimated = estimate_activity_item_height(item, line_height, available_width);
            if (cached_height - estimated).abs() > 100.0 {
                log::info!(
                    "[VSCROLL] Large height difference for {}: cached={:.0} vs estimated={:.0} (delta={:.0})",
                    id, cached_height, estimated, cached_height - estimated
                );
            }
            *cached_height
        } else {
            estimate_activity_item_height(item, line_height, available_width)
        }
    }

    /// Calculate total content height
    fn calculate_total_activity_log_height(
        &self,
        filtered_items: &[(usize, &ActivityItem)],
        line_height: f32,
        available_width: f32,
    ) -> f32 {
        filtered_items
            .iter()
            .map(|(_, item)| self.get_activity_item_height(item, line_height, available_width))
            .sum::<f32>()
        // Removed +20px hack - virtual scrolling with accurate height caching handles this correctly
    }

    /// Update height cache from rendered ComputedElement
    /// This extracts the actual rendered heights from the computed element tree
    pub fn update_activity_log_height_cache(
        &mut self,
        activity_log_computed: &crate::termwindow::box_model::ComputedElement,
        viewport_height: f32,
    ) {
        use crate::termwindow::box_model::ComputedElementContent;

        // Remember if we were at the bottom before updating
        let was_at_bottom = if let Some(scrollbar) = &self.activity_log_scrollbar {
            let max_scroll = (scrollbar.content_height - scrollbar.viewport_height).max(0.0);
            let at_bottom = max_scroll > 0.0 && self.activity_log_scroll_offset >= max_scroll - 1.0;
            if at_bottom {
                log::debug!(
                    "[VSCROLL] was_at_bottom=true (scroll={:.0}, max={:.0}, content={:.0})",
                    self.activity_log_scroll_offset,
                    max_scroll,
                    scrollbar.content_height
                );
            }
            at_bottom
        } else {
            false
        };

        // Remember the old total height
        let old_total_height = self
            .activity_log_scrollbar
            .as_ref()
            .map(|s| s.content_height)
            .unwrap_or(0.0);

        // The activity log computed element structure is:
        // - Root container (with margin for scrolling)
        //   - Content area (with children for each visible item)

        // Track if we need sticky bottom (currently disabled)
        let sticky_bottom_needed = was_at_bottom;

        // First, try to find the content area with the visible items
        // The structure is: root → viewport → content_area → [items]
        if let ComputedElementContent::Children(ref root_children) = activity_log_computed.content {
            log::debug!(
                "[VSCROLL] Root element has {} children, bounds height: {:.0}",
                root_children.len(),
                activity_log_computed.bounds.height()
            );

            if let Some(viewport) = root_children.first() {
                log::debug!(
                    "[VSCROLL] Viewport bounds: origin=({:.0}, {:.0}), size=({:.0} x {:.0})",
                    viewport.bounds.origin.x,
                    viewport.bounds.origin.y,
                    viewport.bounds.size.width,
                    viewport.bounds.size.height
                );

                if let ComputedElementContent::Children(ref viewport_children) = viewport.content {
                    log::debug!(
                        "[VSCROLL] Viewport has {} children",
                        viewport_children.len()
                    );

                    if let Some(content_area) = viewport_children.first() {
                        log::debug!(
                            "[VSCROLL] Content area bounds: origin=({:.0}, {:.0}), size=({:.0} x {:.0})",
                            content_area.bounds.origin.x,
                            content_area.bounds.origin.y,
                            content_area.bounds.size.width,
                            content_area.bounds.size.height
                        );

                        if let ComputedElementContent::Children(ref item_elements) =
                            content_area.content
                        {
                            // Now we have the individual item elements
                            // We need to map these back to the visible items

                            log::debug!(
                                "[VSCROLL] Found {} item elements in content area (visible range has {} items)",
                                item_elements.len(),
                                self.activity_log_visible_range.clone().count()
                            );

                            // Get the filtered items to match against
                            let filtered_items: Vec<(usize, &ActivityItem)> = self
                                .activity_log
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| match self.activity_filter {
                                    ActivityFilter::All => true,
                                    ActivityFilter::Commands => {
                                        matches!(item, ActivityItem::Command { .. })
                                    }
                                    ActivityFilter::Chat => {
                                        matches!(item, ActivityItem::Chat { .. })
                                    }
                                    ActivityFilter::Suggestions => {
                                        matches!(item, ActivityItem::Suggestion { .. })
                                    }
                                })
                                .collect();

                            // Debug: Check if we have the expected number of elements
                            if item_elements.len()
                                != self.activity_log_visible_range.clone().count()
                            {
                                log::warn!(
                                    "[VSCROLL] Mismatch: expected {} item elements, found {}",
                                    self.activity_log_visible_range.clone().count(),
                                    item_elements.len()
                                );
                            }

                            // For each computed element in the visible range
                            for (relative_idx, computed_item) in item_elements.iter().enumerate() {
                                // Map relative index to actual index in visible range
                                if let Some(visible_idx) =
                                    self.activity_log_visible_range.clone().nth(relative_idx)
                                {
                                    if let Some((_, item)) = filtered_items.get(visible_idx) {
                                        // Get the item ID
                                        let item_id = match item {
                                            ActivityItem::Command { id, .. } => id.clone(),
                                            ActivityItem::Chat { id, .. } => id.clone(),
                                            ActivityItem::Suggestion { id, .. } => id.clone(),
                                            ActivityItem::Goal { id, .. } => id.clone(),
                                        };

                                        // Extract the rendered height
                                        // Use border_rect height which includes the full rendered height with padding/borders
                                        let rendered_height = computed_item.border_rect.size.height;

                                        // DEBUG: Log height comparison
                                        log::debug!(
                                    "[VSCROLL] HEIGHT DEBUG {}: calculated={:.0}, border_rect.h={:.0}, content_rect.h={:.0}, y_pos={:.0}",
                                    item_id,
                                    rendered_height,
                                    computed_item.border_rect.size.height,
                                    computed_item.content_rect.size.height,
                                    computed_item.content_rect.origin.y
                                );

                                        // Special logging for tall items
                                        if item_id.contains("chat2")
                                            || computed_item.border_rect.size.height > 1000.0
                                        {
                                            log::debug!(
                                        "[VSCROLL] TALL ITEM {}: border_rect.h={:.0} (vs viewport={:.0}), clipped={}",
                                        item_id,
                                        computed_item.border_rect.size.height,
                                        viewport_height,
                                        computed_item.border_rect.size.height > viewport_height
                                    );
                                        }

                                        // DEBUG: Log detailed element information
                                        log::debug!(
                                            "[VSCROLL] Item {} element #{} debug:",
                                            item_id,
                                            relative_idx
                                        );
                                        log::debug!(
                                            "  CALCULATED height: {:.0}px (position-based)",
                                            rendered_height
                                        );
                                        log::debug!(
                                    "  content_rect: origin=({:.0}, {:.0}) size=({:.0} x {:.0})",
                                    computed_item.content_rect.origin.x,
                                    computed_item.content_rect.origin.y,
                                    computed_item.content_rect.size.width,
                                    computed_item.content_rect.size.height
                                );
                                        log::debug!(
                                            "  bounds: origin=({:.0}, {:.0}) size=({:.0} x {:.0})",
                                            computed_item.bounds.origin.x,
                                            computed_item.bounds.origin.y,
                                            computed_item.bounds.size.width,
                                            computed_item.bounds.size.height
                                        );

                                        // Log the margin/padding info if the height seems wrong
                                        if computed_item.content_rect.size.height > 2000.0 {
                                            let padding_height = computed_item.padding.height()
                                                - computed_item.content_rect.height();
                                            let border_height = computed_item.border_rect.height()
                                                - computed_item.padding.height();
                                            let margin_height = computed_item.bounds.height()
                                                - computed_item.border_rect.height();

                                            log::debug!(
                                        "  HEIGHT BREAKDOWN: content={:.0}, +padding={:.0}, +border={:.0}, +margin={:.0}",
                                        computed_item.content_rect.size.height,
                                        padding_height,
                                        border_height,
                                        margin_height
                                    );
                                        }
                                        log::debug!(
                                    "  border_rect: origin=({:.0}, {:.0}) size=({:.0} x {:.0})",
                                    computed_item.border_rect.origin.x,
                                    computed_item.border_rect.origin.y,
                                    computed_item.border_rect.size.width,
                                    computed_item.border_rect.size.height
                                );

                                        // Check what type of content this element has
                                        match &computed_item.content {
                                            ComputedElementContent::Text(_) => {
                                                log::debug!("  content type: Text");
                                            }
                                            ComputedElementContent::Children(children) => {
                                                log::debug!(
                                                    "  content type: Children (count: {})",
                                                    children.len()
                                                );

                                                // For elements with children, try to calculate height from children
                                                if !children.is_empty() {
                                                    let first_child_y = children
                                                        .first()
                                                        .unwrap()
                                                        .content_rect
                                                        .origin
                                                        .y;
                                                    let last_child = children.last().unwrap();
                                                    let last_child_bottom =
                                                        last_child.content_rect.origin.y
                                                            + last_child.content_rect.size.height;
                                                    let calculated_height =
                                                        last_child_bottom - first_child_y;

                                                    log::debug!(
                                                "  calculated height from children: {:.0} (first_y: {:.0}, last_bottom: {:.0})",
                                                calculated_height, first_child_y, last_child_bottom
                                            );
                                                }
                                            }
                                            _ => {
                                                log::debug!("  content type: Other");
                                            }
                                        }

                                        // Calculate this item's position in the viewport
                                        // The computed element's bounds.origin.y tells us where it is positioned
                                        // relative to the viewport (after scroll transform is applied)

                                        let item_top = computed_item.content_rect.origin.y;
                                        let item_bottom = item_top + rendered_height;

                                        // Check if this item is visible at all
                                        let is_visible =
                                            item_bottom > 0.0 && item_top < viewport_height;

                                        // Get height tracker for this item
                                        let tracker = self
                                            .height_trackers
                                            .entry(item_id.clone())
                                            .or_default();

                                        if is_visible {
                                            // Cache height for any visible item (partial or full)
                                            // IMPORTANT: We cache partially visible items because border_rect provides
                                            // the full unclipped height. This is critical for preventing jumps when
                                            // tall items go in/out of the render buffer.
                                            let old_height = self
                                                .activity_log_height_cache
                                                .get(&item_id)
                                                .copied();

                                            // Only update if change is significant (hysteresis)
                                            let should_update = if let Some(old) = old_height {
                                                (old - rendered_height).abs()
                                                    > HEIGHT_CHANGE_HYSTERESIS
                                            } else {
                                                true // Always cache if we don't have a height yet
                                            };

                                            if should_update {
                                                // Calculate height diff for scroll adjustment
                                                let height_diff = if let Some(old) = old_height {
                                                    rendered_height - old
                                                } else {
                                                    // First time caching - don't adjust scroll
                                                    0.0
                                                };

                                                // If this item extends above the viewport, adjust scroll to maintain position
                                                if item_top < 0.0
                                                    && height_diff.abs() > HEIGHT_CHANGE_HYSTERESIS
                                                {
                                                    self.activity_log_scroll_offset += height_diff;
                                                    log::info!(
                                                "[VSCROLL] Adjusting scroll by {:.0}px for item {} above viewport (old={:.0}, new={:.0}, was_cached={})",
                                                height_diff, item_id, old_height.unwrap_or(0.0), rendered_height, old_height.is_some()
                                            );
                                                }

                                                self.activity_log_height_cache
                                                    .insert(item_id.clone(), rendered_height);
                                                tracker.seen_full_height = true;
                                                tracker.measured_height = Some(rendered_height);

                                                if let Some(old) = old_height {
                                                    log::info!(
                                                "[VSCROLL] Height updated for {}: {:.0}px -> {:.0}px (delta: {:.1}px)",
                                                item_id, old, rendered_height, rendered_height - old
                                            );
                                                }
                                            }

                                            // Log tall items for debugging
                                            if rendered_height >= viewport_height {
                                                log::debug!(
                                            "[VSCROLL] Tall item {} cached: height={:.0}px, viewport={:.0}px",
                                            item_id, rendered_height, viewport_height
                                        );
                                            }
                                        } else {
                                            // Item is not visible at all
                                            log::trace!(
                                        "Item {} not visible (top: {}, bottom: {}), skipping cache",
                                        item_id, item_top, item_bottom
                                    );
                                        }
                                    }
                                }
                            }

                            log::debug!(
                                "Updated {} height cache entries from rendered elements (total cache size: {})",
                                item_elements.len(),
                                self.activity_log_height_cache.len()
                            );

                            // Drop filtered_items by ending this scope
                        }
                    }
                }
            }
        }

        // STICKY BOTTOM DISABLED - This feature was causing scroll jumps because:
        // 1. It uses hardcoded line_height (20.0) vs actual font metrics
        // 2. It recalculates total height with different parameters than render_activity_log
        // 3. This causes massive jumps (3000+ pixels) when heights don't match
        // 4. It doesn't have access to proper font metrics to calculate correctly
        /*
        // Handle sticky bottom
        if sticky_bottom_needed {
                    let viewport_height = self.activity_log_scrollbar
                        .as_ref()
                        .map(|s| s.viewport_height)
                        .unwrap_or(400.0);

                    // Recalculate filtered items for total height
                    let filtered_items: Vec<(usize, &ActivityItem)> = self
                        .activity_log
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| match self.activity_filter {
                            ActivityFilter::All => true,
                            ActivityFilter::Commands => matches!(item, ActivityItem::Command { .. }),
                            ActivityFilter::Chat => matches!(item, ActivityItem::Chat { .. }),
                            ActivityFilter::Suggestions => matches!(item, ActivityItem::Suggestion { .. }),
                        })
                        .collect();

                    // CRITICAL: These values MUST match what's used in render_activity_log!
                    let line_height = 20.0; // This might not match the actual line height!
                    let available_width = self.activity_log_last_width.unwrap_or(300.0);

                    log::debug!(
                        "[VSCROLL] Sticky bottom params: line_height={:.0}, width={:.0}, items={}",
                        line_height, available_width, filtered_items.len()
                    );

                    let new_total_height = self.calculate_total_activity_log_height(
                        &filtered_items,
                        line_height,
                        available_width
                    );

                    let new_max_scroll = (new_total_height - viewport_height).max(0.0);
            if (self.activity_log_scroll_offset - new_max_scroll).abs() > 1.0 {
                log::warn!(
                    "[VSCROLL] STICKY BOTTOM JUMP: scroll {} -> {} (total_height={:.0}, viewport={:.0})",
                    self.activity_log_scroll_offset, new_max_scroll, new_total_height, viewport_height
                );
                self.activity_log_scroll_offset = new_max_scroll;
            }
        }
        */
    }
}

// Static helper functions for virtual scrolling

/// Render an activity item (static version for use in closures)
fn render_activity_item_static(
    item: &ActivityItem,
    fonts: &SidebarFonts,
    idx: usize,
    palette: &wezterm_term::color::ColorPalette,
) -> Element {
    // This is a static version of render_activity_item that doesn't need &mut self
    match item {
        ActivityItem::Command {
            command,
            status,
            output,
            expanded,
            ..
        } => {
            let status_icon = match status {
                CommandStatus::Running => "▶",
                CommandStatus::Success => "✓",
                CommandStatus::Failed(_) => "✗",
            };

            let status_color = match status {
                CommandStatus::Running => LinearRgba::with_components(1.0, 1.0, 0.0, 1.0),
                CommandStatus::Success => LinearRgba::with_components(0.0, 1.0, 0.0, 1.0),
                CommandStatus::Failed(_) => LinearRgba::with_components(1.0, 0.0, 0.0, 1.0),
            };

            let mut children = vec![Element::new(
                &fonts.body,
                ElementContent::Text(format!("{} $ {}", status_icon, command)),
            )
            .colors(ElementColors {
                text: status_color.into(),
                ..Default::default()
            })
            .display(DisplayType::Block)];

            if *expanded {
                if let Some(output) = output {
                    children.push(
                        Element::new(&fonts.body, ElementContent::Text(output.clone()))
                            .colors(ElementColors {
                                text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
                                ..Default::default()
                            })
                            .padding(BoxDimension {
                                left: Dimension::Pixels(16.0),
                                ..Default::default()
                            })
                            .display(DisplayType::Block),
                    );
                }
            }

            Element::new(&fonts.body, ElementContent::Children(children))
                .display(DisplayType::Block)
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        }
        ActivityItem::Chat {
            message, is_user, ..
        } => {
            if *is_user {
                Element::new(&fonts.body, ElementContent::WrappedText(message.clone()))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                        bg: LinearRgba::with_components(0.2, 0.2, 0.3, 0.3).into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension::new(Dimension::Pixels(8.0)))
                    .display(DisplayType::Block)
                    .margin(BoxDimension {
                        left: Dimension::Pixels(40.0),
                        right: Dimension::Pixels(8.0),
                        top: Dimension::Pixels(4.0),
                        bottom: Dimension::Pixels(4.0),
                    })
            } else {
                // AI messages - render with markdown
                MarkdownRenderer::render_with_fonts(
                    message, fonts, None, // max_width
                )
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
                .display(DisplayType::Block)
                .margin(BoxDimension {
                    left: Dimension::Pixels(8.0),
                    right: Dimension::Pixels(40.0),
                    top: Dimension::Pixels(4.0),
                    bottom: Dimension::Pixels(4.0),
                })
            }
        }
        ActivityItem::Suggestion { title, content, .. } => {
            MarkdownRenderer::render_with_fonts(
                &format!("**{}**\n\n{}", title, content),
                fonts,
                None, // max_width
            )
            .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        }
        ActivityItem::Goal { text, .. } => {
            Element::new(&fonts.body, ElementContent::Text(format!("Goal: {}", text)))
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.8, 0.8, 0.8, 1.0).into(),
                    ..Default::default()
                })
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))
        }
    }
}

/// Estimate the height of an activity item
fn estimate_activity_item_height(
    item: &ActivityItem,
    line_height: f32,
    available_width: f32,
) -> f32 {
    // Get the correct spacing for this item type
    let spacing = AiSidebar::get_activity_item_spacing(item);

    match item {
        ActivityItem::Command {
            output, expanded, ..
        } => {
            // Command line height + spacing
            let mut height = line_height + spacing;

            // Add output height if expanded
            if *expanded {
                if let Some(output) = output {
                    let lines = output.lines().count() as f32;
                    height += lines * line_height + 16.0; // Extra padding for output
                }
            }

            height
        }
        ActivityItem::Chat {
            message, is_user, ..
        } => {
            // Estimate wrapped text height
            let horizontal_margin = CHAT_ITEM_HORIZONTAL_MARGIN; // Only one side has margin
            let horizontal_padding = CHAT_ITEM_PADDING * 2.0; // Left + right padding
            let border_width = CHAT_ITEM_BORDER * 2.0; // Left + right border
            let effective_width =
                available_width - horizontal_margin - horizontal_padding - border_width;
            let avg_char_width = line_height * 0.6; // Approximate

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                message,
                effective_width,
                avg_char_width,
            );

            lines * line_height + spacing
        }
        ActivityItem::Suggestion { content, .. } => {
            // Suggestions can be quite long with markdown
            let effective_width = available_width - spacing;
            let avg_char_width = line_height * 0.6;

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                content,
                effective_width,
                avg_char_width,
            );

            // Add extra for markdown formatting overhead
            lines * line_height * 1.2 + spacing
        }
        ActivityItem::Goal { text, .. } => {
            // Simple text with "Goal: " prefix
            let effective_width = available_width - spacing;
            let avg_char_width = line_height * 0.6;
            let full_text = format!("Goal: {}", text);

            let lines = crate::termwindow::box_model::estimate_wrapped_lines(
                &full_text,
                effective_width,
                avg_char_width,
            );

            lines * line_height + spacing
        }
    }
}
