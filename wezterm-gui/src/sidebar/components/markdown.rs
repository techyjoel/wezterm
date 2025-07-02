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

/// Total chrome size for code blocks (padding + border on both sides)
/// 12px padding + 1px border on each side = 26px total
const CODE_BLOCK_CHROME_SIZE: f32 = 26.0;

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
        renderer.render_markdown(text, font, None, 1.0, 3.0, None, None, None)
    }

    /// Render markdown text with a specific code font
    pub fn render_with_code_font(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, font, Some(code_font), 1.0, 3.0, None, None, None)
    }

    /// Render markdown text with a specific code font and max width
    pub fn render_with_width(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
        max_width: Option<f32>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, font, Some(code_font), 1.0, 3.0, max_width, None, None)
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
        renderer.render_markdown(text, font, Some(code_font), 1.0, 3.0, max_width, None, None)
    }

    /// Render markdown text with SidebarFonts (includes bold heading font)
    pub fn render_with_fonts(text: &str, fonts: &SidebarFonts, max_width: Option<f32>) -> Element {
        let mut renderer = Self::new();

        renderer.render_markdown(
            text,
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
        let mut current_paragraph = Vec::new();
        let mut in_code_block = false;
        let mut code_block_lang = None;
        let mut code_block_content = String::new();
        let mut list_depth: usize = 0;
        let mut emphasis_stack: Vec<TextEmphasis> = Vec::new();
        let mut heading_level: Option<HeadingLevel> = None;

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
                    Tag::List(_) => {
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
                    _ => {}
                },
                Event::End(tag) => match tag {
                    Tag::Paragraph => {
                        if !current_paragraph.is_empty() {
                            let text = current_paragraph.join("");
                            elements.push(
                                Element::new(font, ElementContent::WrappedText(text))
                                    .colors(ElementColors {
                                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0)
                                            .into(),
                                        ..Default::default()
                                    })
                                    .padding(BoxDimension {
                                        bottom: Dimension::Pixels(8.0),
                                        ..Default::default()
                                    })
                                    .display(DisplayType::Block),
                            );
                            current_paragraph.clear();
                        }
                    }
                    Tag::Heading(level, _, _) => {
                        if !current_paragraph.is_empty() {
                            let text = current_paragraph.join("");
                            let (size, color, padding) = match level {
                                HeadingLevel::H1 => (
                                    1.5,
                                    LinearRgba::with_components(0.95, 0.95, 0.95, 1.0),
                                    16.0,
                                ),
                                HeadingLevel::H2 => (
                                    1.3,
                                    LinearRgba::with_components(0.93, 0.93, 0.93, 1.0),
                                    14.0,
                                ),
                                HeadingLevel::H3 => (
                                    1.1,
                                    LinearRgba::with_components(0.91, 0.91, 0.91, 1.0),
                                    12.0,
                                ),
                                _ => (1.0, LinearRgba::with_components(0.9, 0.9, 0.9, 1.0), 10.0),
                            };

                            // Use heading font if available, otherwise use regular font
                            let heading_element_font = heading_font.unwrap_or(font);
                            elements.push(
                                Element::new(
                                    heading_element_font,
                                    ElementContent::WrappedText(text),
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
                        );
                        elements.push(highlighted_element);
                        code_block_content.clear();
                        code_block_lang = None;
                    }
                    Tag::List(_) => {
                        list_depth = list_depth.saturating_sub(1);
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
                        // Apply emphasis styles
                        let styled_text = if emphasis_stack
                            .iter()
                            .any(|e| matches!(e, TextEmphasis::Bold))
                        {
                            // TODO: Apply bold styling when font variants are supported
                            text.to_string()
                        } else if emphasis_stack
                            .iter()
                            .any(|e| matches!(e, TextEmphasis::Italic))
                        {
                            // TODO: Apply italic styling when font variants are supported
                            text.to_string()
                        } else {
                            text.to_string()
                        };
                        current_paragraph.push(styled_text);
                    }
                }
                Event::Code(code) => {
                    // Inline code - for now just add as formatted text
                    // TODO: Implement proper inline code with code font
                    current_paragraph.push(format!("`{}`", code));
                }
                Event::SoftBreak => {
                    current_paragraph.push(" ".to_string());
                }
                Event::HardBreak => {
                    current_paragraph.push("\n".to_string());
                }
                _ => {}
            }
        }

        // Handle any remaining paragraph content
        if !current_paragraph.is_empty() {
            let text = current_paragraph.join("");
            elements.push(
                Element::new(font, ElementContent::WrappedText(text))
                    .colors(ElementColors {
                        text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block),
            );
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
    fn create_syntect_theme_from_palette(palette: &ColorPalette) -> syntect::highlighting::Theme {
        use std::str::FromStr;
        use syntect::highlighting::{Color as SyntectColor, StyleModifier, Theme, ThemeSettings};

        let to_syntect_color = |color: SrgbaTuple| -> SyntectColor {
            let (r, g, b, _) = color.to_srgb_u8();
            SyntectColor { r, g, b, a: 255 }
        };

        // Apply 0.85 dimming to a color
        // TODO: Make dimming factor configurable via clibuddy.right_sidebar config
        let dim_color = |color: SrgbaTuple| -> SyntectColor {
            let (r, g, b, _) = color.to_srgb_u8();
            SyntectColor {
                r: (r as f32 * 0.85) as u8,
                g: (g as f32 * 0.85) as u8,
                b: (b as f32 * 0.85) as u8,
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
    ) -> Element {
        // Try to find syntax for the language
        let syntax = language
            .and_then(|lang| self.syntax_set.find_syntax_by_token(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        // Use WezTerm palette theme if available, otherwise use default theme
        let default_theme = &self.theme_set.themes["base16-ocean.dark"];
        let dynamic_theme;
        let theme = if let Some(palette) = palette {
            dynamic_theme = Self::create_syntect_theme_from_palette(palette);
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
                    .map(|(style, text)| (text, style.foreground, style.font_style))
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

                    // Extract font style flags from syntect
                    let font_style_flags = if style.font_style.is_empty() {
                        None
                    } else {
                        Some(crate::termwindow::box_model::FontStyleFlags {
                            bold: style.font_style.contains(syntect::highlighting::FontStyle::BOLD),
                            italic: style.font_style.contains(syntect::highlighting::FontStyle::ITALIC),
                        })
                    };

                    style_spans.push(StyleSpan {
                        start,
                        end,
                        colors: ElementColors {
                            text: color.into(),
                            ..Default::default()
                        },
                        font: None, // Will be resolved during rendering based on font_style
                        font_style: font_style_flags,
                    });

                    combined_text.push_str(text);
                    byte_offset = end;
                }

                // Validate style spans before using them
                if let Err(e) = StyleSpan::validate_spans(&style_spans, combined_text.len()) {
                    log::error!("Invalid style spans in code block: {}", e);
                    // Fall back to plain text
                    line_elements.push(
                        Element::new(font, ElementContent::WrappedText(combined_text))
                            .colors(ElementColors {
                                text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                                ..Default::default()
                            })
                            .display(DisplayType::Block)
                            .line_height(Some(code_line_height))
                            .margin(BoxDimension {
                                bottom: Dimension::Pixels(code_line_margin as f32),
                                ..Default::default()
                            }),
                    );
                    continue;
                }
                
                // Use StyledWrappedText for syntax highlighting with wrapping
                let wrapped_line = Element::new(
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

                line_elements.push(wrapped_line);
            }
        }

        // If no lines were highlighted, fall back to plain text
        if line_elements.is_empty() {
            // Split code into lines and render each as a separate block element
            for line in code.lines() {
                lines_for_measurement.push(line);
                // Use WrappedText for plain code to handle long lines
                line_elements.push(
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
                        }),
                );
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
        // - Code block padding: 12px each side = 24px
        // - Code block border: 1px each side = 2px
        // - Sidebar margins/padding
        let code_block_chrome = 26.0; // padding + border
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

        log::debug!(
            "Code block {}: wrapped into {} lines, max_width={:?}",
            block_id,
            line_elements.len(),
            max_width
        );

        // Simply wrap the line elements in the code block container
        // No horizontal scrolling needed since we're wrapping
        let mut code_block = Element::new(font, ElementContent::Children(line_elements))
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                border: BorderColor::new(LinearRgba::with_components(0.2, 0.2, 0.25, 0.5)),
                ..Default::default()
            })
            .padding(BoxDimension::new(Dimension::Pixels(12.0)))
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
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
                text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 0.8)),
                ..Default::default()
            })
            .hover_colors(Some(ElementColors {
                bg: LinearRgba::with_components(0.25, 0.25, 0.3, 0.95).into(),
                text: LinearRgba::with_components(1.0, 1.0, 1.0, 1.0).into(),
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
