//! Markdown rendering component for sidebar UI
//! Converts markdown text to Elements with proper styling

use crate::color::LinearRgba;
use crate::sidebar::SidebarFonts;
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementCell, ElementColors, ElementContent,
    Float, StyleSpan,
};
use config::Dimension;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use syntect::easy::HighlightLines;
use syntect::highlighting::{ScopeSelectors, Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use wezterm_font::LoadedFont;
use wezterm_term::color::{ColorPalette, SrgbaTuple};

/// Default padding for code blocks (in pixels)
const CODE_BLOCK_PADDING: f32 = 12.0;

/// Default border width for code blocks (in pixels)
const CODE_BLOCK_BORDER: f32 = 1.0;

/// Calculate the total chrome size for code blocks (padding + border on both sides)
fn calculate_code_block_chrome() -> f32 {
    // Each side has padding and border, so multiply by 2
    (CODE_BLOCK_PADDING + CODE_BLOCK_BORDER) * 2.0
}

/// Container for managing code block state (primarily for copy button)
#[derive(Debug, Clone)]
pub struct CodeBlockContainer {
    pub id: String,
    pub raw_code: String,
    pub language: Option<String>,
    pub copy_success_time: Option<Instant>,
}

impl CodeBlockContainer {
    pub fn new(id: String) -> Self {
        Self {
            id,
            raw_code: String::new(),
            language: None,
            copy_success_time: None,
        }
    }
}

/// Registry for tracking active code block containers
pub type CodeBlockRegistry = Arc<Mutex<HashMap<String, CodeBlockContainer>>>;

/// Markdown renderer that converts markdown text to Elements
pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    code_block_counter: usize,
    /// Optional registry for tracking code block containers
    code_block_registry: Option<Arc<Mutex<HashMap<String, CodeBlockContainer>>>>,
    /// Context prefix for generating unique code block IDs
    context_prefix: String,
}

impl MarkdownRenderer {
    /// Get theme foreground color with optional dimming
    fn get_theme_foreground(palette: Option<&ColorPalette>, dimming_factor: f32) -> LinearRgba {
        if let Some(palette) = palette {
            // Apply dimming in sRGB space for perceptually correct results
            let dimmed = palette.foreground.mul_alpha(dimming_factor);
            dimmed.to_linear()
        } else {
            // Fallback to default gray if no palette provided
            LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
        }
    }

