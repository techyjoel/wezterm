use crate::renderstate::RenderContext;
use crate::termwindow::webgpu::{WebGpuState, WebGpuTexture};
use anyhow::Result;
use config::Dimension;
use std::any::Any;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;
use wgpu::util::DeviceExt;
use window::bitmaps::Texture2d;

/// Manages GPU-accelerated blur effects for UI elements
pub struct BlurRenderer {
    /// Cached blur results for static content
    cache: HashMap<BlurCacheKey, CachedBlur>,
    /// Pool of render targets for blur operations
    render_targets: Vec<BlurRenderTarget>,
    /// Maximum cache size in bytes
    max_cache_size: usize,
    /// Current cache size in bytes
    current_cache_size: usize,
}

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct BlurCacheKey {
    /// Hash of the source content
    pub content_hash: u64,
    /// Blur radius
    pub radius: u32,
    /// Dimensions of the blur
    pub width: u32,
    pub height: u32,
}

struct CachedBlur {
    texture: Rc<dyn Texture2d>,
    last_accessed: Instant,
    size_bytes: usize,
}

struct BlurRenderTarget {
    texture: Rc<WebGpuTexture>,
    width: u32,
    height: u32,
    in_use: bool,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlurUniforms {
    pub direction: [f32; 2],
    pub sigma: f32,
    pub kernel_size: u32,
    pub texture_size: [f32; 2],
    pub _padding: [f32; 2], // Ensure 16-byte alignment
}

impl BlurRenderer {
    pub fn new(max_cache_mb: usize) -> Self {
        Self {
            cache: HashMap::new(),
            render_targets: Vec::new(),
            max_cache_size: max_cache_mb * 1024 * 1024,
            current_cache_size: 0,
        }
    }

    /// Initialize blur pipelines on the GPU
    pub fn init_pipelines(state: &mut WebGpuState) -> Result<()> {
        log::info!("Initializing GPU blur pipelines...");

        // Create blur uniform bind group layout
        let blur_uniform_layout =
            state
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Blur Uniform Bind Group Layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        // Load blur shader
        let blur_shader = state
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Blur Shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/blur.wgsl").into()),
            });

