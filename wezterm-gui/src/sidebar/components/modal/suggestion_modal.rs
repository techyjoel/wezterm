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
    run_button_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,
    dismiss_button_bounds: Option<euclid::Rect<f32, window::PixelUnit>>,
}

impl SuggestionModal {
    pub fn new(suggestion: CurrentSuggestion) -> Self {
        Self {
            suggestion,
            content_height: Mutex::new(0.0),
            run_button_bounds: None,
            dismiss_button_bounds: None,
        }
    }
}

impl ModalContent for SuggestionModal {
    fn render(&self, context: &ModalRenderContext) -> Element {
        let mut children = vec![];
        let padding = 20.0;

        // Title
        let title = Element::new(
            &context.fonts.heading,
            ElementContent::Text(self.suggestion.title.clone()),
        )
        .colors(ElementColors {
            border: BorderColor::default(),
            bg: LinearRgba(0.0, 0.0, 0.0, 0.0).into(),
            text: LinearRgba(1.0, 1.0, 1.0, 0.9).into(),
        })
        .padding(BoxDimension {
            left: Dimension::Pixels(0.0),
            right: Dimension::Pixels(0.0),
            top: Dimension::Pixels(0.0),
            bottom: Dimension::Pixels(12.0),
        });

        children.push(title);

        // Render markdown content
        let content = MarkdownRenderer::render_with_code_font(
            &self.suggestion.content,
            &context.fonts.body,
            &context.fonts.code,
        )
        .padding(BoxDimension {
            left: Dimension::Pixels(0.0),
            right: Dimension::Pixels(0.0),
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
                .clickable(true);
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
                .clickable(true);
            button_row.push(dismiss_chip.render(&context.fonts.body));

            let button_container =
                Element::new(&context.fonts.body, ElementContent::Children(button_row))
                    .display(DisplayType::Inline)
                    .padding(BoxDimension {
                        left: Dimension::Pixels(0.0),
                        right: Dimension::Pixels(0.0),
                        top: Dimension::Pixels(8.0),
                        bottom: Dimension::Pixels(0.0),
                    });

            children.push(button_container);
        }

        // Create scrollable content container
        let content_container =
            Element::new(&context.fonts.body, ElementContent::Children(children))
                .display(DisplayType::Block)
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
        // This is a rough estimate - in a real implementation, we'd measure after rendering
        let line_height = context.fonts.body.metrics().cell_height.get() as f32;
        let estimated_lines = (self.suggestion.content.len() as f32 / 60.0).ceil(); // Rough estimate
        let title_height = context.fonts.heading.metrics().cell_height.get() as f32;
        let button_height = if self.suggestion.has_action {
            40.0
        } else {
            0.0
        };

        let estimated_height =
            title_height + (estimated_lines * line_height) + button_height + 40.0;
        *self.content_height.lock().unwrap() = estimated_height;

        content_container
    }

    fn handle_event(&mut self, event: &ModalEvent) -> ModalEventResult {
        match event {
            ModalEvent::Mouse(mouse_event) => {
                use window::{MouseEventKind as WMEK, MousePress};

                match &mouse_event.kind {
                    WMEK::Press(MousePress::Left) => {
                        let point = euclid::point2(
                            mouse_event.coords.x as f32,
                            mouse_event.coords.y as f32,
                        );

                        // Check button clicks if we have tracked their bounds
                        if let Some(bounds) = self.run_button_bounds {
                            if bounds.contains(point) {
                                // Handle run action
                                log::info!("Run button clicked for suggestion");
                                return ModalEventResult::Close;
                            }
                        }

                        if let Some(bounds) = self.dismiss_button_bounds {
                            if bounds.contains(point) {
                                // Handle dismiss action
                                log::info!("Dismiss button clicked for suggestion");
                                return ModalEventResult::Close;
                            }
                        }
                    }
                    _ => {}
                }
            }
            ModalEvent::Key { key, mods } => {
                // Could add keyboard shortcuts here
            }
        }

        ModalEventResult::NotHandled
    }

    fn get_content_height(&self) -> f32 {
        *self.content_height.lock().unwrap()
    }
}
