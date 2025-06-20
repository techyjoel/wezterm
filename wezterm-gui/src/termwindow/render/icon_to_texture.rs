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
        log::info!(
            "create_icon_texture called for '{}', icon_size={}, padding={}, color={:?}",
            text,
            icon_size,
            padding,
            color
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
        log::info!(
            "Shaping info: glyph_pos={}, font_idx={}, x_advance={:?}, y_advance={:?}, x_offset={:?}, y_offset={:?}",
            info.glyph_pos,
            info.font_idx,
            info.x_advance,
            info.y_advance,
            info.x_offset,
            info.y_offset
        );

        let glyph = font.rasterize_glyph(info.glyph_pos, info.font_idx)?;

        // Log font metrics to understand icon positioning
        let metrics = font.metrics();
        log::debug!(
            "Font metrics: cell_height={:?}, cell_width={:?}, descender={:?}, underline_position={:?}",
            metrics.cell_height,
            metrics.cell_width,
            metrics.descender,
            metrics.underline_position
        );

        log::info!(
            "Rasterized glyph dimensions: width={}, height={}, bearing_x={:?}, bearing_y={:?}, has_color={}, data_len={}",
            glyph.width,
            glyph.height,
            glyph.bearing_x,
            glyph.bearing_y,
            glyph.has_color,
            glyph.data.len()
        );

        // Get the actual glyph dimensions
        let glyph_width = glyph.width;
        let glyph_height = glyph.height;

        // Calculate texture size with padding
        // The padding parameter is the blur radius in pixels
        // For proper Gaussian blur, we need to account for the kernel size
        // Kernel radius = ceil(sigma * sqrt(2 * ln(255))) where sigma = radius / 3.33
        // This typically gives kernel_radius ≈ blur_radius
        // We need extra padding to ensure the blur doesn't get clipped
        let min_size = glyph_width.max(glyph_height) as u32;
        let blur_padding = padding; // This is the blur radius
                                    // Add blur_padding on each side
        let texture_size = min_size.max(icon_size) + (blur_padding * 2);

        log::info!(
            "Texture sizing: glyph {}x{}, icon_size {}, padding {}, blur_padding {}, final texture {}",
            glyph_width, glyph_height, icon_size, padding, blur_padding, texture_size
        );

        // Create an image with the glyph centered
        let mut image = Image::new(texture_size as usize, texture_size as usize);

        // Calculate center position for the glyph
        // We want to center the visual bounding box of the glyph within the texture
        // bearing_x is typically 0 or positive for most glyphs
        // bearing_y is the distance from baseline to top of glyph (positive = above baseline)

        // We'll calculate offsets after we scan for content bounds
        let x_offset: isize;
        let y_offset: isize;

        // Track actual pixel bounds written
        let mut min_x = texture_size as isize;
        let mut max_x = 0isize;
        let mut min_y = texture_size as isize;
        let mut max_y = 0isize;
        let mut pixels_written = 0;

        // Check if we need to scan for actual content bounds
        let mut actual_min_x = glyph_width;
        let mut actual_max_x = 0;
        let mut actual_min_y = glyph_height;
        let mut actual_max_y = 0;
        let mut has_content = false;

        // First pass: find actual content bounds
        // Note: glyph data is RGBA (4 bytes per pixel)
        for y in 0..glyph_height {
            for x in 0..glyph_width {
                let src_idx = (y * glyph.width + x) * 4; // RGBA: 4 bytes per pixel
                if src_idx + 3 < glyph.data.len() && glyph.data[src_idx + 3] > 0 {
                    // Check alpha channel
                    actual_min_x = actual_min_x.min(x);
                    actual_max_x = actual_max_x.max(x);
                    actual_min_y = actual_min_y.min(y);
                    actual_max_y = actual_max_y.max(y);
                    has_content = true;
                }
            }
        }

        if has_content {
            // Center the entire glyph bitmap in the texture
            // The glyph coordinates already include any bearing/offset information
            x_offset = (texture_size as isize - glyph_width as isize) / 2;
            y_offset = (texture_size as isize - glyph_height as isize) / 2;
        } else {
            // Fall back to centering the glyph bitmap
            x_offset = (texture_size as isize - glyph_width as isize) / 2;
            y_offset = (texture_size as isize - glyph_height as isize) / 2;
            log::debug!(
                "No content found, centering glyph: offsets ({},{})",
                x_offset,
                y_offset
            );
        }

        // Copy glyph data to the image with the specified color
        // Note: glyph data is RGBA (4 bytes per pixel)
        for y in 0..glyph_height {
            for x in 0..glyph_width {
                let src_idx = (y * glyph.width + x) * 4; // RGBA: 4 bytes per pixel
                if src_idx + 3 < glyph.data.len() {
                    // Use alpha channel (index 3) from RGBA data
                    let alpha = glyph.data[src_idx + 3] as f32 / 255.0;
                    if alpha > 0.0 {
                        let dst_x = x as isize + x_offset;
                        let dst_y = y as isize + y_offset;

                        // Only write pixels that are within bounds
                        if dst_x >= 0
                            && dst_x < texture_size as isize
                            && dst_y >= 0
                            && dst_y < texture_size as isize
                        {
                            let dst_idx =
                                ((dst_y as usize) * texture_size as usize + (dst_x as usize)) * 4;

                            // Track pixel bounds
                            min_x = min_x.min(dst_x);
                            max_x = max_x.max(dst_x);
                            min_y = min_y.min(dst_y);
                            max_y = max_y.max(dst_y);
                            pixels_written += 1;

                            // Get mutable data pointer
                            unsafe {
                                let data = image.pixel_data_mut();
                                // Write as RGBA format in linear space (no sRGB conversion)
                                *data.add(dst_idx + 0) = (color.0 * alpha * 255.0) as u8; // R
                                *data.add(dst_idx + 1) = (color.1 * alpha * 255.0) as u8; // G
                                *data.add(dst_idx + 2) = (color.2 * alpha * 255.0) as u8; // B
                                *data.add(dst_idx + 3) = (alpha * 255.0) as u8; // A
                            }
                        } else if alpha > 0.0 {
                            // Log when pixels would be clipped
                            log::trace!(
                                "Pixel clipped at glyph({}, {}) -> texture({}, {}), alpha={}",
                                x,
                                y,
                                dst_x,
                                dst_y,
                                alpha
                            );
                        }
                    }
                }
            }
        }

        // Log actual pixel bounds written
        if pixels_written > 0 {
            log::info!(
                "Pixels written: count={}, bounds=({}, {}) to ({}, {}), size={}x{}",
                pixels_written,
                min_x,
                min_y,
                max_x,
                max_y,
                max_x - min_x + 1,
                max_y - min_y + 1
            );
        } else {
            log::warn!("No pixels were written to texture!");
        }

        // Check if pixels are centered in texture
        let pixel_center_x = (min_x + max_x) / 2;
        let pixel_center_y = (min_y + max_y) / 2;
        let texture_center = texture_size as isize / 2;
        log::info!(
            "Pixel centering check: pixel_center=({}, {}), texture_center={}, offset=({}, {})",
            pixel_center_x,
            pixel_center_y,
            texture_center,
            pixel_center_x - texture_center,
            pixel_center_y - texture_center
        );

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

        // Debug: optionally save the texture to file for inspection
        if std::env::var("WEZTERM_DEBUG_ICON_TEXTURE").is_ok() {
            save_icon_debug_texture(&image, texture_size, text);
        }

        Ok(texture)
    }
}

/// Save a debug image of the texture for inspection
fn save_icon_debug_texture(image: &Image, size: u32, text: &str) {
    use std::fs::File;
    use std::io::Write;
    use window::bitmaps::BitmapImage;

    // Create a simple PPM file for debugging
    let filename = format!(
        "/tmp/wezterm_icon_{}_{}.ppm",
        text.chars().next().unwrap_or('?') as u32,
        size
    );

    if let Ok(mut file) = File::create(&filename) {
        // PPM header
        writeln!(file, "P6").ok();
        writeln!(file, "{} {}", size, size).ok();
        writeln!(file, "255").ok();

        // Write RGB data (convert from RGBA)
        let data =
            unsafe { std::slice::from_raw_parts(image.pixel_data(), (size * size * 4) as usize) };

        for y in 0..size {
            for x in 0..size {
                let idx = ((y * size + x) * 4) as usize;
                file.write_all(&[data[idx], data[idx + 1], data[idx + 2]])
                    .ok();
            }
        }

        log::info!("Saved debug icon texture to: {}", filename);
    }
}