    /// Create a new markdown renderer with syntax highlighting support
    fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            code_block_counter: 0,
            code_block_registry: None,
            context_prefix: String::new(),
        }
    }
    /// Render markdown text to an Element tree
    pub fn render(text: &str, font: &Rc<LoadedFont>) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, None, font, None, 1.0, 3.0, None, None, None)
    }

    /// Render markdown text with a specific code font
    pub fn render_with_code_font(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(
            text,
            None,
            font,
            Some(code_font),
            1.0,
            3.0,
            None,
            None,
            None,
        )
    }

    /// Render markdown text with a specific code font and max width
    pub fn render_with_width(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
        max_width: Option<f32>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(
            text,
            None,
            font,
            Some(code_font),
            1.0,
            3.0,
            max_width,
            None,
            None,
        )
    }

    /// Render markdown text with a code block registry for state management
    pub fn render_with_registry(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
        max_width: Option<f32>,
        registry: Arc<Mutex<HashMap<String, CodeBlockContainer>>>,
        context: &str,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.code_block_registry = Some(registry);
        renderer.context_prefix = context.to_string();
        renderer.render_markdown(
            text,
            None,
            font,
            Some(code_font),
            1.0,
            3.0,
            max_width,
            None,
            None,
        )
    }

    /// Build a paragraph element from collected segments
    fn build_paragraph_element(
        segments: &[(String, &Rc<LoadedFont>, ElementColors)],
        default_font: &Rc<LoadedFont>,
        palette: Option<&ColorPalette>,
        dimming_factor: f32,
    ) -> Element {
        let mut combined_text = String::new();
        let mut style_spans = Vec::new();

        for (text, text_font, colors) in segments {
            let start = combined_text.len();
            combined_text.push_str(text);
            let end = combined_text.len();

            // Add style span if using different font or colors
            if !Rc::ptr_eq(text_font, default_font) || !colors.is_default() {
                style_spans.push(StyleSpan {
                    start,
                    end,
                    colors: colors.clone(),
                    font: Some((*text_font).clone()),
                    font_style: None,
                });
            }
        }

        // Validate style spans
        if let Err(e) = StyleSpan::validate_spans(&style_spans, combined_text.len()) {
            log::error!("Invalid style spans in paragraph: {}", e);
        }

        let spans_count = style_spans.len();
        let text_len = combined_text.len();

        let element = if !style_spans.is_empty() {
            Element::new(
                default_font,
                ElementContent::StyledWrappedText {
                    text: combined_text,
                    style_spans,
                },
            )
        } else {
            Element::new(default_font, ElementContent::WrappedText(combined_text))
        };

        log::debug!(
            "Created paragraph element with {} style spans, text length: {}",
            spans_count,
            text_len
        );

        element
            .colors(ElementColors {
                text: Self::get_theme_foreground(palette, dimming_factor).into(),
                ..Default::default()
            })
            .padding(BoxDimension {
                bottom: Dimension::Pixels(8.0),
                ..Default::default()
            })
            .display(DisplayType::Block)
    }

    /// Render markdown text with SidebarFonts (includes bold heading font)
    pub fn render_with_fonts(text: &str, fonts: &SidebarFonts, max_width: Option<f32>) -> Element {
        let mut renderer = Self::new();

        renderer.render_markdown(
            text,
            Some(fonts),
            &fonts.body,
            Some(&fonts.code),
            fonts.code_line_height,
            fonts.code_line_margin,
            max_width,
            Some(&fonts.heading),
            None,
        )
    }

    /// Render markdown text with SidebarFonts and registry
    pub fn render_with_fonts_and_registry(
        text: &str,
        fonts: &SidebarFonts,
        max_width: Option<f32>,
        registry: Arc<Mutex<HashMap<String, CodeBlockContainer>>>,
        context: &str,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.code_block_registry = Some(registry);
        renderer.context_prefix = context.to_string();
        renderer.render_markdown(
            text,
            Some(fonts),
            &fonts.body,
            Some(&fonts.code),
            fonts.code_line_height,
            fonts.code_line_margin,
            max_width,
            Some(&fonts.heading),
            None,
        )
    }

    /// Render markdown text with SidebarFonts, registry, and color palette
    pub fn render_with_fonts_registry_and_palette(
        text: &str,
        fonts: &SidebarFonts,
        max_width: Option<f32>,
        registry: Arc<Mutex<HashMap<String, CodeBlockContainer>>>,
        context: &str,
        palette: &ColorPalette,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.code_block_registry = Some(registry);
        renderer.context_prefix = context.to_string();
        renderer.render_markdown(
            text,
            Some(fonts),
            &fonts.body,
            Some(&fonts.code),
            fonts.code_line_height,
            fonts.code_line_margin,
            max_width,
            Some(&fonts.heading),
            Some(palette),
        )
    }

    /// Internal render method
    fn render_markdown(
        &mut self,
        text: &str,
        fonts: Option<&SidebarFonts>,
        font: &Rc<LoadedFont>,
        code_font: Option<&Rc<LoadedFont>>,
        code_line_height: f64,
        code_line_margin: f64,
        max_width: Option<f32>,
        heading_font: Option<&Rc<LoadedFont>>,
        palette: Option<&ColorPalette>,
    ) -> Element {
        let parser = Parser::new(text);
        let mut elements = Vec::new();
        let mut current_paragraph: Vec<(String, &Rc<LoadedFont>, ElementColors)> = Vec::new();
        let mut in_code_block = false;
        let mut code_block_lang = None;
        let mut code_block_content = String::new();
        let mut list_depth: usize = 0;
        let mut emphasis_stack: Vec<TextEmphasis> = Vec::new();
        let mut heading_level: Option<HeadingLevel> = None;

        // Add list state tracking
        let mut list_stack: Vec<(bool, u64)> = Vec::new(); // (is_ordered, current_number)
        let mut in_list_item = false;
        let mut pending_list_marker: Option<String> = None;

        for event in parser {
            match event {
                Event::Start(tag) => match tag {
                    Tag::Paragraph => {
                        current_paragraph.clear();
                    }
                    Tag::Heading(level, _, _) => {
                        current_paragraph.clear();
                        heading_level = Some(level);
                    }
                    Tag::CodeBlock(kind) => {
                        in_code_block = true;
                        code_block_lang = match kind {
                            pulldown_cmark::CodeBlockKind::Indented => None,
                            pulldown_cmark::CodeBlockKind::Fenced(lang) => Some(lang.to_string()),
                        };
                        code_block_content.clear();
                    }
                    Tag::List(start_number) => {
                        let is_ordered = start_number.is_some();
                        let start = start_number.unwrap_or(1);
                        list_stack.push((is_ordered, start));
                        list_depth += 1;
                    }
                    Tag::Emphasis => {
                        emphasis_stack.push(TextEmphasis::Italic);
                    }
                    Tag::Strong => {
                        emphasis_stack.push(TextEmphasis::Bold);
                    }
                    Tag::Link(_, dest, _) => {
                        emphasis_stack.push(TextEmphasis::Link(dest.to_string()));
                    }
                    Tag::Item => {
                        in_list_item = true;

                        // Generate marker based on current list state
                        let depth = list_stack.len();
                        if let Some((is_ordered, current_num)) = list_stack.last_mut() {
                            let is_ordered = *is_ordered;
                            let marker = if is_ordered {
                                let m = format!("{}. ", current_num);
                                *current_num += 1;
                                m
                            } else {
                                match (depth - 1) % 3 {
                                    0 => "\u{2022} ", // • BULLET
                                    1 => "\u{25E6} ", // ◦ WHITE BULLET
                                    _ => "\u{25AA} ", // ▪ BLACK SMALL SQUARE
                                }
                                .to_string()
                            };
                            pending_list_marker = Some(marker);
                        }
                    }
                    _ => {}
                },
                Event::End(tag) => match tag {
                    Tag::Paragraph => {
                        if !current_paragraph.is_empty() {
                            let dimming_factor = fonts
                                .map(|f| f.syntax_dimming_factor as f32)
                                .unwrap_or(0.85);
                            let paragraph_element = Self::build_paragraph_element(
                                &current_paragraph,
                                font,
                                palette,
                                dimming_factor,
                            );
                            elements.push(paragraph_element);
                            current_paragraph.clear();
                        }
                    }
                    Tag::Heading(level, _, _) => {
                        if !current_paragraph.is_empty() {
                            // Build heading text from segments
                            let mut combined_text = String::new();
                            for (text, _, _) in &current_paragraph {
                                combined_text.push_str(text);
                            }

                            let dimming_factor = fonts
                                .map(|f| f.syntax_dimming_factor as f32)
                                .unwrap_or(0.85);
                            let (size, color, padding) = match level {
                                HeadingLevel::H1 => {
                                    // Headings are slightly brighter than body text
                                    (
                                        1.5,
                                        Self::get_theme_foreground(
                                            palette,
                                            (dimming_factor + 0.15).min(1.0),
                                        ),
                                        16.0,
                                    )
                                }
                                HeadingLevel::H2 => (
                                    1.3,
                                    Self::get_theme_foreground(
                                        palette,
                                        (dimming_factor + 0.10).min(1.0),
                                    ),
                                    14.0,
                                ),
                                HeadingLevel::H3 => (
                                    1.1,
                                    Self::get_theme_foreground(
                                        palette,
                                        (dimming_factor + 0.06).min(1.0),
                                    ),
                                    12.0,
                                ),
                                _ => (
                                    1.0,
                                    Self::get_theme_foreground(palette, dimming_factor),
                                    10.0,
                                ),
                            };

                            // Use heading font if available, otherwise use regular font
                            let heading_element_font = heading_font.unwrap_or(font);
                            elements.push(
                                Element::new(
                                    heading_element_font,
                                    ElementContent::WrappedText(combined_text),
                                )
                                .colors(ElementColors {
                                    text: color.into(),
                                    ..Default::default()
                                })
                                .padding(BoxDimension {
                                    top: Dimension::Pixels(padding),
                                    bottom: Dimension::Pixels(padding / 2.0),
                                    ..Default::default()
                                })
                                .display(DisplayType::Block),
                            );
                            current_paragraph.clear();
                        }
                        heading_level = None;
                    }
                    Tag::CodeBlock(_) => {
                        in_code_block = false;
                        // Render code block with syntax highlighting
                        // Use code font if provided, otherwise use regular font
                        let code_render_font = code_font.unwrap_or(&font);

                        // Generate unique ID for this code block
                        self.code_block_counter += 1;
                        let block_id = if self.context_prefix.is_empty() {
                            format!("code_block_{}", self.code_block_counter)
                        } else {
                            format!(
                                "{}__code_block_{}",
                                self.context_prefix, self.code_block_counter
                            )
                        };

                        let highlighted_element = self.highlight_code_block(
                            &code_block_content,
                            code_block_lang.as_deref(),
                            code_render_font,
                            code_line_height,
                            code_line_margin,
                            max_width,
                            block_id,
                            palette,
                            fonts,
                        );
                        elements.push(highlighted_element);
                        code_block_content.clear();
                        code_block_lang = None;
                    }
                    Tag::List(_) => {
                        list_stack.pop();
                        list_depth = list_depth.saturating_sub(1);

                        // Add spacing after top-level lists
                        if list_depth == 0 && !current_paragraph.is_empty() {
                            // Use existing paragraph handling
                            let dimming_factor = fonts
                                .map(|f| f.syntax_dimming_factor as f32)
                                .unwrap_or(0.85);
                            let paragraph_element = Self::build_paragraph_element(
                                &current_paragraph,
                                font,
                                palette,
                                dimming_factor,
                            );
                            elements.push(paragraph_element);
                            current_paragraph.clear();
                        }
                    }
                    Tag::Item => {
                        in_list_item = false;

                        // If we have paragraph content, prepend marker and render with indentation
                        if !current_paragraph.is_empty() {
                            if let Some(marker) = pending_list_marker.take() {
                                // Prepend marker to first text span
                                if !current_paragraph.is_empty() {
                                    let colors = current_paragraph[0].2.clone();
                                    current_paragraph.insert(0, (marker, font, colors));
                                }
                            }

                            // Calculate indentation
                            let indent = (list_depth.saturating_sub(1)) as f32 * 20.0;

                            // Build element with existing method but add indentation
                            let dimming_factor = fonts
                                .map(|f| f.syntax_dimming_factor as f32)
                                .unwrap_or(0.85);
                            let mut list_item = Self::build_paragraph_element(
                                &current_paragraph,
                                font,
                                palette,
                                dimming_factor,
                            );

                            // Add left padding for indentation
                            list_item = list_item.padding(BoxDimension {
                                left: Dimension::Pixels(indent),
                                bottom: Dimension::Pixels(4.0), // Tighter spacing for list items
                                ..Default::default()
                            });

                            elements.push(list_item);
                            current_paragraph.clear();
                        }
                        pending_list_marker = None;
                    }
                    Tag::Emphasis | Tag::Strong | Tag::Link(_, _, _) => {
                        emphasis_stack.pop();
                    }
                    _ => {}
                },
                Event::Text(text) => {
                    if in_code_block {
                        code_block_content.push_str(&text);
                    } else {
                        // Determine if we have bold and/or italic emphasis
                        let has_bold = emphasis_stack
                            .iter()
                            .any(|e| matches!(e, TextEmphasis::Bold));
                        let has_italic = emphasis_stack
                            .iter()
                            .any(|e| matches!(e, TextEmphasis::Italic));

                        // Select the appropriate font based on emphasis
                        let text_font = if let Some(fonts) = fonts {
                            fonts.get_body_font_for_emphasis(has_bold, has_italic)
                        } else {
                            if has_bold || has_italic {
                                log::warn!(
                                    "Font emphasis requested (bold={}, italic={}) but SidebarFonts not provided",
                                    has_bold, has_italic
                                );
                            }
                            font
                        };

                        // Store text with its font
                        // Use explicit text color to avoid transparent text
                        let dimming_factor = fonts
                            .map(|f| f.syntax_dimming_factor as f32)
                            .unwrap_or(0.85);
                        let text_colors = ElementColors {
                            text: Self::get_theme_foreground(palette, dimming_factor).into(),
                            ..Default::default()
                        };
                        current_paragraph.push((text.to_string(), text_font, text_colors));
                    }
                }
                Event::Code(code) => {
                    // Inline code - use code font with special colors
                    let code_colors = ElementColors {
                        text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                        bg: LinearRgba::with_components(0.15, 0.15, 0.15, 1.0).into(),
                        ..Default::default()
                    };

                    current_paragraph.push((
                        code.to_string(),
                        code_font.unwrap_or(font),
                        code_colors,
                    ));
                }
                Event::SoftBreak => {
                    current_paragraph.push((" ".to_string(), font, ElementColors::default()));
                }
                Event::HardBreak => {
                    current_paragraph.push(("\n".to_string(), font, ElementColors::default()));
                }
                _ => {}
            }
        }

        // Handle any remaining paragraph content
        if !current_paragraph.is_empty() {
            let dimming_factor = fonts
                .map(|f| f.syntax_dimming_factor as f32)
                .unwrap_or(0.85);
            let paragraph_element =
                Self::build_paragraph_element(&current_paragraph, font, palette, dimming_factor);
            elements.push(paragraph_element);
        }

        // Wrap all elements in a container
        Element::new(font, ElementContent::Children(elements)).display(DisplayType::Block)
    }
}

