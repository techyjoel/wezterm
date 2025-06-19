use crate::renderstate::RenderContext;
use crate::termwindow::TermWindow;
use anyhow::Result;
use std::rc::Rc;
use wezterm_font::LoadedFont;
use window::bitmaps::{BitmapImage, Image, Texture2d};
use window::color::LinearRgba;

impl TermWindow {
    /// Create a texture containing a rasterized icon/glyph for GPU blur processing
    pub fn create_icon_texture(
        &mut self,
        text: &str,
        font: &Rc<LoadedFont>,
        color: LinearRgba,
        icon_size: u32,
        padding: u32,
    ) -> Result<Rc<dyn Texture2d>> {
        log::debug!(
            "create_icon_texture called for '{}', icon_size={}, padding={}, color={:?}",
            text, icon_size, padding, color
        );
        // Get render state
        let render_state = self
            .render_state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No render state available"))?;

        // Shape the text to get glyph information
        let infos = font.shape(
            text,
            || {},
            |_| {},
            None,
            wezterm_font::shaper::Direction::LeftToRight,
            None,
            None,
        )?;

        if infos.is_empty() {
            anyhow::bail!("No glyphs found for text '{}'", text);
        }

        // Rasterize the glyph
        let info = &infos[0];
        let glyph = font.rasterize_glyph(info.glyph_pos, info.font_idx)?;

        // Calculate texture size with padding
        let texture_size = icon_size + padding * 2;

        // Create an image with the glyph centered
        let mut image = Image::new(texture_size as usize, texture_size as usize);

        // Calculate centering offsets
        let glyph_width = glyph.width.min(icon_size as usize);
        let glyph_height = glyph.height.min(icon_size as usize);
        let x_offset = ((texture_size as usize - glyph_width) / 2) as isize;
        let y_offset = ((texture_size as usize - glyph_height) / 2) as isize;

        // Copy glyph data to the image with the specified color
        for y in 0..glyph_height {
            for x in 0..glyph_width {
                let src_idx = y * glyph.width + x;
                if src_idx < glyph.data.len() {
                    let alpha = glyph.data[src_idx] as f32 / 255.0;
                    if alpha > 0.0 {
                        let dst_x = (x as isize + x_offset) as usize;
                        let dst_y = (y as isize + y_offset) as usize;

                        if dst_x < texture_size as usize && dst_y < texture_size as usize {
                            let dst_idx = (dst_y * texture_size as usize + dst_x) * 4;
                            // Get mutable data pointer
                            unsafe {
                                let data = image.pixel_data_mut();
                                // Premultiply alpha (BGRA format)
                                *data.add(dst_idx + 0) = (color.2 * alpha * 255.0) as u8; // B
                                *data.add(dst_idx + 1) = (color.1 * alpha * 255.0) as u8; // G
                                *data.add(dst_idx + 2) = (color.0 * alpha * 255.0) as u8; // R
                                *data.add(dst_idx + 3) = (alpha * 255.0) as u8; // A
                            }
                        }
                    }
                }
            }
        }

        // Create texture from the image
        let texture = render_state
            .context
            .allocate_render_target(texture_size as usize, texture_size as usize)?;

        // Upload the image data to the texture
        texture.write(
            window::Rect::new(
                window::Point::new(0, 0),
                window::Size::new(texture_size as isize, texture_size as isize),
            ),
            &image,
        );

        Ok(texture)
    }
}
