use crate::renderstate::RenderContext;
use crate::termwindow::render::blur::{BlurCacheKey, BlurRenderer};
use anyhow::Result;
use std::rc::Rc;
use window::bitmaps::atlas::{Atlas, Sprite};
use window::bitmaps::{Image, Texture2d};

/// Extension to BlurRenderer for atlas integration
impl BlurRenderer {
    /// Apply blur and store result in atlas, returning sprite coordinates
    pub fn apply_blur_to_atlas(
        &mut self,
        source_image: &Image,
        width: usize,
        height: usize,
        radius: f32,
        cache_key: BlurCacheKey,
        context: &RenderContext,
        atlas: &mut Atlas,
    ) -> Result<Sprite> {
        // Check sprite cache first
        // TODO: Add sprite caching to avoid re-adding to atlas

        // For now, always create fresh
        // Apply blur using existing GPU pipeline

        // Create texture from source image
        let source_texture = context.allocate_render_target(width, height)?;
        source_texture.write(
            window::Rect::new(
                window::Point::new(0, 0),
                window::Size::new(width as isize, height as isize),
            ),
            source_image,
        );

        // Apply GPU blur
        let blurred_texture =
            self.apply_blur(&*source_texture, radius, Some(cache_key), context)?;

        // Create result image
        let mut result_image = Image::new(width, height);

        // Copy blurred texture to image
        // Note: This requires GPU->CPU transfer which has overhead
        // but still much faster than 240-pass CPU rendering
        self.copy_texture_to_image(&*blurred_texture, &mut result_image, context)?;

        // Add to atlas and return sprite
        let sprite = atlas.allocate(&result_image)?;
        Ok(sprite)
    }

    /// Copy GPU texture to CPU image (placeholder - needs WebGPU implementation)
    fn copy_texture_to_image(
        &self,
        texture: &dyn Texture2d,
        image: &mut Image,
        context: &RenderContext,
    ) -> Result<()> {
        // TODO: Implement GPU->CPU copy
        // This would use wgpu's CommandEncoder::copy_texture_to_buffer
        // For now, return error to indicate not implemented
        anyhow::bail!("GPU to CPU texture copy not yet implemented")
    }
}
