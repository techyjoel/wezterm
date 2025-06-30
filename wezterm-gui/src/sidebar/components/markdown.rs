//! Markdown rendering component for sidebar UI
//! Converts markdown text to Elements with proper styling

use crate::color::LinearRgba;
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent,
    Float,
};
use config::Dimension;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use wezterm_font::LoadedFont;
use crate::sidebar::SidebarFonts;

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
        renderer.render_markdown(text, font, None, 1.0, 3.0, None, None)
    }

    /// Render markdown text with a specific code font
    pub fn render_with_code_font(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, font, Some(code_font), 1.0, 3.0, None, None)
    }

    /// Render markdown text with a specific code font and max width
    pub fn render_with_width(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
        max_width: Option<f32>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, font, Some(code_font), 1.0, 3.0, max_width, None)
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
        renderer.render_markdown(text, font, Some(code_font), 1.0, 3.0, max_width, None)
    }

    /// Render markdown text with SidebarFonts (includes bold heading font)
    pub fn render_with_fonts(
        text: &str,
        fonts: &SidebarFonts,
        max_width: Option<f32>,
    ) -> Element {
        let mut renderer = Self::new();
        
        renderer.render_markdown(text, &fonts.body, Some(&fonts.code), fonts.code_line_height, fonts.code_line_margin, max_width, Some(&fonts.heading))
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
        renderer.render_markdown(text, &fonts.body, Some(&fonts.code), fonts.code_line_height, fonts.code_line_margin, max_width, Some(&fonts.heading))
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
                                Element::new(heading_element_font, ElementContent::WrappedText(text))
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
    ) -> Element {
        // Try to find syntax for the language
        let syntax = language
            .and_then(|lang| self.syntax_set.find_syntax_by_token(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        // Use a dark theme suitable for our UI
        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut line_elements = Vec::new();
        let mut lines_for_measurement = Vec::new();

        // Process each line with syntax highlighting
        for line in LinesWithEndings::from(code) {
            lines_for_measurement.push(line);
            let ranges = highlighter.highlight_line(line, &self.syntax_set).unwrap();
            let mut line_parts = Vec::new();

            for (style, text) in ranges {
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
                // For code wrapping, we need to handle syntax-highlighted segments
                // If we have a max_width, wrap the line intelligently
                if let Some(max_w) = max_width {
                    // Account for code block padding and border
                    let available_width = max_w - CODE_BLOCK_CHROME_SIZE;
                    
                    // First, let's reconstruct the full line text
                    let mut full_line = String::new();
                    for part in &line_parts {
                        if let ElementContent::Text(text) = &part.content {
                            full_line.push_str(text);
                        }
                    }
                    
                    // Now create a single WrappedText element for the entire line
                    // This will properly handle word wrapping
                    if !full_line.is_empty() {
                        log::debug!("Code block line (len={}): {:?}", full_line.len(), full_line);
                        // For now, use the first segment's color for the whole line
                        // TODO: Implement a way to preserve syntax colors across wrapped lines
                        let base_colors = if !line_parts.is_empty() {
                            line_parts[0].colors.clone()
                        } else {
                            ElementColors::default()
                        };
                        
                        let wrapped_line = Element::new(font, ElementContent::WrappedText(full_line))
                            .colors(base_colors)
                            .display(DisplayType::Block)
                            .line_height(Some(code_line_height))
                            .margin(BoxDimension {
                                bottom: Dimension::Pixels(code_line_margin as f32), // Visual separation between logical lines
                                ..Default::default()
                            });
                        line_elements.push(wrapped_line);
                    }
                } else {
                    // No max_width, preserve original behavior
                    let inline_parts: Vec<Element> = line_parts
                        .into_iter()
                        .map(|mut part| {
                            part.display = DisplayType::Inline;
                            part
                        })
                        .collect();

                    let combined_element = Element::new(font, ElementContent::Children(inline_parts))
                        .display(DisplayType::Block)
                        .line_height(Some(code_line_height))
                        // Add bottom margin to create visual separation between logical lines
                        .margin(BoxDimension {
                            bottom: Dimension::Pixels(code_line_margin as f32), // Visual separation between logical lines
                            ..Default::default()
                        });
                    line_elements.push(combined_element);
                }
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
