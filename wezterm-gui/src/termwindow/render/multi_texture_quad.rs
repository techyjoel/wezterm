use crate::quad::{QuadTrait, TripleLayerQuadAllocator};
use window::bitmaps::TextureRect;
use window::color::LinearRgba;

/// Represents a quad that uses a custom texture instead of the atlas
pub struct CustomTextureQuad {
    pub texture_id: u32,  // ID to identify which texture to bind
    pub position: (f32, f32, f32, f32),  // x1, y1, x2, y2
    pub tex_coords: TextureRect,
    pub color: LinearRgba,
    pub z_index: i32,
}

/// Extended quad allocator that supports custom textures
pub trait ExtendedQuadAllocator {
    /// Allocate a quad that uses a custom texture
    fn allocate_custom_texture(&mut self, layer: usize, texture_id: u32) -> Result<CustomTextureQuad, anyhow::Error>;
}

/// Rendering system that can handle multiple textures
pub struct MultiTextureRenderer {
    /// Standard quads using the atlas
    pub atlas_quads: Vec<TripleLayerQuadAllocator>,
    
    /// Custom texture quads grouped by texture ID
    pub custom_quads: std::collections::HashMap<u32, Vec<CustomTextureQuad>>,
    
    /// Texture registry
    pub textures: std::collections::HashMap<u32, Box<dyn window::bitmaps::Texture2d>>,
    
    next_texture_id: u32,
}

impl MultiTextureRenderer {
    pub fn new() -> Self {
        Self {
            atlas_quads: Vec::new(),
            custom_quads: std::collections::HashMap::new(),
            textures: std::collections::HashMap::new(),
            next_texture_id: 1,
        }
    }
    
    /// Register a texture and get its ID
    pub fn register_texture(&mut self, texture: Box<dyn window::bitmaps::Texture2d>) -> u32 {
        let id = self.next_texture_id;
        self.next_texture_id += 1;
        self.textures.insert(id, texture);
        id
    }
    
    /// Add a quad using a custom texture
    pub fn add_custom_quad(&mut self, texture_id: u32, quad: CustomTextureQuad) {
        self.custom_quads
            .entry(texture_id)
            .or_insert_with(Vec::new)
            .push(quad);
    }
    
    /// Render all quads, switching textures as needed
    pub fn render(&self, render_state: &crate::termwindow::RenderState) -> Result<(), anyhow::Error> {
        // First render all atlas quads (existing behavior)
        // ...
        
        // Then render custom texture quads, batched by texture
        for (texture_id, quads) in &self.custom_quads {
            // Bind the custom texture
            // Render all quads using this texture
            // ...
        }
        
        Ok(())
    }
}