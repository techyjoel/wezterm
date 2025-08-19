//! Type definitions for the box model system

use crate::color::LinearRgba;
use crate::customglyph::{BlockKey, Poly};
use crate::glyphcache::CachedGlyph;
use crate::quad::{QuadImpl, QuadTrait};
use crate::termwindow::{ColorEase, RenderState, UIItem, UIItemType};
use crate::utilsprites::RenderMetrics;
use ::window::{RectF, WindowOps};
use config::{Dimension, DimensionContext};
use finl_unicode::grapheme_clusters::Graphemes;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use termwiz::cell::Presentation;
use termwiz::surface::Line;
use unicode_segmentation::UnicodeSegmentation;
use wezterm_font::units::PixelUnit;
use wezterm_font::{LoadedFont, LoadedFontId};
use wezterm_term::color::{ColorAttribute, ColorPalette};
use window::bitmaps::atlas::Sprite;

/// Semantic type information for markdown elements
/// Used to preserve content type through the rendering pipeline
#[derive(Debug, Clone, PartialEq)]
pub enum SemanticType {
    /// Heading with level (1-6) from markdown
    Heading(pulldown_cmark::HeadingLevel),
    /// Regular paragraph text
    Paragraph,
    /// Code block with optional language identifier
    CodeBlock { language: Option<String> },
    /// List item with ordered/unordered flag and nesting depth
    ListItem { ordered: bool, depth: usize },
    /// Inline code
    InlineCode,
    /// Bold text
    Bold,
    /// Italic text
    Italic,
    /// Link with URL
    Link { url: String },
}

/// Maximum number of fonts to cache character widths for
pub const FONT_WIDTH_CACHE_SIZE: usize = 100;

thread_local! {
    /// Cache for font character widths to avoid re-calculating on every wrap
    pub static FONT_WIDTH_CACHE: RefCell<HashMap<LoadedFontId, f32>> = RefCell::new(HashMap::new());

    /// Thread-local width correction factor for text wrapping calculations
    /// Default is 1.02 (2% extra width)
    ///
    /// The 1.02 factor was empirically determined to prevent text overflow
    /// in most cases while minimizing wasted space. It compensates for:
    /// - Kerning and ligatures that affect actual glyph positioning
    /// - Rounding errors in width calculations
    /// - Variations in character distribution vs. the sample text used for averaging
    ///
    /// Note: We use thread-local storage here because box_model doesn't have access
    /// to the config directly, and LayoutContext doesn't carry config data.
    /// This is set by the sidebar renderer before rendering markdown content.
    /// While not ideal architecturally, it's a pragmatic solution that avoids
    /// significant refactoring of the rendering pipeline.
    static WIDTH_CORRECTION_FACTOR: RefCell<f32> = RefCell::new(1.02);
}

/// Set the width correction factor for the current thread
pub fn set_width_correction_factor(factor: f32) {
    WIDTH_CORRECTION_FACTOR.with(|f| *f.borrow_mut() = factor);
}

/// Get the current width correction factor
pub fn get_width_correction_factor() -> f32 {
    WIDTH_CORRECTION_FACTOR.with(|f| *f.borrow())
}

/// Font style flags for syntax highlighting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontStyleFlags {
    pub bold: bool,
    pub italic: bool,
}

/// Style span for tracking syntax highlighting through text wrapping
#[derive(Debug, Clone)]
pub struct StyleSpan {
    pub start: usize,
    pub end: usize,
    pub colors: ElementColors,
    /// Optional font override for bold/italic
    pub font: Option<Rc<LoadedFont>>,
    /// Font style flags (bold/italic) from syntax highlighting
    pub font_style: Option<FontStyleFlags>,
}

impl StyleSpan {
    /// Check if this span represents monospace content (no font variants)
    pub fn is_monospace(&self) -> bool {
        self.font_style.is_none() && self.font.is_none()
    }

    /// Validate a collection of style spans
    pub fn validate_spans(spans: &[StyleSpan], text_len: usize) -> Result<(), String> {
        for (i, span) in spans.iter().enumerate() {
            // Check individual span validity
            if span.start >= span.end {
                return Err(format!(
                    "Style span {} has invalid range: start {} >= end {}",
                    i, span.start, span.end
                ));
            }

            if span.end > text_len {
                return Err(format!(
                    "Style span {} exceeds text length: end {} > text_len {}",
                    i, span.end, text_len
                ));
            }

            // Check for overlaps with previous spans
            for (j, other) in spans[..i].iter().enumerate() {
                if span.start < other.end && span.end > other.start {
                    log::debug!(
                        "Warning: Style spans {} and {} overlap: [{}, {}) and [{}, {})",
                        j,
                        i,
                        other.start,
                        other.end,
                        span.start,
                        span.end
                    );
                }
            }
        }
        Ok(())
    }
}

/// ASCII-only style mapper for syntax highlighting
pub struct AsciiStyleMapper {
    text: String,
    byte_to_grapheme: Vec<usize>,
    grapheme_to_byte_range: Vec<(usize, usize)>,
    grapheme_to_cell: Vec<Option<(usize, usize)>>,
}

