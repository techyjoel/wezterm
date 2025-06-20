use crate::termwindow::box_model::{
    BorderColor, BoxDimension, DisplayType, Element, ElementColors, ElementContent, Float,
};
use ::window::color::LinearRgba;
use config::Dimension;
use std::rc::Rc;
use termwiz::input::{MouseButtons, MouseEvent};
use wezterm_font::LoadedFont;

pub struct ScrollableContainer {
    content: Vec<Element>,
    viewport_height: usize,
    total_height: usize,
    scroll_offset: usize,
    top_row: usize,
    max_visible_items: usize,
    show_scrollbar: bool,
    scrollbar_width: f32,
    auto_hide_scrollbar: bool,
    smooth_scroll: bool,
    scroll_speed: usize,
    hovering_scrollbar: bool,
    dragging_scrollbar: bool,
    drag_start_y: Option<f32>,
    drag_start_offset: Option<usize>,
}

impl ScrollableContainer {
    pub fn new(viewport_height: usize) -> Self {
        Self {
            content: Vec::new(),
            viewport_height,
            total_height: 0,
            scroll_offset: 0,
            top_row: 0,
            max_visible_items: viewport_height,
            show_scrollbar: true,
            scrollbar_width: 8.0,
            auto_hide_scrollbar: true,
            smooth_scroll: true,
            scroll_speed: 3,
            hovering_scrollbar: false,
            dragging_scrollbar: false,
            drag_start_y: None,
            drag_start_offset: None,
        }
    }

    pub fn with_content(mut self, content: Vec<Element>) -> Self {
        self.content = content;
        self.update_total_height();
        self
    }

    pub fn with_auto_hide_scrollbar(mut self, auto_hide: bool) -> Self {
        self.auto_hide_scrollbar = auto_hide;
        self
    }

    pub fn with_smooth_scroll(mut self, smooth: bool) -> Self {
        self.smooth_scroll = smooth;
        self
    }

    pub fn set_content(&mut self, content: Vec<Element>) {
        self.content = content;
        self.update_total_height();
        self.constrain_scroll();
    }

    pub fn add_item(&mut self, item: Element) {
        self.content.push(item);
        self.update_total_height();
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.total_height = 0;
        self.scroll_offset = 0;
        self.top_row = 0;
    }

    fn update_total_height(&mut self) {
        // In a real implementation, we'd calculate actual element heights
        // For now, assume each element has a fixed height
        self.total_height = self.content.len();
        self.max_visible_items = self.viewport_height.min(self.total_height);
    }

    fn constrain_scroll(&mut self) {
        if self.total_height <= self.viewport_height {
            self.scroll_offset = 0;
            self.top_row = 0;
        } else {
            let max_scroll = self.total_height.saturating_sub(self.viewport_height);
            self.scroll_offset = self.scroll_offset.min(max_scroll);
            self.top_row = self.scroll_offset;
        }
    }

    pub fn scroll_up(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
        self.top_row = self.scroll_offset;
        self.constrain_scroll();
    }

