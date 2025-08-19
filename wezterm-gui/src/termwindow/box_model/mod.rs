//! CSS-like box model implementation for WezTerm's UI system
//!
//! This module provides a flexible Element-based layout system similar to a simplified CSS box model.
//! It handles text wrapping, styling, rendering, and layout for UI components like sidebars and overlays.

#![allow(dead_code)]

// Submodules
pub mod element_ops;
pub mod types;
pub mod wrapping;

// Re-export core types from types module
pub use types::{
    AsciiStyleMapper, BorderColor, BoxDimension, ClipBounds, ComputedElement,
    ComputedElementContent, Corners, DisplayType, Element, ElementCell, ElementColors,
    ElementContent, Float, FontStyleFlags, GlyphPositionMap, InheritableColor, LayerScissor,
    LayoutContext, PixelCorners, PixelDimension, PixelSizedPoly, RenderSource, Rects,
    SemanticType, SizedPoly, StyleSpan, VerticalAlign, WrappedLine,
};

// Re-export public functions
pub use types::{
    estimate_wrapped_line_count, estimate_wrapped_lines, get_width_correction_factor,
    set_width_correction_factor, truncate_to_wrapped_lines, FONT_WIDTH_CACHE,
    FONT_WIDTH_CACHE_SIZE,
};