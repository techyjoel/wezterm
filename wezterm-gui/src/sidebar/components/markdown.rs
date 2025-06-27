//! Markdown rendering component for sidebar UI
//! Converts markdown text to Elements with proper styling

use crate::color::LinearRgba;
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent,
};
use config::Dimension;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag};
use std::rc::Rc;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;
use wezterm_font::LoadedFont;

/// Markdown renderer that converts markdown text to Elements
pub struct MarkdownRenderer {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl MarkdownRenderer {
    /// Create a new markdown renderer with syntax highlighting support
    fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }
    /// Render markdown text to an Element tree
    pub fn render(text: &str, font: &Rc<LoadedFont>) -> Element {
        let renderer = Self::new();
        renderer.render_markdown(text, font, None)
    }

    /// Render markdown text with a specific code font
    pub fn render_with_code_font(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
    ) -> Element {
        let renderer = Self::new();
        renderer.render_markdown(text, font, Some(code_font))
    }

    /// Internal render method
    fn render_markdown(
        &self,
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: Option<&Rc<LoadedFont>>,
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

                            // TODO: Implement font size scaling when supported
                            elements.push(
                                Element::new(font, ElementContent::WrappedText(text))
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
                        let highlighted_element = self.highlight_code_block(
                            &code_block_content,
                            code_block_lang.as_deref(),
                            code_render_font,
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
                    // Inline code
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

impl MarkdownRenderer {
    /// Highlight a code block with syntax highlighting
    fn highlight_code_block(
        &self,
        code: &str,
        language: Option<&str>,
        font: &Rc<LoadedFont>,
    ) -> Element {
        // Try to find syntax for the language
        let syntax = language
            .and_then(|lang| self.syntax_set.find_syntax_by_token(lang))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        // Use a dark theme suitable for our UI
        let theme = &self.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut line_elements = Vec::new();

        // Process each line with syntax highlighting
        for line in LinesWithEndings::from(code) {
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
                line_elements.push(
                    Element::new(font, ElementContent::Children(line_parts))
                        .display(DisplayType::Block),
                );
            }
        }

        // If no lines were highlighted, fall back to plain text
        if line_elements.is_empty() {
            // Split code into lines and render each as a separate block element
            for line in code.lines() {
                line_elements.push(
                    Element::new(font, ElementContent::Text(line.to_string()))
                        .colors(ElementColors {
                            text: LinearRgba::with_components(0.85, 0.85, 0.85, 1.0).into(),
                            ..Default::default()
                        })
                        .display(DisplayType::Block),
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

        // Wrap in a code block container
        Element::new(font, ElementContent::Children(line_elements))
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
    }
}
