use crate::termwindow::box_model::{
    BorderColor, BoxDimension, Corners, DisplayType, Element, ElementColors, ElementContent,
    SizedPoly,
};
use crate::termwindow::UIItemType;
use config::Dimension;
use std::rc::Rc;
// Widget traits removed - using Element-based rendering instead
use wezterm_font::LoadedFont;
use ::window::color::LinearRgba;

#[derive(Debug, Clone, PartialEq)]
pub enum ChipStyle {
    Default,
    Primary,
    Success,
    Warning,
    Error,
    Info,
    Custom {
        bg: LinearRgba,
        fg: LinearRgba,
        border: LinearRgba,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ChipSize {
    Small,
    Medium,
    Large,
}

pub struct Chip {
    label: String,
    style: ChipStyle,
    size: ChipSize,
    icon: Option<String>,
    closeable: bool,
    clickable: bool,
    selected: bool,
    hovering: bool,
    on_click: Option<Box<dyn Fn()>>,
    on_close: Option<Box<dyn Fn()>>,
    item_type: Option<UIItemType>,
}

impl Chip {
    pub fn new(label: String) -> Self {
        Self {
            label,
            style: ChipStyle::Default,
            size: ChipSize::Medium,
            icon: None,
            closeable: false,
            clickable: false,
            selected: false,
            hovering: false,
            on_click: None,
            on_close: None,
            item_type: None,
        }
    }

    pub fn with_style(mut self, style: ChipStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_size(mut self, size: ChipSize) -> Self {
        self.size = size;
        self
    }

    pub fn with_icon(mut self, icon: String) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn closeable(mut self, closeable: bool) -> Self {
        self.closeable = closeable;
        self
    }

    pub fn clickable(mut self, clickable: bool) -> Self {
        self.clickable = clickable;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_on_click<F: Fn() + 'static>(mut self, handler: F) -> Self {
        self.on_click = Some(Box::new(handler));
        self.clickable = true;
        self
    }

    pub fn with_on_close<F: Fn() + 'static>(mut self, handler: F) -> Self {
        self.on_close = Some(Box::new(handler));
        self.closeable = true;
        self
    }

    pub fn with_item_type(mut self, item_type: UIItemType) -> Self {
        self.item_type = Some(item_type);
        self
    }

    pub fn set_hovering(&mut self, hovering: bool) {
        self.hovering = hovering;
    }

    pub fn toggle_selected(&mut self) {
        self.selected = !self.selected;
    }

    fn get_colors(&self) -> (LinearRgba, LinearRgba, LinearRgba) {
        match &self.style {
            ChipStyle::Default => (
                LinearRgba::with_components(0.2, 0.2, 0.25, 1.0), // bg
                LinearRgba::with_components(0.8, 0.8, 0.8, 1.0),  // fg
                LinearRgba::with_components(0.3, 0.3, 0.35, 1.0), // border
            ),
            ChipStyle::Primary => (
                LinearRgba::with_components(0.1, 0.3, 0.6, 1.0), // bg
                LinearRgba::with_components(1.0, 1.0, 1.0, 1.0), // fg
                LinearRgba::with_components(0.2, 0.4, 0.7, 1.0), // border
            ),
            ChipStyle::Success => (
                LinearRgba::with_components(0.1, 0.4, 0.2, 1.0), // bg
                LinearRgba::with_components(1.0, 1.0, 1.0, 1.0), // fg
                LinearRgba::with_components(0.2, 0.5, 0.3, 1.0), // border
            ),
            ChipStyle::Warning => (
                LinearRgba::with_components(0.5, 0.4, 0.1, 1.0), // bg
                LinearRgba::with_components(1.0, 1.0, 1.0, 1.0), // fg
                LinearRgba::with_components(0.6, 0.5, 0.2, 1.0), // border
            ),
            ChipStyle::Error => (
                LinearRgba::with_components(0.5, 0.1, 0.1, 1.0), // bg
                LinearRgba::with_components(1.0, 1.0, 1.0, 1.0), // fg
                LinearRgba::with_components(0.6, 0.2, 0.2, 1.0), // border
            ),
            ChipStyle::Info => (
                LinearRgba::with_components(0.1, 0.2, 0.5, 1.0), // bg
                LinearRgba::with_components(1.0, 1.0, 1.0, 1.0), // fg
                LinearRgba::with_components(0.2, 0.3, 0.6, 1.0), // border
            ),
            ChipStyle::Custom { bg, fg, border } => (*bg, *fg, *border),
        }
    }

    fn get_padding(&self) -> BoxDimension {
        match self.size {
            ChipSize::Small => BoxDimension {
                left: Dimension::Pixels(6.0),
                right: Dimension::Pixels(6.0),
                top: Dimension::Pixels(2.0),
                bottom: Dimension::Pixels(2.0),
            },
            ChipSize::Medium => BoxDimension {
                left: Dimension::Pixels(10.0),
                right: Dimension::Pixels(10.0),
                top: Dimension::Pixels(4.0),
                bottom: Dimension::Pixels(4.0),
            },
            ChipSize::Large => BoxDimension {
                left: Dimension::Pixels(14.0),
                right: Dimension::Pixels(14.0),
                top: Dimension::Pixels(6.0),
                bottom: Dimension::Pixels(6.0),
            },
        }
    }

    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let mut children = Vec::new();

        // Add icon if present
        if let Some(icon) = &self.icon {
            children.push(
                Element::new(font, ElementContent::Text(format!("{} ", icon))).colors(
                    ElementColors {
                        text: self.get_colors().1.into(),
                        ..Default::default()
                    },
                ),
            );
        }

        // Add label
        children.push(
            Element::new(font, ElementContent::Text(self.label.clone())).colors(ElementColors {
                text: self.get_colors().1.into(),
                ..Default::default()
            }),
        );

        // Add close button if closeable
        if self.closeable {
            children.push(
                Element::new(font, ElementContent::Text(" ×".to_string()))
                    .colors(ElementColors {
                        text: self.get_colors().1.into(),
                        ..Default::default()
                    })
                    .padding(BoxDimension {
                        left: Dimension::Pixels(4.0),
                        ..Default::default()
                    }),
            );
        }

        let (mut bg_color, fg_color, mut border_color) = self.get_colors();

        // Adjust colors for selected state
        if self.selected {
            bg_color = LinearRgba::with_components(
                bg_color.0 * 1.3,
                bg_color.1 * 1.3,
                bg_color.2 * 1.3,
                bg_color.3,
            );
            border_color = LinearRgba::with_components(
                border_color.0 * 1.5,
                border_color.1 * 1.5,
                border_color.2 * 1.5,
                border_color.3,
            );
        }

        // Create the chip
        let mut chip = Element::new(font, ElementContent::Children(children))
            .display(DisplayType::Inline)
            .padding(self.get_padding())
            .border(BoxDimension::new(Dimension::Pixels(1.0)))
            .border_corners(Some(Corners {
                top_left: SizedPoly::none(),
                top_right: SizedPoly::none(),
                bottom_left: SizedPoly::none(),
                bottom_right: SizedPoly::none(),
            }))
            .colors(ElementColors {
                bg: bg_color.into(),
                text: fg_color.into(),
                border: BorderColor::new(border_color),
            })
            .margin(BoxDimension {
                right: Dimension::Pixels(4.0),
                bottom: Dimension::Pixels(4.0),
                ..Default::default()
            });

        // Add hover colors if clickable or closeable
        if (self.clickable || self.closeable) && self.hovering {
            let hover_bg = LinearRgba::with_components(
                bg_color.0 * 1.2,
                bg_color.1 * 1.2,
                bg_color.2 * 1.2,
                bg_color.3,
            );
            let hover_border = LinearRgba::with_components(
                border_color.0 * 1.3,
                border_color.1 * 1.3,
                border_color.2 * 1.3,
                border_color.3,
            );

            chip = chip.hover_colors(Some(ElementColors {
                bg: hover_bg.into(),
                text: fg_color.into(),
                border: BorderColor::new(hover_border),
            }));
        }

        // Add item type if specified
        if let Some(item_type) = &self.item_type {
            chip = chip.item_type(item_type.clone());
        }

        chip
    }

    pub fn handle_click(&self) {
        if let Some(handler) = &self.on_click {
            handler();
        }
    }

    pub fn handle_close(&self) {
        if let Some(handler) = &self.on_close {
            handler();
        }
    }
}

pub struct ChipGroup {
    chips: Vec<Chip>,
    multi_select: bool,
}

impl ChipGroup {
    pub fn new() -> Self {
        Self {
            chips: Vec::new(),
            multi_select: false,
        }
    }

    pub fn with_multi_select(mut self, multi_select: bool) -> Self {
        self.multi_select = multi_select;
        self
    }

    pub fn add_chip(&mut self, chip: Chip) {
        self.chips.push(chip);
    }

    pub fn handle_chip_click(&mut self, index: usize) {
        if index >= self.chips.len() {
            return;
        }
        
        if self.multi_select {
            self.chips[index].toggle_selected();
        } else {
            // Single select mode - deselect all others
            for i in 0..self.chips.len() {
                self.chips[i].selected = i == index;
            }
        }
        
        // Call the click handler after updating selection
        if let Some(chip) = self.chips.get(index) {
            chip.handle_click();
        }
    }

    pub fn get_selected(&self) -> Vec<usize> {
        self.chips
            .iter()
            .enumerate()
            .filter_map(|(i, chip)| if chip.selected { Some(i) } else { None })
            .collect()
    }

    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        let chip_elements: Vec<Element> = self.chips.iter().map(|chip| chip.render(font)).collect();

        Element::new(font, ElementContent::Children(chip_elements))
            .display(DisplayType::Block)
            .padding(BoxDimension::new(Dimension::Pixels(4.0)))
    }
}

// Note: Event handling will be integrated with the sidebar's mouse event handling
