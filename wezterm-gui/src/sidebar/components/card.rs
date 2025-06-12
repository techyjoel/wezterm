use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent,
};
use crate::termwindow::UIItemType;
use config::Dimension;
use std::rc::Rc;
// Widget traits removed - using Element-based rendering instead
use wezterm_font::LoadedFont;
use ::window::color::LinearRgba;

#[derive(Debug, Clone, PartialEq)]
pub enum CardState {
    Normal,
    Expanded,
    Collapsed,
}

pub struct Card {
    title: Option<String>,
    content: Vec<Element>,
    actions: Vec<Element>,
    state: CardState,
    hover_state: bool,
    expandable: bool,
    item_type: Option<UIItemType>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            title: None,
            content: Vec::new(),
            actions: Vec::new(),
            state: CardState::Normal,
            hover_state: false,
            expandable: false,
            item_type: None,
        }
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_content(mut self, content: Vec<Element>) -> Self {
        self.content = content;
        self
    }

    pub fn with_actions(mut self, actions: Vec<Element>) -> Self {
        self.actions = actions;
        self
    }

    pub fn expandable(mut self, expandable: bool) -> Self {
        self.expandable = expandable;
        self
    }

    pub fn with_item_type(mut self, item_type: UIItemType) -> Self {
        self.item_type = Some(item_type);
        self
    }

    pub fn toggle_expand(&mut self) {
        if self.expandable {
            self.state = match self.state {
                CardState::Normal | CardState::Collapsed => CardState::Expanded,
                CardState::Expanded => CardState::Collapsed,
            };
        }
    }

    pub fn set_hover(&mut self, hover: bool) {
        self.hover_state = hover;
    }

    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let mut children = Vec::new();

        // Card header with title
        if let Some(title) = &self.title {
            let mut header_children = vec![Element::new(font, ElementContent::Text(title.clone()))
                .colors(ElementColors {
                    text: LinearRgba::with_components(0.9, 0.9, 0.9, 1.0).into(),
                    ..Default::default()
                })
                .padding(BoxDimension::new(Dimension::Pixels(8.0)))];

            // Add expand/collapse indicator if expandable
            if self.expandable {
                let indicator = match self.state {
                    CardState::Collapsed => "▶",
                    CardState::Expanded => "▼",
                    CardState::Normal => "▶",
                };
                header_children.insert(
                    0,
                    Element::new(font, ElementContent::Text(indicator.to_string()))
                        .colors(ElementColors {
                            text: LinearRgba::with_components(0.7, 0.7, 0.7, 1.0).into(),
                            ..Default::default()
                        })
                        .padding(BoxDimension {
                            left: Dimension::Pixels(8.0),
                            right: Dimension::Pixels(4.0),
                            top: Dimension::Pixels(8.0),
                            bottom: Dimension::Pixels(8.0),
                        }),
                );
            }

            let header = Element::new(font, ElementContent::Children(header_children))
                .display(DisplayType::Block)
                .colors(ElementColors {
                    bg: LinearRgba::with_components(0.15, 0.15, 0.17, 1.0).into(),
                    ..Default::default()
                })
                .border(BoxDimension {
                    bottom: Dimension::Pixels(1.0),
                    ..Default::default()
                })
                .colors(ElementColors {
                    border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 1.0)),
                    bg: LinearRgba::with_components(0.15, 0.15, 0.17, 1.0).into(),
                    ..Default::default()
                });

            children.push(header);
        }

        // Card content (only if not collapsed)
        if self.state != CardState::Collapsed && !self.content.is_empty() {
            let content_wrapper =
                Element::new(font, ElementContent::Children(self.content.clone()))
                    .display(DisplayType::Block)
                    .padding(BoxDimension::new(Dimension::Pixels(12.0)));
            children.push(content_wrapper);
        }

        // Card actions (only if not collapsed)
        if self.state != CardState::Collapsed && !self.actions.is_empty() {
            let actions_wrapper =
                Element::new(font, ElementContent::Children(self.actions.clone()))
                    .display(DisplayType::Block)
                    .padding(BoxDimension::new(Dimension::Pixels(8.0)))
                    .border(BoxDimension {
                        top: Dimension::Pixels(1.0),
                        ..Default::default()
                    })
                    .colors(ElementColors {
                        border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 1.0)),
                        ..Default::default()
                    });
            children.push(actions_wrapper);
        }

        // Create the card container
        let mut card = Element::new(font, ElementContent::Children(children))
            .display(DisplayType::Block)
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .margin(BoxDimension::new(Dimension::Pixels(8.0)))
            .colors(ElementColors {
                border: BorderColor::new(LinearRgba::with_components(0.3, 0.3, 0.35, 1.0)),
                bg: LinearRgba::with_components(0.1, 0.1, 0.12, 1.0).into(),
                ..Default::default()
            });

        // Add hover colors if in hover state
        if self.hover_state {
            card = card.hover_colors(Some(ElementColors {
                border: BorderColor::new(LinearRgba::with_components(0.4, 0.4, 0.45, 1.0)),
                bg: LinearRgba::with_components(0.13, 0.13, 0.15, 1.0).into(),
                ..Default::default()
            }));
        }

        // Add item type if specified
        if let Some(item_type) = &self.item_type {
            card = card.item_type(item_type.clone());
        }

        card
    }
}

// Note: Event handling will be integrated with the sidebar's mouse event handling
