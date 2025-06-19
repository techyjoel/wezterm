//! Simplified pre-rendered glow effect texture cache
//!
//! This module provides a caching system for pre-rendered glow effects to improve
//! performance over real-time multi-pass rendering.

use crate::utilsprites::RenderMetrics;
use anyhow::Result;
use config::ConfigHandle;
use lfucache::LfuCache;
use std::rc::Rc;
use wezterm_font::{LoadedFont, RasterizedGlyph};
use window::bitmaps::atlas::{Atlas, Sprite};
use window::bitmaps::{BitmapImage, Image};
use window::color::{LinearRgba, SrgbaPixel};

/// Key for caching glow textures
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct GlowKey {
    content: String,
    color_key: [u8; 8], // Quantized color values
    size_key: (u16, u16),
    params: u32, // Packed parameters
}

/// Cache for pre-rendered glow textures
pub struct GlowCache {
    atlas: Atlas,
    cache: LfuCache<GlowKey, Sprite>,
}

impl GlowCache {
    /// Create a new glow cache
    pub fn new(
        atlas: Atlas,
        _metrics: &RenderMetrics,
        _max_entries: usize,
        config: &ConfigHandle,
    ) -> Self {
        Self {
            atlas,
            cache: LfuCache::new("glow_hit", "glow_miss", |_| 256, config),
        }
    }

    /// Get or create a glow texture for text/icon
    pub fn get_or_create_text_glow(
        &mut self,
        text: &str,
        font: &Rc<LoadedFont>,
        base_color: LinearRgba,
        glow_color: LinearRgba,
        glow_radius: f32,
        glow_layers: u8,
        glow_intensity: f64,
    ) -> Result<Sprite> {
        // Create simplified cache key
        let key = GlowKey {
            content: text.to_string(),
            color_key: [
                (base_color.0 * 255.0) as u8,
                (base_color.1 * 255.0) as u8,
                (base_color.2 * 255.0) as u8,
                (base_color.3 * 255.0) as u8,
                (glow_color.0 * 255.0) as u8,
                (glow_color.1 * 255.0) as u8,
                (glow_color.2 * 255.0) as u8,
                (glow_color.3 * 255.0) as u8,
            ],
            size_key: (40, 40), // Fixed size for now
            params: ((glow_radius as u8) as u32) << 16
                | ((glow_layers as u32) << 8)
                | ((glow_intensity * 255.0) as u8) as u32,
        };

        // Check cache
        if let Some(sprite) = self.cache.get(&key) {
            log::debug!("Glow texture cache hit for '{}'", text);
            return Ok(sprite.clone());
        }

        log::debug!(
            "Creating glow texture for '{}' with radius={}, layers={}, intensity={}",
            text,
            glow_radius,
            glow_layers,
            glow_intensity
        );

        // Render the actual glyph with glow effect
        let image = Self::render_glyph_with_glow(
            text,
            &font,
            base_color,
            glow_color,
            glow_radius,
            glow_layers,
            glow_intensity,
        )?;

        let (width, height) = image.image_dimensions();
        log::debug!("Glow texture created with dimensions: {}x{}", width, height);

        // Allocate in atlas
        let sprite = self.atlas.allocate(&image)?;

        log::debug!(
            "Glow texture allocated in atlas at coords: {:?}",
            sprite.texture_coords()
        );

        // Store in cache
        self.cache.put(key, sprite.clone());

        Ok(sprite)
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// Render a glyph with glow effect
    fn render_glyph_with_glow(
        text: &str,
        font: &Rc<LoadedFont>,
        base_color: LinearRgba,
        glow_color: LinearRgba,
        glow_radius: f32,
        glow_layers: u8,
        glow_intensity: f64,
    ) -> Result<Image> {
        // First, shape the text to get glyph information
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
            // No glyphs to render
            return Ok(Image::new(1, 1));
        }

        // Rasterize the first glyph (for icon fonts, there's usually just one)
        let info = &infos[0];
        if info.glyph_pos == 0 {
            // Skip if no valid glyph
            log::warn!("No valid glyph found for text '{}'", text);
            return Ok(Image::new(1, 1));
        }

        log::debug!(
            "Rasterizing glyph for '{}' at position {} with font index {}",
            text,
            info.glyph_pos,
            info.font_idx
        );

        let glyph = font.rasterize_glyph(info.glyph_pos, info.font_idx)?;

        log::debug!(
            "Rasterized glyph dimensions: {}x{}, has_data: {}",
            glyph.width,
            glyph.height,
            !glyph.data.is_empty()
        );

        // Calculate image size with padding for glow
        let padding = (glow_radius * 2.0).ceil() as usize;
        let image_width = glyph.width + padding * 2;
        let image_height = glyph.height + padding * 2;

        let mut final_image = Image::new(image_width, image_height);

        // Center position for the main glyph
        let center_x = padding;
        let center_y = padding;

        // Render glow layers
        if glow_layers > 0 && glow_intensity > 0.0 {
            // We'll render the glyph multiple times with offset positions
            let base_alpha = 0.08 * glow_intensity; // Keep the 8% brightness

            for layer in 1..=glow_layers {
                let layer_radius = (layer as f32 / glow_layers as f32) * glow_radius;
                let layer_alpha = base_alpha * (1.0 - (layer as f64 - 1.0) / glow_layers as f64);

                // Simplified: just render at cardinal directions for now
                let offsets = [
                    (layer_radius, 0.0),
                    (-layer_radius, 0.0),
                    (0.0, layer_radius),
                    (0.0, -layer_radius),
                    (layer_radius * 0.707, layer_radius * 0.707),
                    (-layer_radius * 0.707, layer_radius * 0.707),
                    (layer_radius * 0.707, -layer_radius * 0.707),
                    (-layer_radius * 0.707, -layer_radius * 0.707),
                ];

                for (dx, dy) in &offsets {
                    Self::blit_glyph(
                        &mut final_image,
                        &glyph,
                        (center_x as f32 + dx) as isize,
                        (center_y as f32 + dy) as isize,
                        glow_color,
                        layer_alpha as f32,
                    );
                }
            }
        }

        // Render the main glyph on top
        Self::blit_glyph(
            &mut final_image,
            &glyph,
            center_x as isize,
            center_y as isize,
            base_color,
            1.0,
        );

        Ok(final_image)
    }

