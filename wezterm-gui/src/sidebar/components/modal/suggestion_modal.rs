use super::{ModalContent, ModalEvent, ModalEventResult, ModalRenderContext};
use crate::color::LinearRgba;
use crate::sidebar::ai_sidebar::CurrentSuggestion;
use crate::sidebar::components::markdown::MarkdownRenderer;
use crate::sidebar::components::{Chip, ChipSize, ChipStyle};
use crate::termwindow::box_model::*;
use config::Dimension;
use std::sync::Mutex;

pub struct SuggestionModal {
    pub suggestion: CurrentSuggestion,
    content_height: Mutex<f32>,
}

impl SuggestionModal {
    pub fn new(suggestion: CurrentSuggestion) -> Self {
        Self {
            suggestion,
            content_height: Mutex::new(0.0),
        }
    }
}

impl ModalContent for SuggestionModal {
    fn render(&self, context: &ModalRenderContext) -> Element {
        let mut children = vec![];
        let padding = 20.0;

        // Title with width constraint
        let title_width = context.modal_bounds.width() - 20.0; // Account for scrollbar space
        let title = Element::new(
            &context.fonts.heading,
            ElementContent::WrappedText(self.suggestion.title.clone()),
        )
        .colors(ElementColors {
            border: BorderColor::default(),
            bg: LinearRgba(0.0, 0.0, 0.0, 0.0).into(),
            text: LinearRgba(1.0, 1.0, 1.0, 0.9).into(),
        })
        .max_width(Some(Dimension::Pixels(title_width)))
        .padding(BoxDimension {
            left: Dimension::Pixels(0.0),
            right: Dimension::Pixels(0.0),
            top: Dimension::Pixels(0.0),
            bottom: Dimension::Pixels(12.0),
        });

        children.push(title);

        // Render markdown content with width constraint and code block registry
        // Subtract padding from width to prevent text overflow
        let content_width = context.modal_bounds.width() - 20.0; // Account for right padding

        // Get code block registry from context if available
        let content = if let Some(registry) = context.code_block_registry.as_ref() {
            MarkdownRenderer::render_with_registry(
                &self.suggestion.content,
                &context.fonts.body,
                &context.fonts.code,
                Some(content_width),
                registry.clone(),
            )
        } else {
            MarkdownRenderer::render_with_width(
                &self.suggestion.content,
                &context.fonts.body,
                &context.fonts.code,
                Some(content_width),
            )
        }
        .max_width(Some(Dimension::Pixels(content_width)))
        .padding(BoxDimension {
            left: Dimension::Pixels(0.0),
            right: Dimension::Pixels(20.0), // Add right padding to avoid scrollbar
            top: Dimension::Pixels(0.0),
            bottom: Dimension::Pixels(20.0),
        });

        children.push(content);

        // Action buttons if applicable
        if self.suggestion.has_action {
            let mut button_row = vec![];

            // Run button
            let run_chip = Chip::new("▶ Run".to_string())
                .with_style(ChipStyle::Success)
                .with_size(ChipSize::Large)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::SuggestionRunButton);
            button_row.push(run_chip.render(&context.fonts.body));

            // Spacing between buttons
            button_row.push(
                Element::new(&context.fonts.body, ElementContent::Text(" ".to_string()))
                    .min_width(Some(Dimension::Pixels(8.0))),
            );

            // Dismiss button
            let dismiss_chip = Chip::new("✕ Dismiss".to_string())
                .with_style(ChipStyle::Default)
                .with_size(ChipSize::Large)
                .clickable(true)
                .with_item_type(crate::termwindow::UIItemType::SuggestionDismissButton);
            button_row.push(dismiss_chip.render(&context.fonts.body));

            let button_container =
                Element::new(&context.fonts.body, ElementContent::Children(button_row))
                    .display(DisplayType::Block)
                    .padding(BoxDimension {
                        left: Dimension::Pixels(0.0),
                        right: Dimension::Pixels(0.0),
                        top: Dimension::Pixels(8.0),
                        bottom: Dimension::Pixels(0.0),
                    });

            children.push(button_container);
        }

        // Create scrollable content container with explicit width constraint
        let content_container =
            Element::new(&context.fonts.body, ElementContent::Children(children))
                .display(DisplayType::Block)
                .max_width(Some(Dimension::Pixels(context.modal_bounds.width())))
                .padding(BoxDimension {
                    left: Dimension::Pixels(0.0),
                    right: Dimension::Pixels(0.0),
                    top: Dimension::Pixels(0.0),
                    bottom: Dimension::Pixels(0.0),
                })
                .margin(BoxDimension {
                    left: Dimension::Pixels(0.0),
                    top: Dimension::Pixels(-context.scroll_offset),
                    right: Dimension::Pixels(0.0),
                    bottom: Dimension::Pixels(0.0),
                });

        // Calculate content height (estimate based on line count and font metrics)
        let line_height = context.fonts.body.metrics().cell_height.get() as f32;

        // Count actual lines in the content (including markdown line breaks)
        let line_count = self.suggestion.content.lines().count() as f32;

        // Add extra lines for markdown formatting (headers, code blocks, etc)
        let markdown_overhead = self.suggestion.content.matches("##").count() as f32 * 1.5
            + self.suggestion.content.matches("```").count() as f32 * 0.5;

        let total_lines = line_count + markdown_overhead;

        let title_height = context.fonts.heading.metrics().cell_height.get() as f32;
        let button_height = if self.suggestion.has_action {
            60.0 // More space for button container
        } else {
            0.0
        };

        let estimated_height = title_height + (total_lines * line_height) + button_height + 80.0; // More padding
        *self.content_height.lock().unwrap() = estimated_height;

        log::debug!(
            "SuggestionModal content height calculation: title_height={}, line_count={}, markdown_overhead={}, total_lines={}, line_height={}, button_height={}, estimated_height={}",
            title_height, line_count, markdown_overhead, total_lines, line_height, button_height, estimated_height
        );

        content_container
    }

    fn handle_event(&mut self, event: &ModalEvent) -> ModalEventResult {
        match event {
            ModalEvent::Mouse(_mouse_event) => {
                // Button clicks are now handled via UIItemType
            }
            ModalEvent::Key { key: _, mods: _ } => {
                // Could add keyboard shortcuts here
            }
        }

        ModalEventResult::NotHandled
    }

    fn get_content_height(&self) -> f32 {
        *self.content_height.lock().unwrap()
    }
}
