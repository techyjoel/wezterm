//! Markdown rendering component for sidebar UI
//! Converts markdown text to Elements with proper styling

use crate::color::LinearRgba;
use crate::sidebar::components::horizontal_scroll::{
    create_horizontal_scroll_container, HorizontalScrollConfig,
};
use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent, Float,
};
use crate::termwindow::render::scrollbar_renderer::ScrollbarRenderer;
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

/// Container for managing horizontal scrolling in code blocks
#[derive(Debug, Clone)]
pub struct CodeBlockContainer {
    pub id: String,
    pub content_width: f32,
    pub viewport_width: f32,
    pub scroll_offset: f32,
    pub hovering_scrollbar: bool,
    pub hovering_content: bool,
    pub dragging_scrollbar: bool,
    pub drag_start_x: Option<f32>,
    pub drag_start_offset: Option<f32>,
    pub has_focus: bool,
    pub scrollbar_opacity: f32,
    pub last_activity: Option<Instant>,
    pub raw_code: String,
    pub language: Option<String>,
    pub copy_success_time: Option<Instant>,
}

impl CodeBlockContainer {
    pub fn new(id: String, viewport_width: f32) -> Self {
        Self {
            id,
            content_width: 0.0,
            viewport_width,
            scroll_offset: 0.0,
            hovering_scrollbar: false,
            hovering_content: false,
            dragging_scrollbar: false,
            drag_start_x: None,
            drag_start_offset: None,
            has_focus: false,
            scrollbar_opacity: 0.0,
            last_activity: None,
            raw_code: String::new(),
            language: None,
            copy_success_time: None,
        }
    }

    /// Update content width and return whether scrollbar is needed
    pub fn update_content_width(&mut self, width: f32) -> bool {
        self.content_width = width;
        self.needs_scrollbar()
    }

    pub fn max_scroll(&self) -> f32 {
        (self.content_width - self.viewport_width).max(0.0)
    }

    pub fn scroll_horizontal(&mut self, delta: f32) {
        let new_offset = self.scroll_offset - delta;
        self.set_scroll_offset(new_offset);
    }

    /// Set scroll offset with bounds checking
    pub fn set_scroll_offset(&mut self, offset: f32) {
        self.scroll_offset = offset.clamp(0.0, self.max_scroll());
        self.last_activity = Some(Instant::now());
    }

    pub fn needs_scrollbar(&self) -> bool {
        self.content_width > self.viewport_width
    }

    /// Update hover state and return whether state changed
    pub fn update_hover_state(&mut self, hovering_content: bool, hovering_scrollbar: bool) -> bool {
        let old_hovering = self.hovering_content || self.hovering_scrollbar;
        self.hovering_content = hovering_content;
        self.hovering_scrollbar = hovering_scrollbar;
        let new_hovering = self.hovering_content || self.hovering_scrollbar;

        if new_hovering {
            self.last_activity = Some(Instant::now());
        }

        old_hovering != new_hovering
    }