impl AsciiStyleMapper {
    pub fn new(text: &str) -> Self {
        let mut mapper = Self {
            text: text.to_string(),
            byte_to_grapheme: vec![0; text.len()],
            grapheme_to_byte_range: Vec::new(),
            grapheme_to_cell: Vec::new(),
        };

        // Build byte-grapheme mapping
        let mut byte_idx = 0;
        for (g_idx, grapheme) in text.graphemes(true).enumerate() {
            let grapheme_bytes = grapheme.len();
            mapper
                .grapheme_to_byte_range
                .push((byte_idx, byte_idx + grapheme_bytes));

            for b in byte_idx..byte_idx + grapheme_bytes {
                mapper.byte_to_grapheme[b] = g_idx;
            }
            byte_idx += grapheme_bytes;
        }

        mapper
    }

    pub fn track_wrapping_with_lines(
        &mut self,
        wrapped_lines: &[Vec<ElementCell>],
        wrapped_line_info: &[WrappedLine],
    ) {
        self.grapheme_to_cell.clear();
        self.grapheme_to_cell
            .resize(self.grapheme_to_byte_range.len(), None);

        // Process each wrapped line accounting for skipped spaces
        for (line_idx, (line_cells, line_info)) in wrapped_lines
            .iter()
            .zip(wrapped_line_info.iter())
            .enumerate()
        {
            // Find the starting grapheme index for this line
            let line_start_byte =
                if line_info.skip_leading_spaces && line_info.leading_space_bytes > 0 {
                    line_info.byte_offset + line_info.leading_space_bytes
                } else {
                    line_info.byte_offset
                };

            // Get the grapheme index for the first non-skipped byte
            let mut grapheme_idx = if line_start_byte < self.byte_to_grapheme.len() {
                self.byte_to_grapheme[line_start_byte]
            } else {
                continue;
            };

            // Map cells to graphemes for this line
            for (cell_idx, cell) in line_cells.iter().enumerate() {
                match cell {
                    ElementCell::Glyph(_) | ElementCell::GlyphWithCluster { .. } => {
                        if grapheme_idx < self.grapheme_to_cell.len() {
                            self.grapheme_to_cell[grapheme_idx] = Some((line_idx, cell_idx));
                            grapheme_idx += 1;
                        }
                    }
                    ElementCell::Sprite(_) => {
                        // Sprites are block drawing chars that already had their
                        // grapheme mapped when they were a glyph, don't advance
                    }
                }
            }
        }
    }

    pub fn track_wrapping(&mut self, wrapped_lines: &[Vec<ElementCell>]) {
        self.grapheme_to_cell.clear();
        self.grapheme_to_cell
            .resize(self.grapheme_to_byte_range.len(), None);

        let mut grapheme_idx = 0;

        // More sophisticated tracking that handles ElementCell types
        for (line_idx, line) in wrapped_lines.iter().enumerate() {
            for (cell_idx, cell) in line.iter().enumerate() {
                match cell {
                    ElementCell::Glyph(_) | ElementCell::GlyphWithCluster { .. } => {
                        // Only map actual glyphs from the original text
                        // TODO: This still assumes 1 glyph = 1 grapheme
                        // A full solution would need to track which glyphs
                        // came from which graphemes during shaping
                        if grapheme_idx < self.grapheme_to_cell.len() {
                            self.grapheme_to_cell[grapheme_idx] = Some((line_idx, cell_idx));
                            grapheme_idx += 1;
                        }
                    }
                    ElementCell::Sprite(_) => {
                        // Sprites (e.g., block drawing chars) aren't from original text
                        // Don't advance grapheme_idx
                    }
                }
            }
        }
    }

    pub fn get_style_for_cell(
        &self,
        line: usize,
        cell: usize,
        style_spans: &[StyleSpan],
        default_colors: &ElementColors,
    ) -> (ElementColors, Option<FontStyleFlags>) {
        // Find grapheme for this cell
        let grapheme_idx = match self
            .grapheme_to_cell
            .iter()
            .position(|&pos| pos == Some((line, cell)))
        {
            Some(idx) => idx,
            None => return (default_colors.clone(), None),
        };

        // Get byte range for this grapheme
        let (byte_start, byte_end) = match self.grapheme_to_byte_range.get(grapheme_idx) {
            Some(range) => range,
            None => return (default_colors.clone(), None),
        };

        // Bounds check before accessing text slice
        if *byte_start >= self.text.len() || *byte_end > self.text.len() {
            log::warn!(
                "Style span byte range [{}, {}) exceeds text length {}",
                byte_start,
                byte_end,
                self.text.len()
            );
            return (default_colors.clone(), None);
        }

        // ASCII-ONLY CHECK: Skip coloring for non-ASCII
        let text_slice = &self.text[*byte_start..*byte_end];

        // Check for actual character count, not byte length
        // Single character that might be multi-byte UTF-8 is OK
        let char_count = text_slice.chars().count();
        if char_count > 1 {
            // This is a ligature or multi-character grapheme
            return (default_colors.clone(), None);
        }

        // Non-ASCII gets default color
        // Whitespace (including tabs) doesn't get syntax coloring
        if !text_slice.chars().all(|c| c.is_ascii_graphic()) {
            return (default_colors.clone(), None);
        }

        // Find style span for single ASCII characters with validation
        let found_span = style_spans.iter().find(|span| {
            // Validate span bounds
            if span.start > self.text.len() || span.end > self.text.len() {
                log::warn!(
                    "Invalid style span [{}, {}) for text length {}",
                    span.start,
                    span.end,
                    self.text.len()
                );
                false
            } else {
                *byte_start >= span.start && *byte_start < span.end
            }
        });

        match found_span {
            Some(span) => (span.colors.clone(), span.font_style),
            None => (default_colors.clone(), None),
        }
    }
}