    /// Blit a glyph onto an image with color and alpha
    fn blit_glyph(
        image: &mut Image,
        glyph: &RasterizedGlyph,
        x: isize,
        y: isize,
        color: LinearRgba,
        alpha_multiplier: f32,
    ) {
        let color_srgb = Self::linear_to_srgb_u8(color);

        for gy in 0..glyph.height {
            for gx in 0..glyph.width {
                let px = x + gx as isize;
                let py = y + gy as isize;

                if px >= 0
                    && (px as usize) < image.image_dimensions().0
                    && py >= 0
                    && (py as usize) < image.image_dimensions().1
                {
                    let glyph_alpha = glyph.data[gy * glyph.width + gx] as f32 / 255.0;
                    let final_alpha = glyph_alpha * color.3 * alpha_multiplier;

                    if final_alpha > 0.0 {
                        // Get existing pixel
                        let (width, _) = image.image_dimensions();
                        let idx = py as usize * width + px as usize;
                        let pixel_data = image.pixels_mut();
                        let existing = pixel_data[idx];

                        // Decode existing pixel
                        let existing_pixel = SrgbaPixel::with_srgba_u32(existing);
                        let (er, eg, eb, ea) = existing_pixel.as_rgba();

                        // Alpha blend
                        let inv_alpha = 1.0 - final_alpha;
                        let new_r =
                            (color_srgb.0 as f32 * final_alpha + er as f32 * inv_alpha) as u8;
                        let new_g =
                            (color_srgb.1 as f32 * final_alpha + eg as f32 * inv_alpha) as u8;
                        let new_b =
                            (color_srgb.2 as f32 * final_alpha + eb as f32 * inv_alpha) as u8;
                        let new_a = ((final_alpha + ea as f32 / 255.0 * inv_alpha) * 255.0) as u8;

                        pixel_data[idx] = SrgbaPixel::rgba(new_r, new_g, new_b, new_a).as_srgba32();
                    }
                }
            }
        }
    }

    /// Convert linear RGB to sRGB
    fn linear_to_srgb_u8(color: LinearRgba) -> (u8, u8, u8) {
        fn linear_to_srgb(linear: f32) -> u8 {
            let srgb = if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            };
            (srgb.clamp(0.0, 1.0) * 255.0) as u8
        }

        (
            linear_to_srgb(color.0),
            linear_to_srgb(color.1),
            linear_to_srgb(color.2),
        )
    }
}
