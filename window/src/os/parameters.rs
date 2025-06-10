use wezterm_color_types::LinearRgba;
use wezterm_font::parser::ParsedFont;

use crate::ULength;

pub type FontAndSize = (ParsedFont, f64);

#[derive(Default, Clone, Debug)]
pub struct TitleBar {
    pub padding_left: ULength,
    pub padding_right: ULength,
    pub height: Option<ULength>,
    pub font_and_size: Option<FontAndSize>,
}

#[derive(Default, Clone, Debug)]
pub struct Border {
    pub top: ULength,
    pub left: ULength,
    pub bottom: ULength,
    pub right: ULength,
    pub color: LinearRgba,
}

#[derive(Debug, Clone)]
pub struct OsBorderStyle {
    pub width: f32,
    pub color: (f32, f32, f32, f32), // RGBA
    pub radius: f32,
}

#[derive(Default, Clone, Debug)]
pub struct Parameters {
    pub title_bar: TitleBar,
    /// If present, the application should draw it
    pub border_dimensions: Option<Border>,
    /// OS-level window border style
    pub os_border_style: Option<OsBorderStyle>,
}