/// Estimate how many lines text will wrap to given available width
/// This is a simplified version of the wrap_text logic for quick estimation
/// Used by both the activity log height calculation and suggestion card truncation
pub fn estimate_wrapped_lines(text: &str, available_width: f32, avg_char_width: f32) -> f32 {
    let chars_per_line = (available_width / avg_char_width).floor().max(1.0);

    // Count words and estimate wrapping
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines = 1.0;
    let mut current_line_chars = 0.0;

    for word in words {
        let word_chars = word.len() as f32 + 1.0; // +1 for space
        if current_line_chars + word_chars > chars_per_line && current_line_chars > 0.0 {
            lines += 1.0;
            current_line_chars = word_chars;
        } else {
            current_line_chars += word_chars;
        }
    }

    lines
}

/// Integer version for when you need line count as usize
pub fn estimate_wrapped_line_count(text: &str, available_width: f32, avg_char_width: f32) -> usize {
    estimate_wrapped_lines(text, available_width, avg_char_width).ceil() as usize
}

/// Truncate text to fit within a specified number of lines
/// Returns the truncated text that will fit within max_lines when wrapped
pub fn truncate_to_wrapped_lines(
    text: &str,
    available_width: f32,
    avg_char_width: f32,
    max_lines: usize,
) -> String {
    let chars_per_line = (available_width / avg_char_width) as usize;
    if chars_per_line == 0 {
        return String::new();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut truncated_words = Vec::new();
    let mut line_count = 1;
    let mut current_line_chars = 0;

    for word in &words {
        let word_len = word.len() + 1; // +1 for space

        // Check if adding this word would exceed max lines
        if current_line_chars + word_len > chars_per_line && current_line_chars > 0 {
            line_count += 1;
            current_line_chars = word_len;

            if line_count > max_lines {
                // Stop before this word to stay within max lines
                break;
            }
        } else {
            current_line_chars += word_len;
        }

        truncated_words.push(*word);
    }

    truncated_words.join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalAlign {
    Top,
    Bottom,
    Middle,
}

impl Default for VerticalAlign {
    fn default() -> VerticalAlign {
        VerticalAlign::Top
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayType {
    Block,
    Inline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Float {
    None,
    Right,
}

impl Default for Float {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PixelDimension {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PixelSizedPoly {
    pub poly: &'static [Poly],
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SizedPoly {
    pub poly: &'static [Poly],
    pub width: Dimension,
    pub height: Dimension,
}

impl SizedPoly {
    pub fn to_pixels(&self, context: &LayoutContext) -> PixelSizedPoly {
        PixelSizedPoly {
            poly: self.poly,
            width: self.width.evaluate_as_pixels(context.width),
            height: self.height.evaluate_as_pixels(context.height),
        }
    }

    pub fn none() -> Self {
        Self {
            poly: &[],
            width: Dimension::default(),
            height: Dimension::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PixelCorners {
    pub top_left: PixelSizedPoly,
    pub top_right: PixelSizedPoly,
    pub bottom_left: PixelSizedPoly,
    pub bottom_right: PixelSizedPoly,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Corners {
    pub top_left: SizedPoly,
    pub top_right: SizedPoly,
    pub bottom_left: SizedPoly,
    pub bottom_right: SizedPoly,
}

impl Corners {
    pub fn to_pixels(&self, context: &LayoutContext) -> PixelCorners {
        PixelCorners {
            top_left: self.top_left.to_pixels(context),
            top_right: self.top_right.to_pixels(context),
            bottom_left: self.bottom_left.to_pixels(context),
            bottom_right: self.bottom_right.to_pixels(context),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BoxDimension {
    pub left: Dimension,
    pub top: Dimension,
    pub right: Dimension,
    pub bottom: Dimension,
}

impl BoxDimension {
    pub const fn new(dim: Dimension) -> Self {
        Self {
            left: dim,
            top: dim,
            right: dim,
            bottom: dim,
        }
    }

    pub fn to_pixels(&self, context: &LayoutContext) -> PixelDimension {
        PixelDimension {
            left: self.left.evaluate_as_pixels(context.width),
            top: self.top.evaluate_as_pixels(context.height),
            right: self.right.evaluate_as_pixels(context.width),
            bottom: self.bottom.evaluate_as_pixels(context.height),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InheritableColor {
    Inherited,
    Color(LinearRgba),
    Animated {
        color: LinearRgba,
        alt_color: LinearRgba,
        ease: Rc<RefCell<ColorEase>>,
        one_shot: bool,
    },
}

impl Default for InheritableColor {
    fn default() -> Self {
        Self::Inherited
    }
}

impl From<LinearRgba> for InheritableColor {
    fn from(color: LinearRgba) -> InheritableColor {
        InheritableColor::Color(color)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct BorderColor {
    pub left: LinearRgba,
    pub top: LinearRgba,
    pub right: LinearRgba,
    pub bottom: LinearRgba,
}

impl BorderColor {
    pub const fn new(color: LinearRgba) -> Self {
        Self {
            left: color,
            top: color,
            right: color,
            bottom: color,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ElementColors {
    pub border: BorderColor,
    pub bg: InheritableColor,
    pub text: InheritableColor,
}

impl ElementColors {
    /// Check if this has default values
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

pub(crate) struct ResolvedColor {
    pub(crate) color: LinearRgba,
    pub(crate) alt_color: LinearRgba,
    pub(crate) mix_value: f32,
    pub(crate) alpha_override: Option<f32>,
}

impl ResolvedColor {
    pub(crate) fn apply(&self, quad: &mut QuadImpl) {
        if let Some(alpha) = self.alpha_override {
            // Apply colors with alpha override
            let color_with_alpha =
                LinearRgba::with_components(self.color.0, self.color.1, self.color.2, alpha);
            let alt_color_with_alpha = LinearRgba::with_components(
                self.alt_color.0,
                self.alt_color.1,
                self.alt_color.2,
                alpha,
            );
            quad.set_fg_color(color_with_alpha);
            quad.set_alt_color_and_mix_value(alt_color_with_alpha, self.mix_value);
        } else {
            // Normal behavior - use colors as-is
            quad.set_fg_color(self.color);
            quad.set_alt_color_and_mix_value(self.alt_color, self.mix_value);
        }
    }
}

impl From<LinearRgba> for ResolvedColor {
    fn from(color: LinearRgba) -> Self {
        Self {
            color,
            alt_color: color,
            mix_value: 0.,
            alpha_override: None,
        }
    }
}

/// Specifies how an element should be clipped
#[derive(Debug, Clone)]
pub enum ClipBounds {
    /// Clip to the element's content rect
    ContentBounds,
    /// Clip to explicit dimensions
    Explicit { width: Dimension, height: Dimension },
}

/// Scissor clipping information for a render layer
#[derive(Clone, Debug)]
pub struct LayerScissor {
    /// The clipping rectangle in screen coordinates
    pub rect: euclid::default::Rect<f32>,
}

/// Core UI element with CSS-like box model properties
///
/// Elements are the building blocks of WezTerm's UI system. Each element has:
/// - Box model properties (padding, margin, border)
/// - Layout properties (display type, float, alignment)
/// - Visual properties (colors, hover states)
/// - Content (text, wrapped text, children, etc.)
///
/// Elements are processed recursively during rendering to allocate quads
/// at the appropriate z-index layers.
#[derive(Debug, Clone)]
pub struct Element {
    pub item_type: Option<UIItemType>,
    pub semantic_type: Option<SemanticType>,
    pub vertical_align: VerticalAlign,
    pub zindex: i8,
    pub display: DisplayType,
    pub float: Float,
    pub padding: BoxDimension,
    pub margin: BoxDimension,
    pub border: BoxDimension,
    pub border_corners: Option<Corners>,
    pub colors: ElementColors,
    pub hover_colors: Option<ElementColors>,
    pub font: Rc<LoadedFont>,
    pub content: ElementContent,
    pub presentation: Option<Presentation>,
    /// Global byte offset where this element's text starts in the document
    /// Used for markdown elements to maintain correct text selection positions
    pub global_byte_offset: Option<usize>,
    pub line_height: Option<f64>,
    pub max_width: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub min_height: Option<Dimension>,
    pub clip_bounds: Option<ClipBounds>,
    /// Cached height from previous render (if available)
    pub computed_height: Option<f32>,
    /// Whether this element contributes scissor bounds to its layer
    pub layer_scissor: Option<LayerScissor>,
}

impl Element {
    pub fn new(font: &Rc<LoadedFont>, content: ElementContent) -> Self {
        Self {
            item_type: None,
            semantic_type: None,
            zindex: 0,
            display: DisplayType::Inline,
            float: Float::None,
            padding: BoxDimension::default(),
            margin: BoxDimension::default(),
            border: BoxDimension::default(),
            border_corners: None,
            vertical_align: VerticalAlign::default(),
            colors: ElementColors::default(),
            hover_colors: None,
            font: Rc::clone(font),
            content,
            presentation: None,
            global_byte_offset: None,
            line_height: None,
            max_width: None,
            min_width: None,
            min_height: None,
            clip_bounds: None,
            computed_height: None,
            layer_scissor: None,
        }
    }

    pub fn with_transparent_bg(font: &Rc<LoadedFont>, content: ElementContent) -> Self {
        Element::new(font, content).colors(ElementColors {
            border: BorderColor::default(),
            bg: LinearRgba::TRANSPARENT.into(),
            text: InheritableColor::Inherited,
        })
    }

    pub fn transparent_bg(mut self) -> Self {
        self.colors.bg = LinearRgba::TRANSPARENT.into();
        self
    }

    pub fn with_line(font: &Rc<LoadedFont>, line: &Line, palette: &ColorPalette) -> Self {
        let mut content: Vec<Element> = vec![];
        let mut prior_attr = None;

        for cluster in line.cluster(None) {
            // Clustering may introduce cluster boundaries when the text hasn't actually
            // changed style. Undo that here.
            // There's still an issue where the style does actually change and we
            // subsequently don't clip the element.
            // <https://github.com/wezterm/wezterm/issues/2560>
            if let Some(prior) = content.last_mut() {
                let (fg, bg) = prior_attr.as_ref().unwrap();
                if cluster.attrs.background() == *bg && cluster.attrs.foreground() == *fg {
                    if let ElementContent::Text(t) = &mut prior.content {
                        t.push_str(&cluster.text);
                        continue;
                    }
                }
            }

            let child =
                Element::new(font, ElementContent::Text(cluster.text)).colors(ElementColors {
                    border: BorderColor::default(),
                    bg: if cluster.attrs.background() == ColorAttribute::Default {
                        InheritableColor::Inherited
                    } else {
                        palette
                            .resolve_bg(cluster.attrs.background())
                            .to_linear()
                            .into()
                    },
                    text: if cluster.attrs.foreground() == ColorAttribute::Default {
                        InheritableColor::Inherited
                    } else {
                        palette
                            .resolve_fg(cluster.attrs.foreground())
                            .to_linear()
                            .into()
                    },
                });

            content.push(child);
            prior_attr.replace((cluster.attrs.foreground(), cluster.attrs.background()));
        }

        Self::new(font, ElementContent::Children(content))
    }

    pub fn with_line_transparent_bg(
        font: &Rc<LoadedFont>,
        line: &Line,
        palette: &ColorPalette,
    ) -> Self {
        let mut content: Vec<Element> = vec![];
        let mut prior_attr = None;

        for cluster in line.cluster(None) {
            // Clustering may introduce cluster boundaries when the text hasn't actually
            // changed style. Undo that here.
            if let Some(prior) = content.last_mut() {
                let (fg, _bg) = prior_attr.as_ref().unwrap();
                if cluster.attrs.foreground() == *fg {
                    if let ElementContent::Text(t) = &mut prior.content {
                        t.push_str(&cluster.text);
                        continue;
                    }
                }
            }

            let child =
                Element::new(font, ElementContent::Text(cluster.text)).colors(ElementColors {
                    border: BorderColor::default(),
                    // Always use transparent background regardless of cell attributes
                    bg: LinearRgba::TRANSPARENT.into(),
                    text: if cluster.attrs.foreground() == ColorAttribute::Default {
                        InheritableColor::Inherited
                    } else {
                        palette
                            .resolve_fg(cluster.attrs.foreground())
                            .to_linear()
                            .into()
                    },
                });

            content.push(child);
            prior_attr.replace((cluster.attrs.foreground(), cluster.attrs.background()));
        }

        Self::new(font, ElementContent::Children(content))
    }

    pub fn vertical_align(mut self, align: VerticalAlign) -> Self {
        self.vertical_align = align;
        self
    }

    pub fn item_type(mut self, item_type: UIItemType) -> Self {
        self.item_type.replace(item_type);
        self
    }

    pub fn semantic_type(mut self, semantic_type: SemanticType) -> Self {
        self.semantic_type.replace(semantic_type);
        self
    }

    pub fn global_byte_offset(mut self, offset: usize) -> Self {
        self.global_byte_offset = Some(offset);
        self
    }

    pub fn display(mut self, display: DisplayType) -> Self {
        self.display = display;
        self
    }

    pub fn float(mut self, float: Float) -> Self {
        self.float = float;
        self
    }

    pub fn colors(mut self, colors: ElementColors) -> Self {
        self.colors = colors;
        self
    }

    pub fn hover_colors(mut self, colors: Option<ElementColors>) -> Self {
        self.hover_colors = colors;
        self
    }

    pub fn line_height(mut self, line_height: Option<f64>) -> Self {
        self.line_height = line_height;
        self
    }

    /// Builder method to set computed height
    pub fn with_computed_height(mut self, height: f32) -> Self {
        self.computed_height = Some(height);
        self
    }

    pub fn zindex(mut self, zindex: i8) -> Self {
        self.zindex = zindex;
        self
    }

    pub fn padding(mut self, padding: BoxDimension) -> Self {
        self.padding = padding;
        self
    }

    pub fn border(mut self, border: BoxDimension) -> Self {
        self.border = border;
        self
    }

    pub fn border_corners(mut self, corners: Option<Corners>) -> Self {
        self.border_corners = corners;
        self
    }

    pub fn margin(mut self, margin: BoxDimension) -> Self {
        self.margin = margin;
        self
    }

    pub fn max_width(mut self, width: Option<Dimension>) -> Self {
        self.max_width = width;
        self
    }

    pub fn min_width(mut self, width: Option<Dimension>) -> Self {
        self.min_width = width;
        self
    }

    pub fn min_height(mut self, height: Option<Dimension>) -> Self {
        self.min_height = height;
        self
    }

    pub fn clip_bounds(mut self, bounds: Option<ClipBounds>) -> Self {
        self.clip_bounds = bounds;
        self
    }

    pub fn with_clip_bounds(mut self, bounds: ClipBounds) -> Self {
        self.clip_bounds = Some(bounds);
        self
    }

    /// Mark element to contribute scissor bounds
    pub fn with_layer_scissor(mut self, viewport: euclid::default::Rect<f32>) -> Self {
        self.layer_scissor = Some(LayerScissor { rect: viewport });
        self
    }

    /// Compute absolute clip bounds from element's clip_bounds specification
    pub(crate) fn compute_clip_bounds(&self, context: &LayoutContext, rects: &Rects) -> Option<RectF> {
        self.clip_bounds.as_ref().map(|bounds| {
            let result = match bounds {
                ClipBounds::ContentBounds => {
                    // Clip to the content rect, translated to absolute coordinates
                    rects.content_rect.translate(rects.translate)
                }
                ClipBounds::Explicit { width, height } => {
                    // Compute explicit dimensions and create rect
                    let clip_width = width.evaluate_as_pixels(context.width);
                    let clip_height = height.evaluate_as_pixels(context.height);
                    RectF::new(
                        rects.content_rect.origin + rects.translate,
                        euclid::size2(clip_width, clip_height),
                    )
                }
            };
            log::trace!(
                "compute_clip_bounds: bounds={:?}, content_rect={:?}, translate={:?}, result={:?}",
                bounds,
                rects.content_rect,
                rects.translate,
                result
            );
            result
        })
    }

    pub(crate) fn compute_rects(&self, context: &LayoutContext, content_rect: RectF) -> Rects {
        let padding = self.padding.to_pixels(context);
        let margin = self.margin.to_pixels(context);
        let border = self.border.to_pixels(context);

        let padding = euclid::rect(
            content_rect.min_x() - padding.left,
            content_rect.min_y() - padding.top,
            content_rect.width() + padding.left + padding.right,
            content_rect.height() + padding.top + padding.bottom,
        );

        let border_rect = euclid::rect(
            padding.min_x() - border.left,
            padding.min_y() - border.top,
            padding.width() + border.left + border.right,
            padding.height() + border.top + border.bottom,
        );

        let bounds = euclid::rect(
            border_rect.min_x() - margin.left,
            border_rect.min_y() - margin.top,
            border_rect.width() + margin.left + margin.right,
            border_rect.height() + margin.top + margin.bottom,
        );
        let translate = euclid::vec2(
            context.bounds.min_x() - bounds.min_x(),
            context.bounds.min_y() - bounds.min_y(),
        );
        Rects {
            padding: padding.translate(translate),
            border_rect: border_rect.translate(translate),
            bounds: bounds.translate(translate),
            content_rect: content_rect.translate(translate),
            translate,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ElementContent {
    Text(String),
    WrappedText(String), // Automatically wraps at word boundaries, falling back to character boundaries
    Children(Vec<Element>),
    Poly {
        line_width: isize,
        poly: SizedPoly,
    },
    StyledWrappedText {
        text: String,
        style_spans: Vec<StyleSpan>,
    },
}

/// Identifies the source of rendering to enable context-specific optimizations
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RenderSource {
    /// Terminal rendering - performance critical, no cluster tracking
    Terminal,
    /// Sidebar rendering - can track exact glyph positions
    Sidebar,
    /// Tab bar rendering - no cluster tracking needed
    TabBar,
}

pub struct LayoutContext<'a> {
    pub width: DimensionContext,
    pub height: DimensionContext,
    pub bounds: RectF,
    pub metrics: &'a RenderMetrics,
    pub gl_state: &'a RenderState,
    pub zindex: i8,
    /// Identifies the rendering source to enable context-specific behavior
    pub source: RenderSource,
}

#[derive(Debug, Clone)]
pub struct ComputedElement {
    pub item_type: Option<UIItemType>,
    pub semantic_type: Option<SemanticType>,
    pub zindex: i8,
    /// The outer bounds of the element box (its margin)
    pub bounds: RectF,
    /// The outer bounds of the area enclosed by its border
    pub border_rect: RectF,
    pub border: PixelDimension,
    pub border_corners: Option<PixelCorners>,
    pub colors: ElementColors,
    pub hover_colors: Option<ElementColors>,
    /// The outer bounds of the area enclosed by the padding
    pub padding: RectF,
    /// The outer bounds of the content
    pub content_rect: RectF,
    pub baseline: f32,
    /// Clip bounds in absolute window coordinates (if any)
    pub clip_bounds: Option<RectF>,
    /// Whether this element contributes scissor bounds to its layer
    pub layer_scissor: Option<LayerScissor>,
    /// Global byte offset for text elements (document-relative position)
    pub global_byte_offset: Option<usize>,

    pub content: ComputedElementContent,
}

impl ComputedElement {
    pub fn translate(&mut self, delta: euclid::Vector2D<f32, PixelUnit>) {
        self.bounds = self.bounds.translate(delta);
        self.border_rect = self.border_rect.translate(delta);
        self.padding = self.padding.translate(delta);
        self.content_rect = self.content_rect.translate(delta);

        // Also translate clip bounds if present
        if let Some(clip) = &mut self.clip_bounds {
            *clip = clip.translate(delta);
        }

        match &mut self.content {
            ComputedElementContent::Children(kids) => {
                for kid in kids {
                    kid.translate(delta)
                }
            }
            ComputedElementContent::Text(_) => {}
            ComputedElementContent::MultilineText { .. } => {}
            ComputedElementContent::Poly { .. } => {}
        }
    }

    pub fn ui_items(&self) -> Vec<UIItem> {
        let mut items = vec![];
        self.ui_item_impl(&mut items);
        items
    }

    fn ui_item_impl(&self, items: &mut Vec<UIItem>) {
        if let Some(item_type) = &self.item_type {
            let ui_item = UIItem {
                x: self.bounds.min_x().max(0.) as usize,
                y: self.bounds.min_y().max(0.) as usize,
                width: self.bounds.width().max(0.) as usize,
                height: self.bounds.height().max(0.) as usize,
                item_type: item_type.clone(),
            };

            // Debug logging for Goal text UIItems
            if matches!(&ui_item.item_type, UIItemType::GoalText { .. }) {
                log::debug!(
                    "UIITEM DEBUG: Adding GoalText UIItem - bounds: x={}, y={}, w={}, h={}, total_items_before={}",
                    ui_item.x, ui_item.y, ui_item.width, ui_item.height, items.len()
                );
            }

            items.push(ui_item);
        }

        match &self.content {
            ComputedElementContent::Text(_) => {}
            ComputedElementContent::MultilineText { .. } => {}
            ComputedElementContent::Children(kids) => {
                for kid in kids {
                    kid.ui_item_impl(items);
                }
            }
            ComputedElementContent::Poly { .. } => {}
        }
    }
}

#[derive(Debug, Clone)]
pub enum ComputedElementContent {
    Text(Vec<ElementCell>),
    MultilineText {
        lines: Vec<Vec<ElementCell>>,
        line_height: f32,
        /// Optional per-cell colors for syntax highlighting
        line_styles: Option<Vec<Vec<ElementColors>>>,
        /// Optional per-cell font styles for bold/italic
        line_font_styles: Option<Vec<Vec<Option<FontStyleFlags>>>>,
        /// Line information including byte offsets and source text
        line_info: Option<Vec<WrappedLine>>,
        /// Actual Y positions of each line as determined during layout
        line_positions: Vec<f32>,
    },
    Children(Vec<ComputedElement>),
    Poly {
        line_width: isize,
        poly: PixelSizedPoly,
    },
}

#[derive(Debug, Clone)]
pub enum ElementCell {
    Sprite(Sprite),
    Glyph(Rc<CachedGlyph>),
    // New variant that includes position-specific cluster information
    GlyphWithCluster {
        glyph: Rc<CachedGlyph>,
        cluster: u32,
    },
}

impl ElementCell {
    /// Get the glyph from either Glyph or GlyphWithCluster variant
    pub fn get_glyph(&self) -> Option<&Rc<CachedGlyph>> {
        match self {
            ElementCell::Glyph(glyph) | ElementCell::GlyphWithCluster { glyph, .. } => Some(glyph),
            ElementCell::Sprite(_) => None,
        }
    }
}

/// Represents a wrapped line of text with its byte offsets
#[derive(Debug, Clone)]
pub struct WrappedLine {
    /// Byte offset in the original text where this line starts
    pub byte_offset: usize,
    /// Byte offset in the original text where this line ends (exclusive)
    pub byte_end: usize,
    /// Whether to skip leading spaces when rendering
    pub skip_leading_spaces: bool,
    /// Number of bytes to skip at start (for leading space handling)
    pub leading_space_bytes: usize,
    /// Text actually sent to shaper (may differ from original due to skipped spaces)
    pub shaped_text: String,
    /// Byte offset of shaped_text within the line text
    pub shaped_offset: usize,
}

impl WrappedLine {
    /// Convert a cluster position (relative to shaped text) to document byte offset
    ///
    /// # Unicode Safety
    ///
    /// HarfBuzz clusters represent byte offsets that are guaranteed to be on
    /// character boundaries in the shaped text. This method preserves that
    /// guarantee by only adding offsets that also respect character boundaries
    /// (byte_offset, leading_space_bytes, and shaped_offset are all computed
    /// from character-aware string operations).
    pub fn cluster_to_byte_offset(&self, cluster: u32) -> usize {
        // Cluster is relative to shaped_text, not original document
        // Account for: line offset + skipped spaces + shaped offset + cluster
        if self.skip_leading_spaces {
            self.byte_offset + self.leading_space_bytes + cluster as usize
        } else {
            self.byte_offset + self.shaped_offset + cluster as usize
        }
    }

    /// Validate that a byte offset is on a UTF-8 character boundary
    ///
    /// # Arguments
    /// * `text` - The text to validate against
    /// * `byte_offset` - The byte offset to check
    ///
    /// # Returns
    /// * `true` if the offset is valid (on a character boundary or at text end)
    /// * `false` if the offset would split a UTF-8 sequence
    pub fn is_char_boundary(text: &str, byte_offset: usize) -> bool {
        if byte_offset == 0 || byte_offset == text.len() {
            return true;
        }
        text.is_char_boundary(byte_offset)
    }
}

/// Maps text positions to screen coordinates for accurate hit testing
#[derive(Debug, Clone)]
pub struct GlyphPositionMap {
    /// For each glyph: (byte_offset, x_start, x_end)
    pub positions: Vec<(usize, f32, f32)>,
}

impl GlyphPositionMap {
    /// Create from shaped ElementCells with cluster information
    /// Note: clusters are relative to the shaped line, not the full document
    pub fn from_cells(cells: &[ElementCell], wrapped_line: &WrappedLine) -> Self {
        let mut positions = Vec::new();
        let mut x_pos = 0.0;
        let mut glyphs_with_clusters = 0;
        let mut total_glyphs = 0;

        log::trace!(
            "GlyphPositionMap::from_cells: wrapped_line.shaped_text='{}', byte_offset={}, shaped_offset={}",
            wrapped_line.shaped_text,
            wrapped_line.byte_offset,
            wrapped_line.shaped_offset
        );

        for cell in cells {
            match cell {
                ElementCell::Glyph(cached_glyph) => {
                    total_glyphs += 1;
                    let x_start = x_pos;
                    let x_end = x_pos + cached_glyph.x_advance.get() as f32;

                    // Regular glyphs without cluster info (terminal text)
                    // are skipped for position tracking

                    x_pos = x_end;
                }
                ElementCell::GlyphWithCluster { glyph, cluster } => {
                    total_glyphs += 1;
                    glyphs_with_clusters += 1;
                    let x_start = x_pos;
                    let x_end = x_pos + glyph.x_advance.get() as f32;

                    // Cluster is relative to shaped text, convert to document offset
                    let byte_offset = wrapped_line.cluster_to_byte_offset(*cluster);

                    // Debug: log first few cluster conversions
                    if positions.len() < 3 {
                        log::trace!(
                            "  Glyph {}: cluster={}, byte_offset={}, x=({}, {})",
                            total_glyphs - 1,
                            cluster,
                            byte_offset,
                            x_start,
                            x_end
                        );
                    }

                    positions.push((byte_offset, x_start, x_end));
                    x_pos = x_end;
                }
                ElementCell::Sprite(sprite) => {
                    // Block drawing characters don't have text positions
                    x_pos += sprite.coords.size.width as f32;
                }
            }
        }

        if total_glyphs > 0 && log::log_enabled!(log::Level::Trace) {
            log::trace!(
                "GlyphPositionMap: {} of {} glyphs have cluster info, extracted {} positions",
                glyphs_with_clusters,
                total_glyphs,
                positions.len()
            );

            // Debug: if very few positions compared to glyphs, investigate
            if positions.len() < glyphs_with_clusters / 2 && glyphs_with_clusters > 5 {
                log::warn!("Very few positions extracted from glyphs with clusters: {} positions from {} glyphs", 
                          positions.len(), glyphs_with_clusters);

                // Check for duplicate byte offsets
                let mut offset_counts = std::collections::HashMap::new();
                for (offset, _, _) in &positions {
                    *offset_counts.entry(*offset).or_insert(0) += 1;
                }

                for (offset, count) in offset_counts {
                    if count > 1 {
                        log::debug!("  Byte offset {} appears {} times", offset, count);
                    }
                }
            }
        }

        GlyphPositionMap { positions }
    }

    /// Find byte offset for a given x coordinate
    pub fn hit_test(&self, x: f32) -> Option<usize> {
        // Handle click before first character
        if x < 0.0 {
            return Some(0);
        }

        // Find the glyph containing this x position
        for &(byte_offset, x_start, x_end) in &self.positions {
            if x >= x_start && x < x_end {
                // Determine if click is closer to start or end of glyph
                let mid = (x_start + x_end) / 2.0;
                if x < mid {
                    return Some(byte_offset);
                } else {
                    // Return position after this character
                    // (Need to handle multi-byte characters properly)
                    return self
                        .positions
                        .iter()
                        .find(|(offset, _, _)| *offset > byte_offset)
                        .map(|(offset, _, _)| *offset)
                        .or(Some(byte_offset + 1)); // Approximate for last char
                }
            }
        }

        // Click after last character
        self.positions.last().map(|(offset, _, _)| *offset + 1)
    }
}

#[derive(Debug)]
pub struct Rects {
    pub padding: RectF,
    pub border_rect: RectF,
    pub bounds: RectF,
    pub content_rect: RectF,
    pub translate: euclid::Vector2D<f32, PixelUnit>,
}