#[derive(Debug, Clone)]
enum TextEmphasis {
    Bold,
    Italic,
    Link(String),
}

/// Measure the maximum width of code lines
fn measure_code_block_width(lines: &[&str], font: &Rc<LoadedFont>) -> f32 {
    use termwiz::cell::unicode_column_width;

    lines
        .iter()
        .map(|line| {
            let width = unicode_column_width(line, None) as f32;
            width * font.metrics().cell_width.get() as f32
        })
        .fold(0.0_f32, |max, width| {
            if width.is_finite() && width > max {
                width
            } else {
                max
            }
        })
}

impl MarkdownRenderer {
    /// Create a syntect theme from WezTerm palette
    fn create_syntect_theme_from_palette(
        palette: &ColorPalette,
        dimming_factor: f32,
    ) -> syntect::highlighting::Theme {
        use std::str::FromStr;
        use syntect::highlighting::{Color as SyntectColor, StyleModifier, Theme, ThemeSettings};

        let to_syntect_color = |color: SrgbaTuple| -> SyntectColor {
            let (r, g, b, _) = color.to_srgb_u8();
            SyntectColor { r, g, b, a: 255 }
        };

        // Apply dimming to a color
        let dim_color = |color: SrgbaTuple| -> SyntectColor {
            let (r, g, b, _) = color.to_srgb_u8();
            SyntectColor {
                r: (r as f32 * dimming_factor) as u8,
                g: (g as f32 * dimming_factor) as u8,
                b: (b as f32 * dimming_factor) as u8,
                a: 255,
            }
        };

        let mut theme = Theme {
            name: Some("WezTerm Dynamic".to_string()),
            author: None,
            settings: ThemeSettings {
                foreground: Some(dim_color(palette.foreground)),
                background: Some(to_syntect_color(palette.background)),
                caret: None,
                line_highlight: None,
                misspelling: None,
                minimap_border: None,
                accent: None,
                popup_css: None,
                phantom_css: None,
                bracket_contents_foreground: None,
                bracket_contents_options: None,
                brackets_foreground: None,
                brackets_background: None,
                brackets_options: None,
                tags_foreground: None,
                tags_options: None,
                highlight: None,
                find_highlight: None,
                find_highlight_foreground: None,
                gutter: None,
                gutter_foreground: None,
                selection: None,
                selection_foreground: None,
                selection_border: None,
                inactive_selection: None,
                inactive_selection_foreground: None,
                guide: None,
                active_guide: None,
                stack_guide: None,
                shadow: None,
            },
            scopes: Vec::new(),
        };

        // Helper to dim a color by a custom factor
        let dim_custom = |color: SrgbaTuple, factor: f32| -> SyntectColor {
            let (r, g, b, _) = color.to_srgb_u8();
            SyntectColor {
                r: (r as f32 * factor) as u8,
                g: (g as f32 * factor) as u8,
                b: (b as f32 * factor) as u8,
                a: 255,
            }
        };

        // Add scope rules mapping to theme colors with dimming
        let scope_rules = vec![
            // Comments (gray - dimmed foreground for less distraction)
            (
                vec!["comment", "comment.line", "comment.block"],
                dim_custom(palette.foreground, 0.6),
            ),
            // Documentation comments (slightly brighter than regular comments)
            (
                vec!["comment.block.documentation"],
                dim_custom(palette.foreground, 0.7),
            ),
            // Keywords and control flow (blue)
            (
                vec!["keyword", "keyword.control", "storage"],
                dim_color(palette.colors.0[4]),
            ),
            // Strings and characters (green)
            (
                vec!["string", "string.quoted", "string.regexp"],
                dim_color(palette.colors.0[2]),
            ),
            // String escapes (brighter green to stand out)
            (
                vec!["string.escape", "constant.character.escape"],
                to_syntect_color(palette.colors.0[2]),
            ),
            // Functions and methods (yellow)
            (
                vec![
                    "entity.name.function",
                    "support.function",
                    "variable.function",
                ],
                dim_color(palette.colors.0[3]),
            ),
            // Bash commands (also yellow for consistency)
            (
                vec!["variable.function.shell"],
                dim_color(palette.colors.0[3]),
            ),
            // Classes and types (cyan)
            (
                vec![
                    "entity.name.class",
                    "entity.name.type",
                    "storage.type",
                    "support.class",
                    "support.type",
                ],
                dim_color(palette.colors.0[6]),
            ),
            // Bash built-ins (cyan to show they're "built-in" like types)
            (
                vec!["support.function.shell"],
                dim_color(palette.colors.0[6]),
            ),
            // Constants, numbers, booleans (magenta)
            (
                vec![
                    "constant.numeric",
                    "constant.language",
                    "constant.boolean",
                    "constant.character",
                    "support.constant",
                ],
                dim_color(palette.colors.0[5]),
            ),
            // Variables and parameters (subtle red)
            (
                vec!["variable", "variable.parameter", "variable.other"],
                dim_custom(palette.colors.0[1], 0.7),
            ),
            // Special variables like 'self', 'this' (normal dimmed red)
            (vec!["variable.language"], dim_color(palette.colors.0[1])),
            // Operators (slightly dimmed foreground)
            (
                vec!["keyword.operator"],
                dim_custom(palette.foreground, 0.8),
            ),
            // Tags and attributes (for HTML/XML)
            (vec!["entity.name.tag"], dim_color(palette.colors.0[4])), // Blue like keywords
            (
                vec!["entity.other.attribute-name"],
                dim_color(palette.colors.0[6]),
            ), // Cyan like types
            // Punctuation (very subtle foreground)
            (vec!["punctuation"], dim_custom(palette.foreground, 0.65)),
            // Invalid/illegal code (bright red to draw attention)
            (vec!["invalid"], to_syntect_color(palette.colors.0[1])),
            // Meta scopes (slightly dimmed foreground)
            (vec!["meta"], dim_custom(palette.foreground, 0.9)),
            // Default foreground
            (vec!["source"], dim_color(palette.foreground)),
        ];

        for (scopes, color) in scope_rules {
            for scope in scopes {
                theme.scopes.push(syntect::highlighting::ThemeItem {
                    scope: ScopeSelectors::from_str(scope).unwrap(),
                    style: syntect::highlighting::StyleModifier {
                        foreground: Some(color),
                        background: None,
                        font_style: None,
                    },
                });
            }
        }

        theme
    }