    /// Update scrollbar opacity based on hover and activity
    pub fn update_opacity(&mut self, delta_time: f32) {
        const FADE_IN_TIME: f32 = 0.15;
        const FADE_OUT_TIME: f32 = 0.3;
        const HIDE_DELAY: f32 = 1.5;

        let is_active = self.hovering_content
            || self.hovering_scrollbar
            || self.dragging_scrollbar
            || self.has_focus;

        if is_active {
            // Fade in
            self.scrollbar_opacity = (self.scrollbar_opacity + delta_time / FADE_IN_TIME).min(1.0);
            self.last_activity = Some(Instant::now());
        } else if let Some(last) = self.last_activity {
            let elapsed = last.elapsed().as_secs_f32();
            if elapsed > HIDE_DELAY {
                // Fade out
                self.scrollbar_opacity =
                    (self.scrollbar_opacity - delta_time / FADE_OUT_TIME).max(0.0);
            }
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
}

impl MarkdownRenderer {
    /// Create a new markdown renderer with syntax highlighting support
    fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            code_block_counter: 0,
            code_block_registry: None,
        }
    }
    /// Render markdown text to an Element tree
    pub fn render(text: &str, font: &Rc<LoadedFont>) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, font, None, None)
    }

    /// Render markdown text with a specific code font
    pub fn render_with_code_font(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, font, Some(code_font), None)
    }

    /// Render markdown text with a specific code font and max width
    pub fn render_with_width(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
        max_width: Option<f32>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.render_markdown(text, font, Some(code_font), max_width)
    }

    /// Render markdown text with a code block registry for state management
    pub fn render_with_registry(
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: &Rc<LoadedFont>,
        max_width: Option<f32>,
        registry: Arc<Mutex<HashMap<String, CodeBlockContainer>>>,
    ) -> Element {
        let mut renderer = Self::new();
        renderer.code_block_registry = Some(registry);
        renderer.render_markdown(text, font, Some(code_font), max_width)
    }

    /// Internal render method
    fn render_markdown(
        &mut self,
        text: &str,
        font: &Rc<LoadedFont>,
        code_font: Option<&Rc<LoadedFont>>,
        max_width: Option<f32>,
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

                        // Generate unique ID for this code block
                        self.code_block_counter += 1;
                        let block_id = format!("code_block_{}", self.code_block_counter);

                        let highlighted_element = self.highlight_code_block(
                            &code_block_content,
                            code_block_lang.as_deref(),
                            code_render_font,
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
                // Ensure all parts display inline so they flow together on wrapping
                let inline_parts: Vec<Element> = line_parts
                    .into_iter()
                    .map(|mut part| {
                        part.display = DisplayType::Inline;
                        part
                    })
                    .collect();

                // Create a block container for the line that allows its inline children to wrap
                let combined_element = Element::new(font, ElementContent::Children(inline_parts))
                    .display(DisplayType::Block);
                line_elements.push(combined_element);
            }
        }

        // If no lines were highlighted, fall back to plain text
        if line_elements.is_empty() {
            // Split code into lines and render each as a separate block element
            for line in code.lines() {
                lines_for_measurement.push(line);
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

        // Measure the maximum line width
        let content_width = measure_code_block_width(&lines_for_measurement, font);
        
        // Account for code block padding (12px on each side)
        let code_block_padding = 24.0;
        let available_width = max_width.map(|w| w - code_block_padding);
        let viewport_width = available_width.unwrap_or(content_width);

        // Create or update container for tracking scroll state
        let mut container = CodeBlockContainer::new(block_id.clone(), viewport_width);
        let needs_scrollbar = container.update_content_width(content_width);
        container.raw_code = code.to_string();
        container.language = language.map(|s| s.to_string());
        
        // TEMPORARY: Force scrollbar visibility for debugging
        if needs_scrollbar {
            container.scrollbar_opacity = 1.0;
        }
        
        log::debug!(
            "Code block {}: content_width={}, viewport_width={}, max_width={:?}, needs_scrollbar={}, opacity={}",
            block_id,
            content_width,
            viewport_width,
            max_width,
            needs_scrollbar,
            container.scrollbar_opacity
        );

        // Update registry if available
        if let Some(ref registry) = self.code_block_registry {
            if let Ok(mut reg) = registry.lock() {
                // Preserve existing scroll state if container exists
                if let Some(existing) = reg.get(&block_id) {
                    container.scroll_offset = existing.scroll_offset;
                    container.hovering_content = existing.hovering_content;
                    container.hovering_scrollbar = existing.hovering_scrollbar;
                    container.dragging_scrollbar = existing.dragging_scrollbar;
                    container.drag_start_x = existing.drag_start_x;
                    container.drag_start_offset = existing.drag_start_offset;
                    container.has_focus = existing.has_focus;
                    container.scrollbar_opacity = existing.scrollbar_opacity;
                    container.last_activity = existing.last_activity;
                    container.copy_success_time = existing.copy_success_time;
                }
                reg.insert(block_id.clone(), container.clone());
            }
        }

        // Use the horizontal scroll helper to create scrollable content
        let scroll_config = HorizontalScrollConfig::default();
        let elements = create_horizontal_scroll_container(
            font,
            line_elements,
            viewport_width,
            content_width,
            container.scroll_offset,
            container.scrollbar_opacity,
            &scroll_config,
            crate::termwindow::UIItemType::CodeBlockScrollbar(block_id.clone()),
        );
        
        log::debug!("Code block {} got {} elements from horizontal scroll container", 
            block_id, elements.len());

        // Separate content and scrollbar if we have multiple elements
        let (code_content, scrollbar_element) = if elements.len() > 1 && needs_scrollbar {
            // We have viewport + scrollbar
            let viewport = elements[0].clone();
            let scrollbar = elements[1].clone();
            (viewport, Some(scrollbar))
        } else {
            // No scrollbar, just content
            (Element::new(font, ElementContent::Children(elements)), None)
        };
        
        // Wrap content in a code block container with padding
        let mut code_block_content = Element::new(font, ElementContent::Children(vec![code_content]))
            .colors(ElementColors {
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                border: BorderColor::new(LinearRgba::with_components(0.2, 0.2, 0.25, 0.5)),
                ..Default::default()
            })
            .padding(BoxDimension::new(Dimension::Pixels(12.0)))
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .display(DisplayType::Block)
            .item_type(crate::termwindow::UIItemType::CodeBlockContent(
                block_id.clone(),
            ));

        // Add focus indicator
        if container.has_focus {
            code_block_content = code_block_content
                .border(BoxDimension::new(Dimension::Pixels(2.0)))
                .colors(ElementColors {
                    bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                    border: BorderColor::new(LinearRgba::with_components(0.3, 0.5, 0.8, 0.8)),
                    ..Default::default()
                });
        }
        
        // Create final element structure
        let code_block = if let Some(scrollbar) = scrollbar_element {
            // Stack code block and scrollbar vertically
            Element::new(font, ElementContent::Children(vec![code_block_content, scrollbar]))
                .margin(BoxDimension {
                    top: Dimension::Pixels(8.0),
                    bottom: Dimension::Pixels(8.0),
                    ..Default::default()
                })
                .display(DisplayType::Block)
        } else {
            // Just the code block with margins
            code_block_content
                .margin(BoxDimension {
                    top: Dimension::Pixels(8.0),
                    bottom: Dimension::Pixels(8.0),
                    ..Default::default()
                })
        };

        // If hovering, add a copy button above the code block
        if container.hovering_content {
            // Check if we should show success state
            let show_success = container.copy_success_time
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
            let wrapper = Element::new(font, ElementContent::Children(vec![copy_button, code_block]))
                .display(DisplayType::Block);
            
            wrapper
        } else {
            code_block
        }
    }
}
