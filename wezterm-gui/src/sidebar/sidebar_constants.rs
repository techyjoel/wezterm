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
pub const CODE_BLOCK_TOP_MARGIN: f32 = 8.0;  // Top margin for code blocks (from markdown.rs line 1260)
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
pub const CHAT_ITEM_PADDING: f32 = 12.0; // Padding on all sides
pub const CHAT_ITEM_BOTTOM_MARGIN: f32 = 8.0; // Bottom margin between chat items
pub const CHAT_ITEM_BORDER: f32 = 0.0; // No border for cleaner look
pub const CHAT_ITEM_HORIZONTAL_MARGIN: f32 = 20.0; // Left margin for user messages (AI has 0)

/// Card component constants
pub const CARD_CONTENT_PADDING: f32 = 12.0; // Default padding for Card content wrapper
pub const CARD_BORDER: f32 = 0.0; // No border for cleaner look
pub const CARD_MARGIN: f32 = 8.0; // Default margin for Card component

// Goal card constants
pub const GOAL_CARD_PADDING: f32 = 8.0; // Padding for goal text content

/// Scrollbar space allocation
pub const SCROLLBAR_SPACE: f32 = 12.0; // Space reserved for scrollbar
