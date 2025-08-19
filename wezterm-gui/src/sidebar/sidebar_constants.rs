//! Shared constants for sidebar rendering and position tracking
//!
//! This module contains constants that are shared between the rendering
//! pipeline and position extraction to ensure consistency.

use crate::color::LinearRgba;

/// Default line heights for text elements
pub const PARAGRAPH_LINE_HEIGHT: f32 = 20.0;
pub const HEADING_LINE_HEIGHT_MULTIPLIER: f32 = 1.2;
pub const CODE_LINE_HEIGHT: f32 = 20.0;

/// Padding and margin constants
pub const CODE_BLOCK_PADDING: f32 = 12.0;
pub const CODE_BLOCK_BORDER: f32 = 1.0;
pub const CODE_BLOCK_TOP_MARGIN: f32 = 8.0; // Top margin for code blocks (from markdown.rs line 1260)
pub const INLINE_CODE_PADDING: f32 = 4.0;

/// Background colors for markdown elements
pub const CODE_BLOCK_BG: LinearRgba = LinearRgba(0.1, 0.1, 0.12, 1.0);
pub const INLINE_CODE_BG: LinearRgba = LinearRgba(0.15, 0.15, 0.17, 1.0);

/// Element detection thresholds
pub const CODE_BG_THRESHOLD_R: f32 = 0.2;
pub const CODE_BG_THRESHOLD_G: f32 = 0.2;
pub const CODE_BG_THRESHOLD_B: f32 = 0.25;
pub const CODE_PADDING_THRESHOLD: f32 = 20.0;

pub const INLINE_CODE_BG_MIN_R: f32 = 0.1;
pub const INLINE_CODE_BG_MAX_R: f32 = 0.2;
pub const INLINE_CODE_BG_MIN_G: f32 = 0.1;
pub const INLINE_CODE_BG_MAX_G: f32 = 0.2;
pub const INLINE_CODE_BG_MIN_B: f32 = 0.12;
pub const INLINE_CODE_BG_MAX_B: f32 = 0.25;

/// List indentation
pub const LIST_BASE_INDENT: f32 = 30.0;
pub const LIST_INDENT_STEP: f32 = 20.0;
pub const LIST_MARKER_WIDTH: f32 = 20.0;

/// Heading font size multipliers (relative to base sidebar font size)
pub const H1_FONT_SIZE_MULTIPLIER: f32 = 2.0;
pub const H2_FONT_SIZE_MULTIPLIER: f32 = 1.5;
pub const H3_FONT_SIZE_MULTIPLIER: f32 = 1.25;
pub const H4_FONT_SIZE_MULTIPLIER: f32 = 1.1;
pub const H5_FONT_SIZE_MULTIPLIER: f32 = 1.0;
pub const H6_FONT_SIZE_MULTIPLIER: f32 = 0.9;

/// Heading detection thresholds
pub const HEADING_HEIGHT_RATIO_THRESHOLD: f32 = 1.3;
pub const HEADING_H1_RATIO: f32 = 1.5;
pub const HEADING_H2_RATIO: f32 = 1.4;

/// Chat item layout constants
pub const CHAT_ITEM_PADDING: f32 = 10.0; // Padding on all sides
pub const CHAT_ITEM_BOTTOM_MARGIN: f32 = 8.0; // Bottom margin between chat items
pub const CHAT_ITEM_BORDER: f32 = 0.0; // No border for cleaner look
pub const CHAT_ITEM_HORIZONTAL_MARGIN: f32 = 20.0; // Left margin for user messages (AI has 1/2 this)

/// Card component constants
pub const CARD_CONTENT_PADDING: f32 = 12.0; // Default horizontal padding for Card content wrapper
pub const CARD_CONTENT_VERTICAL_PADDING: f32 = 4.0; // Vertical padding for Card content wrapper
pub const CARD_BORDER: f32 = 0.0; // No border for cleaner look
pub const CARD_MARGIN: f32 = 8.0; // Default margin for Card component

// Goal card constants
pub const GOAL_CARD_PADDING: f32 = 8.0; // Padding for goal text content

// Suggestion content container padding (inside the Card component)
pub const SUGGESTION_CONTENT_HORIZONTAL_PADDING: f32 = 8.0;
pub const SUGGESTION_CONTENT_VERTICAL_PADDING: f32 = 8.0; // Same as goal card

/// Scrollbar space allocation
pub const DEFAULT_SCROLLBAR_WIDTH: f32 = 8.0; // Space for scrollbar

/// Text rendering line spacing
pub const LINE_SPACING_MULTIPLIER: f32 = 1.1; // Multiplier for line height to add spacing between lines

/// Character width estimation
pub const CHAR_WIDTH_ESTIMATE: f32 = 8.5; // Approximate width of a single character
pub const CHAR_WIDTH_UPPERCASE_MULTIPLIER: f32 = 1.2; // Uppercase letters are wider

/// Chat input
pub const CHAT_INPUT_HEIGHT: f32 = 100.0;  // Not used consistently everywhere yet

// General
pub const SIDEBAR_ELEMENT_MARGIN_VERTICAL: f32 = 10.0; // Vertical margin between sidebar elements

// Selection
pub const SELECTION_CLICK_TOLERANCE: f32 = 20.0; // Allow clicking up to 20px outside text to start selection