    /// Highlight a code block with syntax highlighting
    fn highlight_code_block(
        &self,
        code: &str,
        language: Option<&str>,
        font: &Rc<LoadedFont>,
        code_line_height: f64,
        code_line_margin: f64,
        max_width: Option<f32>,
        block_id: String,
        palette: Option<&ColorPalette>,
        fonts: Option<&SidebarFonts>,
    ) -> Element {
        // Try to find syntax for the language
        let syntax = language
            .and_then(|lang| self.syntax_set.find_syntax_by_token(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        // Use WezTerm palette theme if available, otherwise use default theme
        let default_theme = &self.theme_set.themes["base16-ocean.dark"];
        let dynamic_theme;
        let theme = if let Some(palette) = palette {
            let dimming_factor = fonts
                .map(|f| f.syntax_dimming_factor as f32)
                .unwrap_or(0.85);
            dynamic_theme = Self::create_syntect_theme_from_palette(palette, dimming_factor);
            &dynamic_theme
        } else {
            default_theme
        };
        let mut highlighter = HighlightLines::new(syntax, theme);

        // Debug logging for language detection
        log::debug!(
            "Code block language: {:?}, using syntax: {}",
            language,
            syntax.name
        );

        // Calculate available width for code content
        // Note: max_width is the sidebar width, we need to account for:
        // - Code block padding and border on each side
        // - Sidebar margins/padding
        let code_block_chrome = calculate_code_block_chrome();
        let available_width = max_width.map(|w| {
            let adjusted = w - code_block_chrome;
            log::debug!(
                "Width calc: max_width={}, chrome={}, available={}",
                w,
                code_block_chrome,
                adjusted
            );
            adjusted
        });

        let mut line_elements = Vec::new();
        let mut lines_for_measurement = Vec::new();

        // Process each line with syntax highlighting
        for line in LinesWithEndings::from(code) {
            lines_for_measurement.push(line);
            let ranges = highlighter.highlight_line(line, &self.syntax_set).unwrap();

            // Debug logging to understand syntax highlighting
            log::debug!(
                "Syntax highlighting for line '{}': {:?}",
                line.trim_end(),
                ranges
                    .iter()
                    .map(|(style, text)| { (text, style.foreground, style.font_style) })
                    .collect::<Vec<_>>()
            );

            let mut line_parts = Vec::new();

            for (style, text) in &ranges {
                let color = LinearRgba::with_components(
                    style.foreground.r as f32 / 255.0,
                    style.foreground.g as f32 / 255.0,
                    style.foreground.b as f32 / 255.0,
                    style.foreground.a as f32 / 255.0,
                );

                line_parts.push(
                    Element::new(font, ElementContent::Text(text.to_string())).colors(
                        ElementColors {
                            text: color.into(),
                            ..Default::default()
                        },
                    ),
                );
            }

            if !line_parts.is_empty() {
                // Build style spans from syntax highlighting
                let mut style_spans = Vec::new();
                let mut combined_text = String::new();
                let mut byte_offset = 0;

                for (idx, (style, text)) in ranges.iter().enumerate() {
                    let start = byte_offset;
                    let end = byte_offset + text.len();

                    let color = LinearRgba::with_components(
                        style.foreground.r as f32 / 255.0,
                        style.foreground.g as f32 / 255.0,
                        style.foreground.b as f32 / 255.0,
                        style.foreground.a as f32 / 255.0,
                    );

                    style_spans.push(StyleSpan {
                        start,
                        end,
                        colors: ElementColors {
                            text: color.into(),
                            ..Default::default()
                        },
                        font: None,
                        font_style: None, // No font variants in code blocks
                    });

                    combined_text.push_str(text);
                    byte_offset = end;
                }

                // Use StyledWrappedText for syntax highlighting with wrapping
                // Set max_width to match the available width for proper wrapping
                let mut wrapped_line = Element::new(
                    font,
                    ElementContent::StyledWrappedText {
                        text: combined_text,
                        style_spans,
                    },
                )
                .display(DisplayType::Block)
                .line_height(Some(code_line_height))
                .margin(BoxDimension {
                    bottom: Dimension::Pixels(code_line_margin as f32),
                    ..Default::default()
                })
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                    ..Default::default()
                });

                // Set max_width if available to ensure proper text wrapping
                if let Some(width) = available_width {
                    wrapped_line = wrapped_line.max_width(Some(Dimension::Pixels(width)));
                }

                line_elements.push(wrapped_line);
            }
        }

        // If no lines were highlighted, fall back to plain text
        if line_elements.is_empty() {
            // Split code into lines and render each as a separate block element
            for line in code.lines() {
                lines_for_measurement.push(line);
                // Use WrappedText for plain code to handle long lines
                let mut plain_line =
                    Element::new(font, ElementContent::WrappedText(line.to_string()))
                        .colors(ElementColors {
                            text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                            ..Default::default()
                        })
                        .display(DisplayType::Block)
                        .line_height(Some(code_line_height))
                        // Add bottom margin to create visual separation between logical lines
                        .margin(BoxDimension {
                            bottom: Dimension::Pixels(code_line_margin as f32), // Visual separation between logical lines
                            ..Default::default()
                        });

                // Set max_width if available to ensure proper text wrapping
                if let Some(width) = available_width {
                    plain_line = plain_line.max_width(Some(Dimension::Pixels(width)));
                }

                line_elements.push(plain_line);
            }
            // Handle case where code is empty or has no lines
            if line_elements.is_empty() {
                line_elements.push(
                    Element::new(font, ElementContent::Text(String::new()))
                        .display(DisplayType::Block),
                );
            }
        }

        // Measure the maximum line width
        let content_width = measure_code_block_width(&lines_for_measurement, font);

        // Get the actual available width for code content
        // Note: max_width is the sidebar width, we need to account for:
        // - Code block padding and border on each side
        // - Sidebar margins/padding
        let code_block_chrome = calculate_code_block_chrome();
        let available_width = max_width.map(|w| {
            let adjusted = w - code_block_chrome;
            log::debug!(
                "Width calc: max_width={}, chrome={}, available={}",
                w,
                code_block_chrome,
                adjusted
            );
            adjusted
        });
        let viewport_width = available_width.unwrap_or(content_width);

        // Debug the context
        log::debug!("Markdown context: block_id={}, language={:?}, max_width={:?}, available_width={:?}, content_width={}, viewport_width={}", 
            block_id, language, max_width, available_width, content_width, viewport_width);

        // Track copy button state in registry
        let mut copy_success_time = None;
        if let Some(ref registry) = self.code_block_registry {
            if let Ok(mut reg) = registry.lock() {
                // Create a simple container just for tracking copy button state
                let mut container = CodeBlockContainer::new(block_id.clone());
                container.raw_code = code.to_string();
                container.language = language.map(|s| s.to_string());

                // Preserve copy success time if it exists
                if let Some(existing) = reg.get(&block_id) {
                    container.copy_success_time = existing.copy_success_time;
                    copy_success_time = existing.copy_success_time;
                }
                reg.insert(block_id.clone(), container);
            }
        }

        let line_count = line_elements.len();
        log::debug!(
            "Code block {}: wrapped into {} lines, max_width={:?}",
            block_id,
            line_count,
            max_width
        );

        // Simply wrap the line elements in the code block container
        // No horizontal scrolling needed since we're wrapping
        let computed_height = line_count as f32 * code_line_height as f32;
        let mut code_block = Element::new(font, ElementContent::Children(line_elements))
            .with_computed_height(computed_height)
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                border: BorderColor::new(LinearRgba::with_components(0.2, 0.2, 0.25, 0.5)),
                ..Default::default()
            })
            .padding(BoxDimension::new(Dimension::Pixels(CODE_BLOCK_PADDING)))
            .border(BoxDimension::new(Dimension::Pixels(CODE_BLOCK_BORDER)))
            .margin(BoxDimension {
                top: Dimension::Pixels(8.0),
                bottom: Dimension::Pixels(8.0),
                ..Default::default()
            })
            .display(DisplayType::Block)
            .item_type(crate::termwindow::UIItemType::CodeBlockContent(
                block_id.clone(),
            ));

        // Add a copy button above the code block (always visible)
        // Check if we should show success state
        let show_success = copy_success_time
            .map(|time| time.elapsed().as_secs_f32() < 2.0)
            .unwrap_or(false);

        let button_text = if show_success {
            "✅ Copied!".to_string()
        } else {
            "📋 Copy".to_string()
        };

        let copy_button = Element::new(font, ElementContent::Text(button_text))
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.2, 0.2, 0.25, 0.9).into(),
                text: Self::get_theme_foreground(
                    palette,
                    fonts
                        .map(|f| f.syntax_dimming_factor as f32)
                        .unwrap_or(0.85),
                )
                .into(),
                border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 0.8)),
                ..Default::default()
            })
            .hover_colors(Some(ElementColors {
                bg: LinearRgba::with_components(0.25, 0.25, 0.3, 0.95).into(),
                text: Self::get_theme_foreground(palette, 1.0).into(),
                border: BorderColor::new(LinearRgba::with_components(0.4, 0.4, 0.45, 0.9)),
                ..Default::default()
            }))
            .padding(BoxDimension {
                left: Dimension::Pixels(8.0),
                right: Dimension::Pixels(8.0),
                top: Dimension::Pixels(4.0),
                bottom: Dimension::Pixels(4.0),
            })
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .float(Float::Right)
            .display(DisplayType::Block)
            .item_type(crate::termwindow::UIItemType::CodeBlockCopyButton(
                block_id.clone(),
            ));

        // Create a wrapper that includes both the copy button and the code block
        Element::new(
            font,
            ElementContent::Children(vec![copy_button, code_block]),
        )
        .display(DisplayType::Block)
    }
}
