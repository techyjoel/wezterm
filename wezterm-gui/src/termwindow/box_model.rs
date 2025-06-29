#![allow(dead_code)]
use crate::color::LinearRgba;
use crate::customglyph::{BlockKey, Poly};
use crate::glyphcache::CachedGlyph;
use crate::quad::{QuadImpl, QuadTrait, TripleLayerQuadAllocator, TripleLayerQuadAllocatorTrait};
use crate::termwindow::{
    ColorEase, MouseCapture, RenderState, TermWindowNotif, UIItem, UIItemType,
};
use crate::utilsprites::RenderMetrics;
use ::window::{RectF, WindowOps};
use anyhow::anyhow;
use config::{Dimension, DimensionContext};
use finl_unicode::grapheme_clusters::Graphemes;
use std::cell::RefCell;
use std::rc::Rc;
use termwiz::cell::{grapheme_column_width, unicode_column_width, Presentation};
use termwiz::surface::Line;
use wezterm_font::shaper::GlyphInfo;
use wezterm_font::units::PixelUnit;
use wezterm_font::LoadedFont;
use wezterm_term::color::{ColorAttribute, ColorPalette};
use window::bitmaps::atlas::Sprite;

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

#[derive(Debug, Clone, Default)]
pub struct ElementColors {
    pub border: BorderColor,
    pub bg: InheritableColor,
    pub text: InheritableColor,
}

struct ResolvedColor {
    color: LinearRgba,
    alt_color: LinearRgba,
    mix_value: f32,
    alpha_override: Option<f32>,
}

