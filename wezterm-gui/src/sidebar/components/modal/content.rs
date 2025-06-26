use crate::sidebar::SidebarFonts;
use crate::termwindow::box_model::Element;
use window::RectF;
use termwiz::input::KeyCode;
use wezterm_term::KeyModifiers;

#[derive(Clone, Copy)]
pub struct ModalRenderContext<'a> {
    pub modal_bounds: RectF,
    pub fonts: &'a SidebarFonts,
    pub visible_height: f32,
    pub scroll_offset: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ModalEventResult {
    Handled,
    NotHandled,
    Close,
}

#[derive(Debug)]
pub enum ModalEvent {
    Mouse(window::MouseEvent),
    Key { key: KeyCode, mods: KeyModifiers },
}

pub trait ModalContent: Send + Sync {
    fn render(&self, context: &ModalRenderContext) -> Element;
    
    fn handle_event(&mut self, event: &ModalEvent) -> ModalEventResult {
        ModalEventResult::NotHandled
    }
    
    fn get_content_height(&self) -> f32;
    
    fn get_initial_focus(&self) -> Option<String> {
        None
    }
    
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}