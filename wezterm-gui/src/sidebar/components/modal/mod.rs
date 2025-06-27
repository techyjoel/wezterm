use crate::color::LinearRgba;
use crate::sidebar::SidebarFonts;
use crate::termwindow::box_model::*;
use config::Dimension;
use std::sync::{Arc, Mutex};
use termwiz::input::KeyCode;
use wezterm_term::KeyModifiers;
use window::RectF;

pub mod animation;
pub mod content;
pub mod suggestion_modal;

pub use animation::*;
pub use content::*;
pub use suggestion_modal::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalSize {
    FillSidebar,
    HalfWindow,
    Fixed(f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalAnimationState {
    Opening,
    Open,
    Closing,
    Closed,
}

pub struct Modal {
    pub id: String,
    pub size: ModalSize,
    pub content: Box<dyn ModalContent>,
    pub animation_state: ModalAnimationState,
    pub close_on_click_outside: bool,
    pub close_on_escape: bool,
    pub position: Option<RectF>,
}

pub struct ModalManager {
    active_modal: Option<Modal>,
    dimmer_opacity: f32,
    animation_start: Option<std::time::Instant>,
    scroll_offset: f32,
    content_height: f32,
    visible_height: f32,
    // Scrollbar interaction state
    hovering_scrollbar: bool,
    dragging_scrollbar: bool,
    drag_start_y: Option<f32>,
    drag_start_offset: Option<f32>,
}

impl ModalManager {
    pub fn new() -> Self {
        Self {
            active_modal: None,
            dimmer_opacity: 0.0,
            animation_start: None,
            scroll_offset: 0.0,
            content_height: 0.0,
            visible_height: 0.0,
            hovering_scrollbar: false,
            dragging_scrollbar: false,
            drag_start_y: None,
            drag_start_offset: None,
        }
    }

    pub fn show(&mut self, mut modal: Modal) {
        modal.animation_state = ModalAnimationState::Opening;
        self.active_modal = Some(modal);
        self.dimmer_opacity = 0.7; // Target opacity
        self.animation_start = Some(std::time::Instant::now());
        self.reset_scroll();
    }

    pub fn close(&mut self) {
        if let Some(ref mut modal) = self.active_modal {
            modal.animation_state = ModalAnimationState::Closing;
            self.dimmer_opacity = 0.0; // Fade out
            self.animation_start = Some(std::time::Instant::now());
        }
    }

    pub fn is_active(&self) -> bool {
        self.active_modal.is_some()
    }

    pub fn update(&mut self) {
        // Update animation state
        if let Some(ref mut modal) = self.active_modal {
            match modal.animation_state {
                ModalAnimationState::Opening => {
                    if self.dimmer_opacity >= 0.69 {
                        modal.animation_state = ModalAnimationState::Open;
                    }
                }
                ModalAnimationState::Closing => {
                    if self.dimmer_opacity <= 0.01 {
                        modal.animation_state = ModalAnimationState::Closed;
                        self.active_modal = None;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn render(
        &mut self,
        sidebar_bounds: RectF,
        window_bounds: RectF,
        fonts: &SidebarFonts,
    ) -> Vec<Element> {
        self.update();

        let mut elements = vec![];

        // Extract values before mutable borrow
        let (modal_size, opacity) = if let Some(ref modal) = self.active_modal {
            (Some(modal.size), self.dimmer_opacity)
        } else {
            (None, 0.0)
        };

        if let Some(size) = modal_size {
            // Render dimmer at z-index 20 - only over sidebar area
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg: LinearRgba(0.0, 0.0, 0.0, opacity * 0.5).into(), // Semi-transparent dimmer
                        text: LinearRgba(1.0, 1.0, 1.0, 1.0).into(),
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(sidebar_bounds.width())))
                    .min_height(Some(Dimension::Pixels(sidebar_bounds.height())))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(sidebar_bounds.min_x()),
                        top: Dimension::Pixels(0.0),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(20),
            );

            // Calculate modal bounds based on size
            let modal_bounds = self.calculate_modal_bounds(size, sidebar_bounds, window_bounds);

            // Store position for event handling
            if let Some(ref mut modal) = self.active_modal {
                modal.position = Some(modal_bounds);
            }

            // Update visible height for scrolling
            self.visible_height = modal_bounds.height() - 60.0; // Account for header and padding
            log::debug!(
                "Modal bounds: width={}, height={}, visible_height={}",
                modal_bounds.width(),
                modal_bounds.height(),
                self.visible_height
            );

            // Render modal container with shadow at z-index 21
            let shadow_color = LinearRgba(0.0, 0.0, 0.0, 0.3 * opacity);

            // Shadow (offset slightly)
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg: shadow_color.into(),
                        text: LinearRgba(1.0, 1.0, 1.0, 1.0).into(),
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(modal_bounds.width() as f32)))
                    .min_height(Some(Dimension::Pixels(modal_bounds.height() as f32)))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(modal_bounds.min_x() + 4.0),
                        top: Dimension::Pixels(modal_bounds.min_y() + 4.0),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(21),
            );

            // Modal background with cut-a-hole pattern
            // We'll render the background in sections, leaving a hole for the scrollable content
            let content_area_top = modal_bounds.min_y() + 40.0; // Header height
            let content_area_bottom = modal_bounds.max_y() - 20.0; // Bottom padding
            let content_area_left = modal_bounds.min_x() + 20.0;
            let content_area_right = modal_bounds.max_x() - 20.0;
            
            let bg_color = LinearRgba(0.2, 0.2, 0.2, 1.0);
            let border_color = LinearRgba(0.4, 0.4, 0.4, 1.0);
            
            // Top section (header area) at z-index 22
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(modal_bounds.width())))
                    .min_height(Some(Dimension::Pixels(content_area_top - modal_bounds.min_y())))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(modal_bounds.min_x()),
                        top: Dimension::Pixels(modal_bounds.min_y()),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(22),
            );
            
            // Bottom section at z-index 22
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(modal_bounds.width())))
                    .min_height(Some(Dimension::Pixels(modal_bounds.max_y() - content_area_bottom)))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(modal_bounds.min_x()),
                        top: Dimension::Pixels(content_area_bottom),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(22),
            );
            
            // Left edge at z-index 22
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(content_area_left - modal_bounds.min_x())))
                    .min_height(Some(Dimension::Pixels(content_area_bottom - content_area_top)))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(modal_bounds.min_x()),
                        top: Dimension::Pixels(content_area_top),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(22),
            );
            
            // Right edge (includes scrollbar area) at z-index 22
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(modal_bounds.max_x() - content_area_right)))
                    .min_height(Some(Dimension::Pixels(content_area_bottom - content_area_top)))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(content_area_right),
                        top: Dimension::Pixels(content_area_top),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(22),
            );
            
            // Modal border at z-index 22 (same as frame sections)
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        border: BorderColor::new(border_color),
                        bg: LinearRgba(0.0, 0.0, 0.0, 0.0).into(), // Transparent
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(modal_bounds.width())))
                    .min_height(Some(Dimension::Pixels(modal_bounds.height())))
                    .border(BoxDimension::new(Dimension::Pixels(1.0)))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(modal_bounds.min_x()),
                        top: Dimension::Pixels(modal_bounds.min_y()),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(22),
            );

            // Render close button with proper positioning
            let close_button_size = 32.0;
            let close_button_x = modal_bounds.max_x() - close_button_size - 8.0;
            let close_button_y = modal_bounds.min_y() + 8.0;

            // Close button background at z-index 22 (same as frame)
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg: LinearRgba(0.3, 0.3, 0.3, 0.8).into(),
                        text: LinearRgba(1.0, 1.0, 1.0, 1.0).into(),
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(close_button_size)))
                    .min_height(Some(Dimension::Pixels(close_button_size)))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(close_button_x),
                        top: Dimension::Pixels(close_button_y),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(22),
            );

            // Close button X text at z-index 22 (same as frame)
            elements.push(
                Element::new(&fonts.heading, ElementContent::Text("X".to_string()))
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg: LinearRgba(0.0, 0.0, 0.0, 0.0).into(),
                        text: LinearRgba(0.9, 0.9, 0.9, 1.0).into(),
                    })
                    .display(DisplayType::Block)
                    .margin(BoxDimension {
                        left: Dimension::Pixels(close_button_x + 11.0), // Center the X
                        top: Dimension::Pixels(close_button_y + 5.0),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(22),
            );

            // Render modal content
            let content_bounds = euclid::rect(
                modal_bounds.min_x() + 20.0,
                modal_bounds.min_y() + 40.0,
                modal_bounds.width() - 40.0,
                modal_bounds.height() - 60.0,
            );

            // Add content area background at z-index 20 (lowest layer, will show through the hole)
            elements.push(
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        bg: bg_color.into(),
                        ..Default::default()
                    })
                    .display(DisplayType::Block)
                    .min_width(Some(Dimension::Pixels(content_bounds.width())))
                    .min_height(Some(Dimension::Pixels(content_bounds.height())))
                    .margin(BoxDimension {
                        left: Dimension::Pixels(content_bounds.min_x()),
                        top: Dimension::Pixels(content_bounds.min_y()),
                        right: Dimension::Pixels(0.0),
                        bottom: Dimension::Pixels(0.0),
                    })
                    .zindex(20),
            );

            let context = ModalRenderContext {
                modal_bounds: content_bounds,
                fonts,
                visible_height: self.visible_height,
                scroll_offset: self.scroll_offset,
            };

            if let Some(ref modal) = self.active_modal {
                // Create a container for the modal content with proper bounds
                let content_element = modal.content.render(&context);

                // Wrap content in a positioned container with proper width constraints
                // Use z-index 21 so it renders on top of the content background (20) but below the modal frame (22)
                let positioned_content =
                    Element::new(&fonts.body, ElementContent::Children(vec![content_element]))
                        .display(DisplayType::Block)
                        .min_width(Some(Dimension::Pixels(content_bounds.width())))
                        .max_width(Some(Dimension::Pixels(content_bounds.width())))
                        .margin(BoxDimension {
                            left: Dimension::Pixels(content_bounds.min_x()),
                            top: Dimension::Pixels(content_bounds.min_y()),
                            right: Dimension::Pixels(0.0),
                            bottom: Dimension::Pixels(0.0),
                        })
                        .zindex(21);

                elements.push(positioned_content);

                // Update content height
                self.content_height = modal.content.get_content_height();
            }

            // Render scrollbar if needed at z-index 23 (above everything else)
            log::debug!(
                "Modal scrollbar check: content_height={}, visible_height={}, should_show={}",
                self.content_height,
                self.visible_height,
                self.content_height > self.visible_height
            );
            if self.content_height > self.visible_height {
                let scrollbar_elements = self.render_scrollbar(modal_bounds, fonts);
                elements.extend(scrollbar_elements);
            }
        }

        elements
    }

    fn calculate_modal_bounds(
        &self,
        size: ModalSize,
        sidebar_bounds: RectF,
        window_bounds: RectF,
    ) -> RectF {
        match size {
            ModalSize::FillSidebar => {
                let padding = 16.0;
                euclid::rect(
                    sidebar_bounds.min_x() + padding,
                    sidebar_bounds.min_y() + 40.0,
                    sidebar_bounds.width() - 2.0 * padding,
                    sidebar_bounds.height() - 80.0,
                )
            }
            ModalSize::HalfWindow => {
                let width = window_bounds.width() / 2.0;
                let height = window_bounds.height() - 160.0;
                let x = window_bounds.max_x() - width; // Right-aligned
                let y = (window_bounds.height() - height) / 2.0;
                euclid::rect(x, y, width, height)
            }
            ModalSize::Fixed(width, height) => {
                let x = sidebar_bounds.min_x() + (sidebar_bounds.width() - width) / 2.0;
                let y = sidebar_bounds.min_y() + (sidebar_bounds.height() - height) / 2.0;
                euclid::rect(x, y, width, height)
            }
        }
    }

    fn render_scrollbar(&self, modal_bounds: RectF, fonts: &SidebarFonts) -> Vec<Element> {
        let mut elements = vec![];

        let scrollbar_width = 8.0;
        let scrollbar_padding = 4.0;
        let scrollbar_x = modal_bounds.max_x() - scrollbar_width - scrollbar_padding;
        let scrollbar_height = self.visible_height;
        let scrollbar_y = modal_bounds.min_y() + 40.0;

        log::debug!(
            "Modal scrollbar positioning: modal_bounds=({}, {}, {}, {}), scrollbar_x={}, scrollbar_y={}",
            modal_bounds.min_x(),
            modal_bounds.min_y(),
            modal_bounds.max_x(),
            modal_bounds.max_y(),
            scrollbar_x,
            scrollbar_y
        );

        // Calculate thumb size and position
        let thumb_height = (self.visible_height / self.content_height) * scrollbar_height;
        let thumb_height = thumb_height.max(30.0).min(scrollbar_height); // Minimum thumb size

        let max_scroll = (self.content_height - self.visible_height).max(0.0);
        let thumb_y = if max_scroll > 0.0 {
            scrollbar_y + (self.scroll_offset / max_scroll) * (scrollbar_height - thumb_height)
        } else {
            scrollbar_y
        };

        // Scrollbar track background
        let track_opacity = if self.hovering_scrollbar { 0.4 } else { 0.2 };

        // Create a container at the scrollbar position
        let track_container = Element::new(
            &fonts.body,
            ElementContent::Children(vec![
                // Track background
                Element::new(&fonts.body, ElementContent::Text(String::new()))
                    .colors(ElementColors {
                        border: BorderColor::default(),
                        bg: LinearRgba(0.2, 0.2, 0.2, track_opacity).into(),
                        text: LinearRgba(1.0, 1.0, 1.0, 1.0).into(),
                    })
                    .min_width(Some(Dimension::Pixels(scrollbar_width)))
                    .min_height(Some(Dimension::Pixels(scrollbar_height)))
                    .display(DisplayType::Block),
            ]),
        )
        .margin(BoxDimension {
            left: Dimension::Pixels(scrollbar_x),
            top: Dimension::Pixels(scrollbar_y),
            right: Dimension::Pixels(0.0),
            bottom: Dimension::Pixels(0.0),
        })
        .zindex(23);

        elements.push(track_container);

        // Scrollbar thumb
        let thumb_opacity = if self.dragging_scrollbar {
            0.9
        } else if self.hovering_scrollbar {
            0.8
        } else {
            0.6
        };

        let thumb_element = Element::new(&fonts.body, ElementContent::Text(String::new()))
            .colors(ElementColors {
                border: BorderColor::default(),
                bg: LinearRgba(0.4, 0.4, 0.4, thumb_opacity).into(),
                text: LinearRgba(1.0, 1.0, 1.0, 1.0).into(),
            })
            .min_width(Some(Dimension::Pixels(scrollbar_width)))
            .min_height(Some(Dimension::Pixels(thumb_height)))
            .margin(BoxDimension {
                left: Dimension::Pixels(scrollbar_x),
                top: Dimension::Pixels(thumb_y),
                right: Dimension::Pixels(0.0),
                bottom: Dimension::Pixels(0.0),
            })
            .display(DisplayType::Block)
            .zindex(23);

        elements.push(thumb_element);

        elements
    }

    pub fn handle_mouse_event(
        &mut self,
        event: &window::MouseEvent,
        sidebar_bounds: RectF,
    ) -> bool {
        use window::{MouseEventKind as WMEK, MousePress};

        if let Some(ref modal) = self.active_modal {
            if let Some(modal_bounds) = modal.position {
                let point = euclid::point2(event.coords.x as f32, event.coords.y as f32);

                // Calculate scrollbar bounds if needed
                let scrollbar_visible = self.content_height > self.visible_height;
                let scrollbar_bounds = if scrollbar_visible {
                    let scrollbar_width = 8.0;
                    let scrollbar_padding = 4.0;
                    let scrollbar_x = modal_bounds.max_x() - scrollbar_width - scrollbar_padding;
                    let scrollbar_y = modal_bounds.min_y() + 40.0;
                    Some(euclid::rect(
                        scrollbar_x,
                        scrollbar_y,
                        scrollbar_width,
                        self.visible_height,
                    ))
                } else {
                    None
                };

                match &event.kind {
                    WMEK::Move => {
                        // Check if hovering scrollbar
                        if let Some(bounds) = scrollbar_bounds {
                            self.hovering_scrollbar = bounds.contains(point);
                        }

                        // Handle scrollbar dragging
                        if self.dragging_scrollbar {
                            if let (Some(drag_start_y), Some(drag_start_offset)) =
                                (self.drag_start_y, self.drag_start_offset)
                            {
                                let delta_y = event.coords.y as f32 - drag_start_y;
                                let scrollbar_y = modal_bounds.min_y() + 40.0;
                                let thumb_height = (self.visible_height / self.content_height)
                                    * self.visible_height;
                                let thumb_height = thumb_height.max(30.0).min(self.visible_height);
                                let max_thumb_travel = self.visible_height - thumb_height;

                                if max_thumb_travel > 0.0 {
                                    let thumb_position =
                                        (delta_y / max_thumb_travel).clamp(0.0, 1.0);
                                    let max_scroll =
                                        (self.content_height - self.visible_height).max(0.0);
                                    self.scroll_offset = thumb_position * max_scroll;
                                }
                            }
                            return true;
                        }
                    }
                    WMEK::Press(MousePress::Left) => {
                        // Check if clicking on scrollbar
                        if let Some(bounds) = scrollbar_bounds {
                            if bounds.contains(point) {
                                // Start scrollbar drag
                                self.dragging_scrollbar = true;
                                self.drag_start_y = Some(event.coords.y as f32);
                                self.drag_start_offset = Some(self.scroll_offset);
                                return true;
                            }
                        }

                        // Check close button (updated to match new size)
                        let close_button_bounds = euclid::rect(
                            modal_bounds.max_x() - 40.0, // 32 + 8 padding
                            modal_bounds.min_y() + 8.0,
                            32.0,
                            32.0,
                        );

                        if close_button_bounds.contains(point) {
                            self.close();
                            return true;
                        }

                        // Check click outside
                        if modal.close_on_click_outside && !modal_bounds.contains(point) {
                            self.close();
                            return true;
                        }
                    }
                    WMEK::Release(MousePress::Left) => {
                        // End scrollbar drag
                        if self.dragging_scrollbar {
                            self.dragging_scrollbar = false;
                            self.drag_start_y = None;
                            self.drag_start_offset = None;
                            return true;
                        }
                    }
                    WMEK::VertWheel(delta) => {
                        // Only scroll if mouse is over modal
                        if modal_bounds.contains(point) && self.content_height > self.visible_height
                        {
                            let scroll_amount = *delta as f32 * 30.0; // Increased scroll speed
                            self.scroll_offset -= scroll_amount;

                            // Clamp scroll offset
                            let max_scroll = (self.content_height - self.visible_height).max(0.0);
                            self.scroll_offset = self.scroll_offset.clamp(0.0, max_scroll);

                            return true;
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }

    pub fn handle_key_event(&mut self, key: KeyCode, mods: KeyModifiers) -> bool {
        if let Some(ref modal) = self.active_modal {
            if modal.close_on_escape && key == KeyCode::Escape && mods.is_empty() {
                self.close();
                return true;
            }
        }
        false
    }

    pub fn get_scroll_offset(&self) -> f32 {
        self.scroll_offset
    }

    pub fn get_scroll_info(&self) -> (f32, f32, f32) {
        (self.scroll_offset, self.content_height, self.visible_height)
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0.0;
        self.hovering_scrollbar = false;
        self.dragging_scrollbar = false;
        self.drag_start_y = None;
        self.drag_start_offset = None;
    }
}