    pub fn scroll_down(&mut self, lines: usize) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
        self.top_row = self.scroll_offset;
        self.constrain_scroll();
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.top_row = 0;
    }

    pub fn scroll_to_bottom(&mut self) {
        if self.total_height > self.viewport_height {
            self.scroll_offset = self.total_height - self.viewport_height;
            self.top_row = self.scroll_offset;
        }
    }

    pub fn is_at_bottom(&self) -> bool {
        if self.total_height <= self.viewport_height {
            true
        } else {
            self.scroll_offset >= self.total_height - self.viewport_height
        }
    }

    fn get_scrollbar_thumb_info(&self) -> (f32, f32) {
        if self.total_height <= self.viewport_height {
            return (0.0, self.viewport_height as f32);
        }

        let viewport_ratio = self.viewport_height as f32 / self.total_height as f32;
        let thumb_height = (viewport_ratio * self.viewport_height as f32).max(20.0);

        let scroll_ratio =
            self.scroll_offset as f32 / (self.total_height - self.viewport_height) as f32;
        let thumb_top = scroll_ratio * (self.viewport_height as f32 - thumb_height);

        (thumb_top, thumb_height)
    }

    pub fn render(&self, font: &Rc<LoadedFont>) -> Element {
        // Create content area
        let mut content_children = Vec::new();
        for (_idx, item) in self
            .content
            .iter()
            .skip(self.top_row)
            .take(self.max_visible_items)
            .enumerate()
        {
            content_children.push(item.clone());
        }

        let content_area = Element::new(font, ElementContent::Children(content_children))
            .display(DisplayType::Block);

        // If no scrollbar needed, just return content
        if !self.should_show_scrollbar() {
            return content_area;
        }

        // Create scrollbar visual elements
        let (thumb_top, thumb_height) = self.get_scrollbar_thumb_info();
        
        // For now, add a visual indicator of scroll position
        let scroll_indicator = Element::new(
            font,
            ElementContent::Text(format!(
                "▐ {}/{}",
                self.top_row + self.max_visible_items.min(self.content.len() - self.top_row),
                self.content.len()
            ))
        )
        .colors(ElementColors {
            text: LinearRgba::with_components(0.4, 0.4, 0.45, 0.7).into(),
            ..Default::default()
        })
        .padding(BoxDimension {
            top: Dimension::Pixels(4.0),
            left: Dimension::Pixels(8.0),
            bottom: Dimension::Pixels(4.0),
            ..Default::default()
        });

        // Return content with scroll indicator
        Element::new(
            font,
            ElementContent::Children(vec![content_area, scroll_indicator])
        )
        .display(DisplayType::Block)
    }

    fn should_show_scrollbar(&self) -> bool {
        if !self.show_scrollbar {
            return false;
        }

        if self.total_height <= self.viewport_height {
            return false;
        }

        if self.auto_hide_scrollbar {
            self.hovering_scrollbar || self.dragging_scrollbar
        } else {
            true
        }
    }

    pub fn handle_mouse_event(&mut self, event: &MouseEvent) -> bool {
        if event.mouse_buttons.contains(MouseButtons::VERT_WHEEL) {
            if event.mouse_buttons.contains(MouseButtons::WHEEL_POSITIVE) {
                self.scroll_up(self.scroll_speed);
            } else {
                self.scroll_down(self.scroll_speed);
            }
            true
        } else if event.mouse_buttons == MouseButtons::LEFT {
            if self.is_over_scrollbar(event.x, event.y) {
                self.dragging_scrollbar = true;
                self.drag_start_y = Some(event.y as f32);
                self.drag_start_offset = Some(self.scroll_offset);
                true
            } else {
                false
            }
        } else if event.mouse_buttons == MouseButtons::NONE {
            if self.dragging_scrollbar {
                if let (Some(start_y), Some(start_offset)) =
                    (self.drag_start_y, self.drag_start_offset)
                {
                    let delta_y = event.y as f32 - start_y;
                    let scroll_ratio = delta_y / self.viewport_height as f32;
                    let scroll_delta = (scroll_ratio * self.total_height as f32) as isize;

                    self.scroll_offset = (start_offset as isize + scroll_delta).max(0) as usize;
                    self.constrain_scroll();
                }
                true
            } else {
                let was_hovering = self.hovering_scrollbar;
                self.hovering_scrollbar = self.is_over_scrollbar(event.x, event.y);
                was_hovering != self.hovering_scrollbar
            }
        } else {
            false
        }
    }

    pub fn handle_mouse_release(&mut self) {
        self.dragging_scrollbar = false;
        self.drag_start_y = None;
        self.drag_start_offset = None;
    }

    fn is_over_scrollbar(&self, _x: u16, _y: u16) -> bool {
        // This is a simplified check - in practice you'd need the actual rendered bounds
        false
    }
}

// Note: Event handling will be integrated with the sidebar's mouse event handling