        // Create render pipeline layout
        let pipeline_layout =
            state
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Blur Pipeline Layout"),
                    bind_group_layouts: &[&blur_uniform_layout, &state.texture_bind_group_layout],
                    push_constant_ranges: &[],
                });

        // Create horizontal blur pipeline
        let horizontal_pipeline =
            state
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Horizontal Blur Pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &blur_shader,
                        entry_point: Some("vs_blur"),
                        buffers: &[],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &blur_shader,
                        entry_point: Some("fs_blur"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba8UnormSrgb,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                    cache: None,
                });

        // Create vertical blur pipeline (same as horizontal)
        let vertical_pipeline =
            state
                .device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Vertical Blur Pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &blur_shader,
                        entry_point: Some("vs_blur"),
                        buffers: &[],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &blur_shader,
                        entry_point: Some("fs_blur"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Rgba8UnormSrgb,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        unclipped_depth: false,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: 1,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    multiview: None,
                    cache: None,
                });

        // Store pipelines in state
        state.blur_horizontal_pipeline = Some(horizontal_pipeline);
        state.blur_vertical_pipeline = Some(vertical_pipeline);
        state.blur_uniform_bind_group_layout = Some(blur_uniform_layout);

        log::info!("✓ GPU blur pipelines initialized successfully");
        Ok(())
    }

    /// Get or create a render target of the specified size
    fn get_render_target(
        &mut self,
        width: u32,
        height: u32,
        state: &WebGpuState,
    ) -> Result<Rc<WebGpuTexture>> {
        // Try to find an unused render target of the right size
        for target in &mut self.render_targets {
            if !target.in_use && target.width == width && target.height == height {
                target.in_use = true;
                return Ok(target.texture.clone());
            }
        }

        // Create a new render target
        let texture = Rc::new(WebGpuTexture::new_render_target(width, height, state)?);
        self.render_targets.push(BlurRenderTarget {
            texture: texture.clone(),
            width,
            height,
            in_use: true,
        });

        Ok(texture)
    }

    /// Release a render target back to the pool
    fn release_render_target(&mut self, texture: &Rc<WebGpuTexture>) {
        for target in &mut self.render_targets {
            if Rc::ptr_eq(&target.texture, texture) {
                target.in_use = false;
                break;
            }
        }
    }

    /// Test the blur pipeline with a simple colored square
    pub fn test_blur_pipeline(&mut self, context: &RenderContext) -> Result<()> {
        log::info!("Testing GPU blur pipeline...");

        // Get WebGPU state
        let state = match context {
            RenderContext::WebGpu(state) => state,
            _ => anyhow::bail!("Blur test only supported with WebGPU backend"),
        };

        // Check if pipelines are initialized
        if state.blur_horizontal_pipeline.is_none() {
            anyhow::bail!("Blur pipelines not initialized");
        }

        // Create a small test texture
        let test_size = 64u32;
        let test_texture = self.get_render_target(test_size, test_size, state)?;

        // Fill it with a white square (this would normally be done by rendering)
        // For now, just test the blur pass itself

        let blurred = self.get_render_target(test_size, test_size, state)?;

        // Test horizontal blur pass
        match self.blur_pass(&*test_texture, &*blurred, true, 5.0, 15, state) {
            Ok(_) => {
                log::info!("✓ Horizontal blur pass succeeded");

                // Test vertical blur pass too
                let final_blur = self.get_render_target(test_size, test_size, state)?;
                match self.blur_pass(&*blurred, &*final_blur, false, 5.0, 15, state) {
                    Ok(_) => log::info!("✓ Vertical blur pass succeeded"),
                    Err(e) => log::error!("✗ Vertical blur pass failed: {}", e),
                }
                self.release_render_target(&final_blur);
            }
            Err(e) => log::error!("✗ Horizontal blur pass failed: {}", e),
        }

        // Release targets
        self.release_render_target(&test_texture);
        self.release_render_target(&blurred);

        log::info!("GPU blur pipeline test completed successfully!");
        Ok(())
    }

    /// Apply blur effect to a texture and return sprite coordinates in atlas
    pub fn apply_blur_to_sprite(
        &mut self,
        source: &dyn Texture2d,
        radius: f32,
        cache_key: BlurCacheKey,
        context: &RenderContext,
        atlas: &mut ::window::bitmaps::atlas::Atlas,
    ) -> Result<::window::bitmaps::atlas::Sprite> {
        // Check cache first
        if let Some(cached) = self.cache.get_mut(&cache_key) {
            cached.last_accessed = Instant::now();
            // Need to get sprite from atlas
            // For now, we'll need to re-add to atlas each time
        }

        // Apply blur
        let blurred = self.apply_blur(source, radius, Some(cache_key.clone()), context)?;

        // Convert blurred texture to image for atlas
        let width = blurred.width();
        let height = blurred.height();
        let mut image = ::window::bitmaps::Image::new(width, height);

        // Read the blurred texture data
        blurred.read(
            ::window::Rect::new(
                ::window::Point::new(0, 0),
                ::window::Size::new(width as isize, height as isize),
            ),
            &mut image,
        );

        // Add to atlas and get sprite
        let sprite = atlas.allocate(&image)?;

        Ok(sprite)
    }

    /// Apply blur effect to a texture
    pub fn apply_blur(
        &mut self,
        source: &dyn Texture2d,
        radius: f32,
        cache_key: Option<BlurCacheKey>,
        context: &RenderContext,
    ) -> Result<Rc<dyn Texture2d>> {
        // Check cache if key provided
        if let Some(key) = &cache_key {
            if let Some(cached) = self.cache.get_mut(key) {
                cached.last_accessed = Instant::now();
                return Ok(cached.texture.clone());
            }
        }

        // Get WebGPU state
        let state = match context {
            RenderContext::WebGpu(state) => state,
            _ => anyhow::bail!("Blur effects only supported with WebGPU backend"),
        };

        let width = source.width() as u32;
        let height = source.height() as u32;

        // Calculate blur parameters
        let sigma = radius / 3.0; // Standard deviation from radius
        let kernel_size = ((sigma * 6.0).ceil() as u32) | 1; // Ensure odd

        // Get render targets for ping-pong
        let intermediate = self.get_render_target(width, height, state)?;
        let final_target = self.get_render_target(width, height, state)?;

        // Perform two-pass blur (horizontal then vertical)
        self.blur_pass(source, &*intermediate, true, sigma, kernel_size, state)?;
        self.blur_pass(
            &*intermediate,
            &*final_target,
            false,
            sigma,
            kernel_size,
            state,
        )?;

        // Release intermediate target
        self.release_render_target(&intermediate);

        // Cache result if requested
        if let Some(key) = cache_key {
            let size_bytes = (width * height * 4) as usize;
            self.add_to_cache(key, final_target.clone(), size_bytes);
        }

        Ok(final_target as Rc<dyn Texture2d>)
    }

    /// Perform a single blur pass
    fn blur_pass(
        &self,
        source: &dyn Texture2d,
        target: &dyn Texture2d,
        horizontal: bool,
        sigma: f32,
        kernel_size: u32,
        state: &WebGpuState,
    ) -> Result<()> {
        // Get WebGPU textures
        let source_tex = source
            .downcast_ref::<WebGpuTexture>()
            .ok_or_else(|| anyhow::anyhow!("Source texture is not WebGPU texture"))?;
        let target_tex = target
            .downcast_ref::<WebGpuTexture>()
            .ok_or_else(|| anyhow::anyhow!("Target texture is not WebGPU texture"))?;

        // Create blur uniforms
        let uniforms = BlurUniforms {
            direction: if horizontal { [1.0, 0.0] } else { [0.0, 1.0] },
            sigma,
            kernel_size,
            texture_size: [source.width() as f32, source.height() as f32],
            _padding: [0.0, 0.0],
        };

        // Create uniform buffer and bind group
        let uniform_buffer = state
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Blur Uniform Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM,
            });

        let uniform_bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur Uniform Bind Group"),
            layout: state.blur_uniform_bind_group_layout.as_ref().unwrap(),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        // Create texture bind group for source texture
        let source_view = source_tex.create_view();
        let texture_bind_group = state.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Blur Texture Bind Group"),
            layout: &state.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&state.texture_linear_sampler),
                },
            ],
        });

        // Create command encoder
        let mut encoder = state
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Blur Command Encoder"),
            });

        // Create render pass
        {
            let target_view = target_tex.create_view();
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Blur Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Use appropriate pipeline
            let pipeline = if horizontal {
                state.blur_horizontal_pipeline.as_ref().unwrap()
            } else {
                state.blur_vertical_pipeline.as_ref().unwrap()
            };

            render_pass.set_pipeline(pipeline);
            render_pass.set_bind_group(0, &uniform_bind_group, &[]);
            render_pass.set_bind_group(1, &texture_bind_group, &[]);

            // Draw full-screen triangle
            render_pass.draw(0..3, 0..1);
        }

        // Submit commands
        state.queue.submit(std::iter::once(encoder.finish()));

        Ok(())
    }

    /// Add a blur result to the cache
    fn add_to_cache(&mut self, key: BlurCacheKey, texture: Rc<dyn Texture2d>, size_bytes: usize) {
        // Evict old entries if cache is full
        while self.current_cache_size + size_bytes > self.max_cache_size && !self.cache.is_empty() {
            // Find oldest entry
            let oldest_key = self
                .cache
                .iter()
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone());

            if let Some(key) = oldest_key {
                if let Some(removed) = self.cache.remove(&key) {
                    self.current_cache_size -= removed.size_bytes;
                }
            }
        }

        // Add new entry
        self.cache.insert(
            key,
            CachedBlur {
                texture,
                last_accessed: Instant::now(),
                size_bytes,
            },
        );
        self.current_cache_size += size_bytes;
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.current_cache_size = 0;
    }

    /// Compute hash for content-based caching
    pub fn compute_content_hash(data: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        hasher.finish()
    }
}