impl ResolvedColor {
    fn apply(&self, quad: &mut QuadImpl) {
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

#[derive(Debug, Clone)]
pub struct Element {
    pub item_type: Option<UIItemType>,
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
    pub line_height: Option<f64>,
    pub max_width: Option<Dimension>,
    pub min_width: Option<Dimension>,
    pub min_height: Option<Dimension>,
    pub clip_bounds: Option<ClipBounds>,
}

impl Element {
    pub fn new(font: &Rc<LoadedFont>, content: ElementContent) -> Self {
        Self {
            item_type: None,
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
            line_height: None,
            max_width: None,
            min_width: None,
            min_height: None,
            clip_bounds: None,
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
}

#[derive(Debug, Clone)]
pub enum ElementContent {
    Text(String),
    WrappedText(String), // Automatically wraps at word boundaries, falling back to character boundaries
    Children(Vec<Element>),
    Poly { line_width: isize, poly: SizedPoly },
}

pub struct LayoutContext<'a> {
    pub width: DimensionContext,
    pub height: DimensionContext,
    pub bounds: RectF,
    pub metrics: &'a RenderMetrics,
    pub gl_state: &'a RenderState,
    pub zindex: i8,
}

#[derive(Debug, Clone)]
pub struct ComputedElement {
    pub item_type: Option<UIItemType>,
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
            items.push(UIItem {
                x: self.bounds.min_x().max(0.) as usize,
                y: self.bounds.min_y().max(0.) as usize,
                width: self.bounds.width().max(0.) as usize,
                height: self.bounds.height().max(0.) as usize,
                item_type: item_type.clone(),
            });
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
}

#[derive(Debug)]
struct Rects {
    padding: RectF,
    border_rect: RectF,
    bounds: RectF,
    content_rect: RectF,
    translate: euclid::Vector2D<f32, PixelUnit>,
}

impl Element {
    /// Compute absolute clip bounds from element's clip_bounds specification
    fn compute_clip_bounds(&self, context: &LayoutContext, rects: &Rects) -> Option<RectF> {
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

    fn compute_rects(&self, context: &LayoutContext, content_rect: RectF) -> Rects {
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

impl super::TermWindow {
    /// Wraps text at word boundaries to fit within max_width, with character-level fallback
    fn wrap_text(
        &self,
        text: &str,
        font: &Rc<LoadedFont>,
        max_width: f32,
        context: &LayoutContext,
        style: &config::TextStyle,
    ) -> anyhow::Result<Vec<Vec<ElementCell>>> {
        let mut lines = Vec::new();
        let mut current_line = Vec::new();
        let mut current_width = 0.0;

        // Split by whitespace while preserving it
        let words = text.split_inclusive(' ');

        for word in words {
            // Shape the word to get its width
            let window = self.window.as_ref().unwrap().clone();
            let word_infos = font.shape(
                word,
                move || window.notify(TermWindowNotif::InvalidateShapeCache),
                BlockKey::filter_out_synthetic,
                None, // presentation
                wezterm_bidi::Direction::LeftToRight,
                None, // range
                None, // direction override
            )?;

            // Calculate word width
            let word_width = self.calculate_text_width(word, &word_infos, font, context, style)?;

            // Check if word fits on current line
            if current_width > 0.0 && current_width + word_width > max_width {
                // Finalize current line
                if !current_line.is_empty() {
                    lines.push(current_line);
                    current_line = Vec::new();
                    current_width = 0.0;
                }
            }

            // If word itself is too wide, break it at character boundaries
            if word_width > max_width {
                let cells = self.shape_text_to_cells(word, &word_infos, font, context, style)?;
                let mut char_line = Vec::new();
                let mut char_width = 0.0;

                for cell in cells {
                    let cell_width = self.get_cell_width(&cell, context)?;

                    if char_width + cell_width > max_width && !char_line.is_empty() {
                        lines.push(char_line);
                        char_line = Vec::new();
                        char_width = 0.0;
                    }

                    char_line.push(cell);
                    char_width += cell_width;
                }

                if !char_line.is_empty() {
                    current_line = char_line;
                    current_width = char_width;
                }
            } else {
                // Word fits, add it to current line
                let cells = self.shape_text_to_cells(word, &word_infos, font, context, style)?;
                current_line.extend(cells);
                current_width += word_width;
            }
        }

        // Add final line
        if !current_line.is_empty() {
            lines.push(current_line);
        }

        Ok(lines)
    }

    /// Helper to calculate text width from shaped glyphs
    fn calculate_text_width(
        &self,
        text: &str,
        infos: &[GlyphInfo],
        font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &config::TextStyle,
    ) -> anyhow::Result<f32> {
        let mut width = 0.0;
        let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();

        for info in infos {
            // Check if it's a unicode block glyph
            let cell_start = &text[info.cluster as usize..];
            let mut iter = Graphemes::new(cell_start).peekable();
            if let Some(grapheme) = iter.next() {
                if let Some(_key) = BlockKey::from_str(grapheme) {
                    width += context.width.pixel_cell;
                    continue;
                }

                let followed_by_space = iter.peek() == Some(&" ");
                let num_cells = grapheme_column_width(grapheme, None);
                let glyph = glyph_cache.cached_glyph(
                    info,
                    style,
                    followed_by_space,
                    font,
                    context.metrics,
                    num_cells as u8,
                )?;
                width += glyph.x_advance.get() as f32;
            }
        }

        Ok(width)
    }

    /// Helper to get width of a single ElementCell
    fn get_cell_width(&self, cell: &ElementCell, context: &LayoutContext) -> anyhow::Result<f32> {
        match cell {
            ElementCell::Sprite(_) => Ok(context.width.pixel_cell),
            ElementCell::Glyph(glyph) => Ok(glyph.x_advance.get() as f32),
        }
    }

    /// Helper to convert shaped text to ElementCells
    fn shape_text_to_cells(
        &self,
        text: &str,
        infos: &[GlyphInfo],
        font: &Rc<LoadedFont>,
        context: &LayoutContext,
        style: &config::TextStyle,
    ) -> anyhow::Result<Vec<ElementCell>> {
        let mut cells = Vec::new();
        let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();

        for info in infos {
            // Check if it's a unicode block glyph
            let cell_start = &text[info.cluster as usize..];
            let mut iter = Graphemes::new(cell_start).peekable();
            if let Some(grapheme) = iter.next() {
                if let Some(key) = BlockKey::from_str(grapheme) {
                    let sprite = glyph_cache.cached_block(key, context.metrics)?;
                    cells.push(ElementCell::Sprite(sprite));
                    continue;
                }

                let followed_by_space = iter.peek() == Some(&" ");
                let num_cells = grapheme_column_width(grapheme, None);
                let glyph = glyph_cache.cached_glyph(
                    info,
                    style,
                    followed_by_space,
                    font,
                    context.metrics,
                    num_cells as u8,
                )?;
                cells.push(ElementCell::Glyph(glyph));
            }
        }

        Ok(cells)
    }

    pub fn compute_element<'a>(
        &self,
        context: &LayoutContext,
        element: &Element,
    ) -> anyhow::Result<ComputedElement> {
        let local_metrics;
        let local_context;
        let context = if let Some(line_height) = element.line_height {
            local_metrics = context.metrics.scale_line_height(line_height);
            local_context = LayoutContext {
                height: DimensionContext {
                    dpi: context.height.dpi,
                    pixel_max: context.height.pixel_max,
                    pixel_cell: context.height.pixel_cell * line_height as f32,
                },
                width: context.width,
                bounds: context.bounds,
                gl_state: context.gl_state,
                metrics: &local_metrics,
                zindex: context.zindex,
            };
            &local_context
        } else {
            context
        };
        let border_corners = element
            .border_corners
            .as_ref()
            .map(|c| c.to_pixels(context));
        let style = element.font.style();
        let border = element.border.to_pixels(context);
        let padding = element.padding.to_pixels(context);
        let baseline = context.height.pixel_cell + context.metrics.descender.get() as f32;
        let min_width = match element.min_width {
            Some(w) => w.evaluate_as_pixels(context.width),
            None => 0.0,
        };
        let min_height = match element.min_height {
            Some(h) => h.evaluate_as_pixels(context.height),
            None => 0.0,
        };

        let border_and_padding_width = border.left + border.right + padding.left + padding.right;

        let max_width = match element.max_width {
            Some(w) => {
                w.evaluate_as_pixels(context.width)
                    .min(context.bounds.width())
                    - border_and_padding_width
            }
            None => context.bounds.width() - border_and_padding_width,
        }
        .min((context.width.pixel_max - context.bounds.min_x()) - border_and_padding_width);

        match &element.content {
            ElementContent::Text(s) => {
                let window = self.window.as_ref().unwrap().clone();
                let direction = wezterm_bidi::Direction::LeftToRight;
                let infos = element.font.shape(
                    &s,
                    move || window.notify(TermWindowNotif::InvalidateShapeCache),
                    BlockKey::filter_out_synthetic,
                    element.presentation,
                    direction,
                    None,
                    None,
                )?;
                let mut computed_cells = vec![];
                let mut glyph_cache = context.gl_state.glyph_cache.borrow_mut();
                let mut pixel_width = 0.0;
                let mut x_pos = context.bounds.min_x();
                let mut min_y = 0.0f32;
                // If element has no max_width constraint, use a very large value to shape all text
                let max_x = if element.max_width.is_none() {
                    f32::MAX
                } else {
                    context.bounds.min_x() + max_width
                };

                for info in infos {
                    let cell_start = &s[info.cluster as usize..];
                    let mut iter = Graphemes::new(cell_start).peekable();
                    let grapheme = iter
                        .next()
                        .ok_or_else(|| anyhow!("info.cluster didn't map into string"))?;
                    if let Some(key) = BlockKey::from_str(grapheme) {
                        // Only break if we have a max_width constraint
                        if element.max_width.is_some()
                            && pixel_width + context.width.pixel_cell >= max_x
                        {
                            break;
                        }
                        pixel_width += context.width.pixel_cell;
                        x_pos += context.width.pixel_cell;
                        let sprite = glyph_cache.cached_block(key, context.metrics)?;
                        computed_cells.push(ElementCell::Sprite(sprite));
                    } else {
                        let next_grapheme: Option<&str> = iter.peek().map(|s| *s);
                        let followed_by_space = next_grapheme == Some(" ");
                        let num_cells = grapheme_column_width(grapheme, None);
                        let glyph = glyph_cache.cached_glyph(
                            &info,
                            style,
                            followed_by_space,
                            &element.font,
                            context.metrics,
                            num_cells as u8,
                        )?;

                        if let Some(texture) = glyph.texture.as_ref() {
                            let x_pos = x_pos + (glyph.x_offset + glyph.bearing_x).get() as f32;
                            let width = texture.coords.size.width as f32 * glyph.scale as f32;
                            // Only break if we have a max_width constraint
                            if element.max_width.is_some() && x_pos + width >= max_x {
                                break;
                            }
                        } else if element.max_width.is_some()
                            && x_pos + glyph.x_advance.get() as f32 >= max_x
                        {
                            break;
                        }

                        min_y =
                            min_y.min(baseline - (glyph.y_offset + glyph.bearing_y).get() as f32);

                        pixel_width += glyph.x_advance.get() as f32;
                        x_pos += glyph.x_advance.get() as f32;

                        computed_cells.push(ElementCell::Glyph(glyph));
                    }
                }

                let content_rect = euclid::rect(
                    0.,
                    0.,
                    pixel_width.max(min_width),
                    context.height.pixel_cell.max(min_height),
                );

                let rects = element.compute_rects(context, content_rect);
                let clip_bounds = element.compute_clip_bounds(context, &rects);

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    content: ComputedElementContent::Text(computed_cells),
                })
            }
            ElementContent::WrappedText(text) => {
                let lines = self.wrap_text(text, &element.font, max_width, context, &style)?;
                let line_height = context.height.pixel_cell;
                let num_lines = lines.len() as f32;

                // Calculate max width of all lines for proper content rect
                let mut max_line_width: f32 = 0.0;
                for line in &lines {
                    let mut line_width = 0.0;
                    for cell in line {
                        line_width += self.get_cell_width(cell, context)?;
                    }
                    max_line_width = max_line_width.max(line_width);
                }

                let content_rect = euclid::rect(
                    0.,
                    0.,
                    max_line_width.max(min_width),
                    (line_height * num_lines).max(min_height),
                );

                let rects = element.compute_rects(context, content_rect);
                let clip_bounds = element.compute_clip_bounds(context, &rects);

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    content: ComputedElementContent::MultilineText { lines, line_height },
                })
            }
            ElementContent::Children(kids) => {
                let mut block_pixel_width: f32 = 0.;
                let mut block_pixel_height: f32 = 0.;
                let mut computed_kids = vec![];
                let mut max_x: f32 = 0.;
                let mut float_width: f32 = 0.;
                let mut y_coord: f32 = 0.;

                for child in kids {
                    if child.display == DisplayType::Block {
                        y_coord += block_pixel_height;
                        block_pixel_height = 0.;
                        block_pixel_width = 0.;
                    }

                    let bounds = match child.float {
                        Float::None => euclid::rect(
                            block_pixel_width,
                            y_coord,
                            context.bounds.max_x() - (context.bounds.min_x() + block_pixel_width),
                            context.bounds.max_y() - (context.bounds.min_y() + y_coord),
                        ),
                        Float::Right => euclid::rect(
                            0.,
                            y_coord,
                            context.bounds.width(),
                            context.bounds.max_y() - (context.bounds.min_y() + y_coord),
                        ),
                    };
                    let kid = self.compute_element(
                        &LayoutContext {
                            bounds,
                            gl_state: context.gl_state,
                            height: context.height,
                            metrics: context.metrics,
                            width: DimensionContext {
                                dpi: context.width.dpi,
                                pixel_cell: context.width.pixel_cell,
                                pixel_max: max_width,
                            },
                            zindex: context.zindex + element.zindex,
                        },
                        child,
                    )?;
                    match child.float {
                        Float::Right => {
                            float_width += float_width.max(kid.bounds.width());
                        }
                        Float::None => {
                            block_pixel_width += kid.bounds.width();
                            max_x = max_x.max(block_pixel_width);
                        }
                    }
                    block_pixel_height = block_pixel_height.max(kid.bounds.height());

                    computed_kids.push(kid);
                }

                // Respect min-width
                max_x = max_x.max(min_width);

                let mut float_max_x = (max_x + float_width).min(max_width);

                let pixel_height = (y_coord + block_pixel_height).max(min_height);

                for (kid, child) in computed_kids.iter_mut().zip(kids.iter()) {
                    match child.float {
                        Float::Right => {
                            max_x = max_x.max(float_max_x);
                            let x = float_max_x - kid.bounds.width();
                            float_max_x -= kid.bounds.width();
                            kid.translate(euclid::vec2(x, 0.));
                        }
                        _ => {}
                    }
                    match child.vertical_align {
                        VerticalAlign::Bottom => {
                            kid.translate(euclid::vec2(0., pixel_height - kid.bounds.height()));
                        }
                        VerticalAlign::Middle => {
                            kid.translate(euclid::vec2(
                                0.,
                                (pixel_height - kid.bounds.height()) / 2.0,
                            ));
                        }
                        VerticalAlign::Top => {}
                    }
                }

                computed_kids.sort_by(|a, b| a.zindex.cmp(&b.zindex));

                let content_rect = euclid::rect(0., 0., max_x.min(max_width), pixel_height);
                let rects = element.compute_rects(context, content_rect);

                for kid in &mut computed_kids {
                    kid.translate(rects.translate);
                }

                let clip_bounds = element.compute_clip_bounds(context, &rects);

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    content: ComputedElementContent::Children(computed_kids),
                })
            }
            ElementContent::Poly { poly, line_width } => {
                let poly = poly.to_pixels(context);
                let content_rect = euclid::rect(0., 0., poly.width, poly.height.max(min_height));
                let rects = element.compute_rects(context, content_rect);
                let clip_bounds = element.compute_clip_bounds(context, &rects);

                Ok(ComputedElement {
                    item_type: element.item_type.clone(),
                    zindex: element.zindex + context.zindex,
                    baseline,
                    border,
                    border_corners,
                    colors: element.colors.clone(),
                    hover_colors: element.hover_colors.clone(),
                    bounds: rects.bounds,
                    border_rect: rects.border_rect,
                    padding: rects.padding,
                    content_rect: rects.content_rect,
                    clip_bounds,
                    content: ComputedElementContent::Poly {
                        poly,
                        line_width: *line_width,
                    },
                })
            }
        }
    }

    pub fn render_element<'a>(
        &self,
        element: &ComputedElement,
        gl_state: &RenderState,
        inherited_colors: Option<&ElementColors>,
    ) -> anyhow::Result<()> {
        let layer = gl_state.layer_for_zindex(element.zindex)?;
        let mut layers = layer.quad_allocator();

        let colors = match &element.hover_colors {
            Some(hc) => {
                let hovering =
                    match &self.current_mouse_event {
                        Some(event) => {
                            let mouse_x = event.coords.x as f32;
                            let mouse_y = event.coords.y as f32;
                            mouse_x >= element.bounds.min_x()
                                && mouse_x <= element.bounds.max_x()
                                && mouse_y >= element.bounds.min_y()
                                && mouse_y <= element.bounds.max_y()
                        }
                        None => false,
                    } && matches!(self.current_mouse_capture, None | Some(MouseCapture::UI));
                if hovering {
                    hc
                } else {
                    &element.colors
                }
            }
            None => &element.colors,
        };

        self.render_element_background(element, colors, &mut layers, inherited_colors)?;

        // Debug: Log clip bounds
        if let Some(clip_bounds) = element.clip_bounds {
            log::info!(
                "DEBUG: Element has clip bounds {:?}, content_rect={:?}",
                clip_bounds,
                element.content_rect
            );
        }

        let left = self.dimensions.pixel_width as f32 / -2.0;
        let top = self.dimensions.pixel_height as f32 / -2.0;
        match &element.content {
            ComputedElementContent::Text(cells) => {
                let mut pos_x = element.content_rect.min_x();
                // Check if we should apply manual clipping based on clip_bounds
                let should_clip = element.clip_bounds.is_some();
                let clip_min_x = element
                    .clip_bounds
                    .as_ref()
                    .map(|b| b.min_x())
                    .unwrap_or(f32::MIN);
                let clip_max_x = element
                    .clip_bounds
                    .as_ref()
                    .map(|b| b.max_x())
                    .unwrap_or(f32::MAX);

                for cell in cells {
                    // Don't break early if we have clip bounds - keep rendering all content
                    if !should_clip && pos_x >= element.content_rect.max_x() {
                        break;
                    }
                    match cell {
                        ElementCell::Sprite(sprite) => {
                            let width = sprite.coords.width();
                            let height = sprite.coords.height();
                            let pos_y = top + element.content_rect.min_y();

                            // Don't break early if we have clip bounds
                            if !should_clip && pos_x + width as f32 > element.content_rect.max_x() {
                                break;
                            }

                            // Manual clipping check
                            if should_clip {
                                // Apply the same left offset used in rendering to get actual screen coordinates
                                let sprite_left = pos_x + left;
                                let sprite_right = pos_x + left + width as f32;

                                // Skip sprites entirely outside clip bounds
                                if sprite_right < clip_min_x || sprite_left > clip_max_x {
                                    log::trace!("Skipping sprite outside clip bounds: sprite [{}, {}], clip [{}, {}]", 
                                        sprite_left, sprite_right, clip_min_x, clip_max_x);
                                    pos_x += width as f32;
                                    continue;
                                }

                                // Handle partially clipped sprites
                                if sprite_left < clip_min_x || sprite_right > clip_max_x {
                                    // Calculate visible portion
                                    let visible_left = sprite_left.max(clip_min_x);
                                    let visible_right = sprite_right.min(clip_max_x);
                                    let visible_width = visible_right - visible_left;

                                    // Calculate texture coordinate adjustments
                                    let sprite_width = width as f32;
                                    let left_clip_ratio =
                                        (visible_left - sprite_left) / sprite_width;
                                    let right_clip_ratio =
                                        (sprite_right - visible_right) / sprite_width;

                                    log::trace!("Partial sprite clipping: sprite [{}, {}], visible [{}, {}], clip ratios: left={:.3}, right={:.3}", 
                                        sprite_left, sprite_right, visible_left, visible_right, left_clip_ratio, right_clip_ratio);

                                    // Adjust texture coordinates
                                    let tex_coords = sprite.texture_coords();
                                    let tex_width = tex_coords.max_x() - tex_coords.min_x();
                                    let adjusted_tex_left =
                                        tex_coords.min_x() + left_clip_ratio * tex_width;
                                    let adjusted_tex_right =
                                        tex_coords.max_x() - right_clip_ratio * tex_width;

                                    // Create quad with adjusted position and texture
                                    let mut quad = layers.allocate(2)?;
                                    quad.set_position(
                                        visible_left,
                                        pos_y,
                                        visible_right,
                                        pos_y + height as f32,
                                    );
                                    self.resolve_text(colors, inherited_colors).apply(&mut quad);
                                    quad.set_texture(euclid::rect(
                                        adjusted_tex_left,
                                        tex_coords.min_y(),
                                        adjusted_tex_right - adjusted_tex_left,
                                        tex_coords.max_y() - tex_coords.min_y(),
                                    ));
                                    quad.set_hsv(None);

                                    pos_x += width as f32;
                                    continue;
                                }
                            }

                            let mut quad = layers.allocate(2)?;
                            quad.set_position(
                                pos_x + left,
                                pos_y,
                                pos_x + left + width as f32,
                                pos_y + height as f32,
                            );
                            self.resolve_text(colors, inherited_colors).apply(&mut quad);
                            quad.set_texture(sprite.texture_coords());
                            quad.set_hsv(None);
                            pos_x += width as f32;
                        }
                        ElementCell::Glyph(glyph) => {
                            if let Some(texture) = glyph.texture.as_ref() {
                                let pos_y = element.content_rect.min_y() as f32 + top
                                    - (glyph.y_offset + glyph.bearing_y).get() as f32
                                    + element.baseline;

                                // Don't break early if we have clip bounds
                                if !should_clip
                                    && pos_x + glyph.x_advance.get() as f32
                                        > element.content_rect.max_x()
                                {
                                    break;
                                }
                                let glyph_pos_x =
                                    pos_x + (glyph.x_offset + glyph.bearing_x).get() as f32;
                                let width = texture.coords.size.width as f32 * glyph.scale as f32;
                                let height = texture.coords.size.height as f32 * glyph.scale as f32;

                                // Manual clipping check for glyphs
                                if should_clip {
                                    // Apply the same left offset used in rendering to get actual screen coordinates
                                    let glyph_left = glyph_pos_x + left;
                                    let glyph_right = glyph_pos_x + left + width;

                                    // Skip glyphs entirely outside clip bounds
                                    if glyph_right < clip_min_x || glyph_left > clip_max_x {
                                        log::trace!("Skipping glyph outside clip bounds: glyph [{}, {}], clip [{}, {}]", 
                                            glyph_left, glyph_right, clip_min_x, clip_max_x);
                                        pos_x += glyph.x_advance.get() as f32;
                                        continue;
                                    }

                                    // Handle partially clipped glyphs
                                    if glyph_left < clip_min_x || glyph_right > clip_max_x {
                                        // Calculate visible portion
                                        let visible_left = glyph_left.max(clip_min_x);
                                        let visible_right = glyph_right.min(clip_max_x);
                                        let visible_width = visible_right - visible_left;

                                        // Calculate texture coordinate adjustments
                                        let glyph_width = width;
                                        let left_clip_ratio =
                                            (visible_left - glyph_left) / glyph_width;
                                        let right_clip_ratio =
                                            (glyph_right - visible_right) / glyph_width;

                                        log::trace!("Partial glyph clipping: glyph [{}, {}], visible [{}, {}], clip ratios: left={:.3}, right={:.3}", 
                                            glyph_left, glyph_right, visible_left, visible_right, left_clip_ratio, right_clip_ratio);

                                        // Adjust texture coordinates
                                        let tex_coords = texture.texture_coords();
                                        let tex_width = tex_coords.max_x() - tex_coords.min_x();
                                        let adjusted_tex_left =
                                            tex_coords.min_x() + left_clip_ratio * tex_width;
                                        let adjusted_tex_right =
                                            tex_coords.max_x() - right_clip_ratio * tex_width;

                                        // Create quad with adjusted position and texture
                                        let mut quad = layers.allocate(1)?;
                                        quad.set_position(
                                            visible_left,
                                            pos_y,
                                            visible_right,
                                            pos_y + height,
                                        );
                                        self.resolve_text(colors, inherited_colors)
                                            .apply(&mut quad);
                                        quad.set_texture(euclid::rect(
                                            adjusted_tex_left,
                                            tex_coords.min_y(),
                                            adjusted_tex_right - adjusted_tex_left,
                                            tex_coords.max_y() - tex_coords.min_y(),
                                        ));
                                        quad.set_hsv(None);
                                        quad.set_has_color(glyph.has_color);

                                        pos_x += glyph.x_advance.get() as f32;
                                        continue;
                                    }
                                }

                                let mut quad = layers.allocate(1)?;
                                quad.set_position(
                                    glyph_pos_x + left,
                                    pos_y,
                                    glyph_pos_x + left + width,
                                    pos_y + height,
                                );
                                self.resolve_text(colors, inherited_colors).apply(&mut quad);
                                quad.set_texture(texture.texture_coords());
                                quad.set_has_color(glyph.has_color);
                                quad.set_hsv(None);
                            }
                            pos_x += glyph.x_advance.get() as f32;
                        }
                    }
                }
            }
            ComputedElementContent::MultilineText { lines, line_height } => {
                let mut y_offset = 0.0;

                for (line_idx, line_cells) in lines.iter().enumerate() {
                    let mut pos_x = element.content_rect.min_x();
                    let y = element.content_rect.min_y() + (line_idx as f32 * line_height);

                    for cell in line_cells {
                        if pos_x >= element.content_rect.max_x() {
                            break;
                        }
                        match cell {
                            ElementCell::Sprite(sprite) => {
                                let width = sprite.coords.width();
                                let height = sprite.coords.height();
                                let pos_y = top + y;

                                if pos_x + width as f32 > element.content_rect.max_x() {
                                    break;
                                }

                                let mut quad = layers.allocate(2)?;
                                quad.set_position(
                                    pos_x + left,
                                    pos_y,
                                    pos_x + left + width as f32,
                                    pos_y + height as f32,
                                );
                                self.resolve_text(colors, inherited_colors).apply(&mut quad);
                                quad.set_texture(sprite.texture_coords());
                                quad.set_hsv(None);
                                pos_x += width as f32;
                            }
                            ElementCell::Glyph(glyph) => {
                                if let Some(texture) = glyph.texture.as_ref() {
                                    let pos_y = y as f32 + top
                                        - (glyph.y_offset + glyph.bearing_y).get() as f32
                                        + element.baseline;

                                    if pos_x + glyph.x_advance.get() as f32
                                        > element.content_rect.max_x()
                                    {
                                        break;
                                    }
                                    let pos_x =
                                        pos_x + (glyph.x_offset + glyph.bearing_x).get() as f32;
                                    let width =
                                        texture.coords.size.width as f32 * glyph.scale as f32;
                                    let height =
                                        texture.coords.size.height as f32 * glyph.scale as f32;

                                    let mut quad = layers.allocate(1)?;
                                    quad.set_position(
                                        pos_x + left,
                                        pos_y,
                                        pos_x + left + width,
                                        pos_y + height,
                                    );
                                    self.resolve_text(colors, inherited_colors).apply(&mut quad);
                                    quad.set_texture(texture.texture_coords());
                                    quad.set_has_color(glyph.has_color);
                                    quad.set_hsv(None);
                                }
                                pos_x += glyph.x_advance.get() as f32;
                            }
                        }
                    }
                }
            }
            ComputedElementContent::Children(kids) => {
                drop(layers);

                for kid in kids {
                    self.render_element(kid, gl_state, Some(colors))?;
                }
            }
            ComputedElementContent::Poly { poly, line_width } => {
                if element.content_rect.width() >= poly.width {
                    let mut quad = self.poly_quad(
                        &mut layers,
                        1,
                        element.content_rect.origin,
                        poly.poly,
                        *line_width,
                        euclid::size2(poly.width, poly.height),
                        LinearRgba::TRANSPARENT,
                    )?;
                    self.resolve_text(colors, inherited_colors).apply(&mut quad);
                }
            }
        }

        Ok(())
    }

    fn resolve_text(
        &self,
        colors: &ElementColors,
        inherited_colors: Option<&ElementColors>,
    ) -> ResolvedColor {
        match &colors.text {
            InheritableColor::Inherited => match inherited_colors {
                Some(colors) => self.resolve_text(colors, None),
                None => LinearRgba::TRANSPARENT.into(),
            },
            InheritableColor::Color(color) => {
                // Check if the color has non-standard alpha (not fully opaque)
                let mut resolved = ResolvedColor::from(*color);
                if color.3 < 1.0 {
                    // Preserve the alpha from the original color
                    resolved.alpha_override = Some(color.3);
                }
                resolved
            }
            InheritableColor::Animated {
                color,
                alt_color,
                ease,
                one_shot,
            } => {
                if let Some((mix_value, next)) = ease.borrow_mut().intensity(*one_shot) {
                    self.update_next_frame_time(Some(next));
                    let mut resolved = ResolvedColor {
                        color: *color,
                        alt_color: *alt_color,
                        mix_value,
                        alpha_override: None,
                    };
                    // Check if either color has non-standard alpha
                    if color.3 < 1.0 || alt_color.3 < 1.0 {
                        // For animated colors, we might need to interpolate alpha
                        // For now, just use the primary color's alpha
                        resolved.alpha_override = Some(color.3);
                    }
                    resolved
                } else {
                    let mut resolved = ResolvedColor::from(*color);
                    if color.3 < 1.0 {
                        resolved.alpha_override = Some(color.3);
                    }
                    resolved
                }
            }
        }
    }

    fn resolve_bg(
        &self,
        colors: &ElementColors,
        inherited_colors: Option<&ElementColors>,
    ) -> ResolvedColor {
        match &colors.bg {
            InheritableColor::Inherited => match inherited_colors {
                Some(colors) => self.resolve_bg(colors, None),
                None => LinearRgba::TRANSPARENT.into(),
            },
            InheritableColor::Color(color) => (*color).into(),
            InheritableColor::Animated {
                color,
                alt_color,
                ease,
                one_shot,
            } => {
                if let Some((mix_value, next)) = ease.borrow_mut().intensity(*one_shot) {
                    self.update_next_frame_time(Some(next));
                    ResolvedColor {
                        color: *color,
                        alt_color: *alt_color,
                        mix_value,
                        alpha_override: None,
                    }
                } else {
                    (*color).into()
                }
            }
        }
    }

    fn render_element_background<'a>(
        &self,
        element: &ComputedElement,
        colors: &ElementColors,
        layers: &mut TripleLayerQuadAllocator,
        inherited_colors: Option<&ElementColors>,
    ) -> anyhow::Result<()> {
        let mut top_left_width = 0.;
        let mut top_left_height = 0.;
        let mut top_right_width = 0.;
        let mut top_right_height = 0.;

        let mut bottom_left_width = 0.;
        let mut bottom_left_height = 0.;
        let mut bottom_right_width = 0.;
        let mut bottom_right_height = 0.;

        if let Some(c) = &element.border_corners {
            top_left_width = c.top_left.width;
            top_left_height = c.top_left.height;
            top_right_width = c.top_right.width;
            top_right_height = c.top_right.height;

            bottom_left_width = c.bottom_left.width;
            bottom_left_height = c.bottom_left.height;
            bottom_right_width = c.bottom_right.width;
            bottom_right_height = c.bottom_right.height;

            if top_left_width > 0. && top_left_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    element.border_rect.origin,
                    c.top_left.poly,
                    element.border.top as isize,
                    euclid::size2(top_left_width, top_left_height),
                    colors.border.top,
                )?
                .set_grayscale();
            }
            if top_right_width > 0. && top_right_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    euclid::point2(
                        element.border_rect.max_x() - top_right_width,
                        element.border_rect.min_y(),
                    ),
                    c.top_right.poly,
                    element.border.top as isize,
                    euclid::size2(top_right_width, top_right_height),
                    colors.border.top,
                )?
                .set_grayscale();
            }
            if bottom_left_width > 0. && bottom_left_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    euclid::point2(
                        element.border_rect.min_x(),
                        element.border_rect.max_y() - bottom_left_height,
                    ),
                    c.bottom_left.poly,
                    element.border.bottom as isize,
                    euclid::size2(bottom_left_width, bottom_left_height),
                    colors.border.bottom,
                )?
                .set_grayscale();
            }
            if bottom_right_width > 0. && bottom_right_height > 0. {
                self.poly_quad(
                    layers,
                    0,
                    euclid::point2(
                        element.border_rect.max_x() - bottom_right_width,
                        element.border_rect.max_y() - bottom_right_height,
                    ),
                    c.bottom_right.poly,
                    element.border.bottom as isize,
                    euclid::size2(bottom_right_width, bottom_right_height),
                    colors.border.bottom,
                )?
                .set_grayscale();
            }

            // Filling the background is more complex because we can't
            // simply fill the padding rect--we'd clobber the corner
            // graphics.
            // Instead, we consider the element as consisting of:
            //
            //   TL T TR
            //   L  C  R
            //   BL B BR
            //
            // We already rendered the corner pieces, so now we need
            // to do the rest

            // The `T` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + top_left_width,
                    element.border_rect.min_y(),
                    element.border_rect.width() - (top_left_width + top_right_width) as f32,
                    top_left_height.max(top_right_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `B` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + bottom_left_width,
                    element.border_rect.max_y() - bottom_left_height.max(bottom_right_height),
                    element.border_rect.width() - (bottom_left_width + bottom_right_width),
                    bottom_left_height.max(bottom_right_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `L` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x(),
                    element.border_rect.min_y() + top_left_height,
                    top_left_width.max(bottom_left_width),
                    element.border_rect.height() - (top_left_height + bottom_left_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `R` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.max_x() - top_right_width,
                    element.border_rect.min_y() + top_right_height,
                    top_right_width.max(bottom_right_width),
                    element.border_rect.height() - (top_right_height + bottom_right_height),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);

            // The `C` piece
            let mut quad = self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + top_left_width,
                    element.border_rect.min_y() + top_right_height.min(top_left_height),
                    element.border_rect.width() - (top_left_width + top_right_width),
                    element.border_rect.height()
                        - (top_right_height.min(top_left_height)
                            + bottom_right_height.min(bottom_left_height)),
                ),
                LinearRgba::TRANSPARENT,
            )?;
            self.resolve_bg(colors, inherited_colors).apply(&mut quad);
        } else {
            let resolved_bg = self.resolve_bg(colors, inherited_colors);
            if resolved_bg.color != LinearRgba::TRANSPARENT {
                let mut quad =
                    self.filled_rectangle(layers, 0, element.padding, LinearRgba::TRANSPARENT)?;
                resolved_bg.apply(&mut quad);
            }
        }

        if element.border_rect == element.padding {
            // There's no border to be drawn
            return Ok(());
        }

        if element.border.top > 0. && colors.border.top != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + top_left_width as f32,
                    element.border_rect.min_y(),
                    element.border_rect.width() - (top_left_width + top_right_width) as f32,
                    element.border.top,
                ),
                colors.border.top,
            )?;
        }
        if element.border.bottom > 0. && colors.border.bottom != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x() + bottom_left_width as f32,
                    element.border_rect.max_y() - element.border.bottom,
                    element.border_rect.width() - (bottom_left_width + bottom_right_width) as f32,
                    element.border.bottom,
                ),
                colors.border.bottom,
            )?;
        }
        if element.border.left > 0. && colors.border.left != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.min_x(),
                    element.border_rect.min_y() + top_left_height as f32,
                    element.border.left,
                    element.border_rect.height() - (top_left_height + bottom_left_height) as f32,
                ),
                colors.border.left,
            )?;
        }
        if element.border.right > 0. && colors.border.right != LinearRgba::TRANSPARENT {
            self.filled_rectangle(
                layers,
                0,
                euclid::rect(
                    element.border_rect.max_x() - element.border.right,
                    element.border_rect.min_y() + top_right_height as f32,
                    element.border.left,
                    element.border_rect.height() - (top_right_height + bottom_right_height) as f32,
                ),
                colors.border.right,
            )?;
        }

        Ok(())
    }
}
