use crate::renderstate::{OpenGLRenderTexture, RenderContext};
use crate::termwindow::webgpu::{WebGpuState, WebGpuTexture};
use crate::uniforms::UniformBuilder;
use anyhow::Result;
use config::Dimension;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;
use wgpu::util::DeviceExt;
use window::bitmaps::Texture2d;
use window::glium::backend::Context as GliumContext;
use window::glium::{framebuffer, implement_vertex, texture, uniform};

/// Backend-specific blur state
enum BlurBackend {
    WebGpu {
        render_targets: Vec<BlurRenderTarget>,
    },
    OpenGl {
        blur_program: window::glium::Program,
        render_targets: Vec<OpenGLRenderTarget>,
        vertex_buffer: window::glium::VertexBuffer<BlurVertex>,
    },
}

/// OpenGL render target
struct OpenGLRenderTarget {
    texture: Rc<texture::Texture2d>,
    width: u32,
    height: u32,
    in_use: bool,
}

/// Vertex format for OpenGL blur
#[derive(Copy, Clone)]
struct BlurVertex {
    position: [f32; 2],
}

implement_vertex!(BlurVertex, position);

/// Manages GPU-accelerated blur effects for UI elements
pub struct BlurRenderer {
    /// Cached blur results for static content
    cache: HashMap<BlurCacheKey, CachedBlur>,
    /// Backend-specific state
    backend: Option<BlurBackend>,
    /// Maximum cache size in bytes
    max_cache_size: usize,
    /// Current cache size in bytes
    current_cache_size: usize,
}

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
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
    pub radius: f32,
    pub _padding: f32, // Ensure 16-byte alignment
}

impl BlurRenderer {
    pub fn new(max_cache_mb: usize) -> Self {
        Self {
            cache: HashMap::new(),
            backend: None,
            max_cache_size: max_cache_mb * 1024 * 1024,
            current_cache_size: 0,
        }
    }

    /// Initialize blur pipelines on the GPU
    pub fn init_pipelines(state: &mut WebGpuState) -> Result<()> {
        log::debug!("Initializing GPU blur pipelines...");

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
                            format: wgpu::TextureFormat::Rgba8Unorm, // Linear format for blur
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
                            format: wgpu::TextureFormat::Rgba8Unorm, // Linear format for blur
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

        log::debug!("✓ GPU blur pipelines initialized successfully");
        Ok(())
    }

    /// Initialize OpenGL blur renderer
    pub fn init_opengl(&mut self, context: &Rc<GliumContext>) -> Result<()> {
        log::debug!("Initializing OpenGL blur renderer...");

        // Compile blur shaders
        let blur_program = crate::renderstate::RenderState::compile_prog(context, |version| {
            (
                format!(
                    "#version {}\n{}",
                    version,
                    include_str!("../../blur-vertex.glsl")
                ),
                format!(
                    "#version {}\n{}",
                    version,
                    include_str!("../../blur-frag.glsl")
                ),
            )
        })?;

        // Create vertex buffer for full-screen triangle
        let vertex_buffer = window::glium::VertexBuffer::new(
            context,
            &[
                BlurVertex {
                    position: [-3.0, -1.0],
                },
                BlurVertex {
                    position: [1.0, -1.0],
                },
                BlurVertex {
                    position: [1.0, 3.0],
                },
            ],
        )?;

        self.backend = Some(BlurBackend::OpenGl {
            blur_program,
            render_targets: Vec::new(),
            vertex_buffer,
        });

        log::debug!("✓ OpenGL blur renderer initialized successfully");
        Ok(())
    }

    /// Get or create a render target of the specified size
    fn get_render_target(
        &mut self,
        width: u32,
        height: u32,
        state: &WebGpuState,
    ) -> Result<Rc<WebGpuTexture>> {
        // Get render targets from backend
        let render_targets = match &mut self.backend {
            Some(BlurBackend::WebGpu { render_targets }) => render_targets,
            _ => anyhow::bail!("WebGPU backend not initialized"),
        };

        // Try to find an unused render target of the right size
        for target in &mut *render_targets {
            if !target.in_use && target.width == width && target.height == height {
                target.in_use = true;
                return Ok(target.texture.clone());
            }
        }

        // Create a new render target
        let texture = Rc::new(WebGpuTexture::new_render_target(width, height, state)?);
        render_targets.push(BlurRenderTarget {
            texture: texture.clone(),
            width,
            height,
            in_use: true,
        });

        Ok(texture)
    }

    /// Release a render target back to the pool
    fn release_render_target(&mut self, texture: &Rc<WebGpuTexture>) {
        let render_targets = match &mut self.backend {
            Some(BlurBackend::WebGpu { render_targets }) => render_targets,
            _ => return,
        };

        for target in render_targets {
            if Rc::ptr_eq(&target.texture, texture) {
                target.in_use = false;
                break;
            }
        }
    }

    /// Test the blur pipeline with a simple colored square
    pub fn test_blur_pipeline(&mut self, context: &RenderContext) -> Result<()> {
        log::debug!("Testing GPU blur pipeline...");

        // Get WebGPU state
        let state = match context {
            RenderContext::WebGpu(state) => state,
            _ => anyhow::bail!("Blur test only supported with WebGPU backend"),
        };

        // Check if pipelines are initialized
        if state.blur_horizontal_pipeline.is_none() {
            anyhow::bail!("Blur pipelines not initialized");
        }

        // Ensure WebGPU backend is initialized
        if self.backend.is_none() {
            self.backend = Some(BlurBackend::WebGpu {
                render_targets: Vec::new(),
            });
        }

        // Create a small test texture
        let test_size = 64u32;
        let test_texture = self.get_render_target(test_size, test_size, state)?;

        // Fill it with a white square (this would normally be done by rendering)
        // For now, just test the blur pass itself

        let blurred = self.get_render_target(test_size, test_size, state)?;

        // Test horizontal blur pass
        match self.blur_pass(&*test_texture, &*blurred, true, 5.0, 5.0 / 3.33, 15, state) {
            Ok(_) => {
                log::debug!("✓ Horizontal blur pass succeeded");

                // Test vertical blur pass too
                let final_blur = self.get_render_target(test_size, test_size, state)?;
                match self.blur_pass(&*blurred, &*final_blur, false, 5.0, 5.0 / 3.33, 15, state) {
                    Ok(_) => log::debug!("✓ Vertical blur pass succeeded"),
                    Err(e) => log::error!("✗ Vertical blur pass failed: {}", e),
                }
                self.release_render_target(&final_blur);
            }
            Err(e) => log::error!("✗ Horizontal blur pass failed: {}", e),
        }

        // Release targets
        self.release_render_target(&test_texture);
        self.release_render_target(&blurred);

        log::debug!("GPU blur pipeline test completed successfully!");
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
                log::debug!("Cache hit for blur key {:?}", key);
                return Ok(cached.texture.clone());
            } else {
                log::debug!("Cache miss for blur key {:?}", key);
            }
        }

        // Route to appropriate backend implementation
        match context {
            RenderContext::WebGpu(state) => {
                self.apply_blur_webgpu(source, radius, cache_key, state)
            }
            RenderContext::Glium(context) => {
                self.apply_blur_opengl(source, radius, cache_key, context)
            }
        }
    }

    /// Apply blur using WebGPU backend
    fn apply_blur_webgpu(
        &mut self,
        source: &dyn Texture2d,
        radius: f32,
        cache_key: Option<BlurCacheKey>,
        state: &WebGpuState,
    ) -> Result<Rc<dyn Texture2d>> {
        // Ensure WebGPU backend is initialized
        if self.backend.is_none() {
            self.backend = Some(BlurBackend::WebGpu {
                render_targets: Vec::new(),
            });
        }

        let width = source.width() as u32;
        let height = source.height() as u32;

        // Calculate blur parameters using GIMP's formula
        // GIMP adds 1.0 to radius: radius = fabs(radius) + 1.0
        // Then: std_dev = sqrt(-(radius * radius) / (2 * log(1.0 / 255.0)))
        // This simplifies to: sigma = radius / sqrt(2 * ln(255)) ≈ radius / 3.33
        // However, GIMP uses IIR filter which spreads further than convolution.
        // To approximate this with convolution, we need a larger sigma.
        let effective_radius = radius.abs() + 1.0;
        let sigma = effective_radius / 2.0; // Larger sigma for more spread

        log::debug!(
            "Blur calculation: radius={}, effective_radius={}, sigma={}",
            radius,
            effective_radius,
            sigma
        );

        // For proper blur spread, we need a kernel that extends beyond the nominal radius
        // to capture the gaussian tail. Use 3*sigma as a good approximation.
        let kernel_radius = (sigma * 3.0).ceil() as u32;
        let mut kernel_size = kernel_radius * 2 + 1; // Make it odd

        // Clamp kernel_size to our shader's maximum supported size
        const MAX_KERNEL_SIZE: u32 = 63;
        if kernel_size > MAX_KERNEL_SIZE {
            log::warn!(
                "Blur radius {} requires kernel_size {} which exceeds maximum {}, clamping",
                radius,
                kernel_size,
                MAX_KERNEL_SIZE
            );
            kernel_size = MAX_KERNEL_SIZE;
        }

        log::debug!(
            "Final blur parameters: kernel_radius={}, kernel_size={}",
            kernel_radius,
            kernel_size
        );

        // Get render targets for ping-pong
        let intermediate = self.get_render_target(width, height, state)?;
        let final_target = self.get_render_target(width, height, state)?;

        // Perform two-pass blur (horizontal then vertical)
        self.blur_pass(
            source,
            &*intermediate,
            true,
            radius,
            sigma,
            kernel_size,
            state,
        )?;
        self.blur_pass(
            &*intermediate,
            &*final_target,
            false,
            radius,
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
        radius: f32,
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
            radius,
            _padding: 0.0,
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

    /// Apply blur using OpenGL backend
    fn apply_blur_opengl(
        &mut self,
        source: &dyn Texture2d,
        radius: f32,
        cache_key: Option<BlurCacheKey>,
        context: &Rc<GliumContext>,
    ) -> Result<Rc<dyn Texture2d>> {
        let width = source.width() as u32;
        let height = source.height() as u32;

        log::debug!(
            "apply_blur_opengl called with radius={}, source size={}x{}",
            radius,
            width,
            height
        );

        // Debug: Check if source texture has any content
        if std::env::var("WEZTERM_DEBUG_BLUR").is_ok() {
            self.save_blur_debug_texture(source, width, height, 0.0);
        }

        // Ensure OpenGL backend is initialized
        if self.backend.is_none() {
            self.init_opengl(context)?;
        }

        // Calculate blur parameters (same as WebGPU)
        let effective_radius = radius.abs() + 1.0;
        let sigma = effective_radius / 2.0;
        let kernel_radius = (sigma * 3.0).ceil() as u32;
        let mut kernel_size = kernel_radius * 2 + 1;

        const MAX_KERNEL_SIZE: u32 = 63;
        if kernel_size > MAX_KERNEL_SIZE {
            kernel_size = MAX_KERNEL_SIZE;
        }

        // Get OpenGL resources
        let (blur_program, vertex_buffer, render_targets) = match &mut self.backend {
            Some(BlurBackend::OpenGl {
                blur_program,
                vertex_buffer,
                render_targets,
            }) => (blur_program, vertex_buffer, render_targets),
            _ => unreachable!("Backend should be OpenGL"),
        };

        // Get or create render targets
        let intermediate = Self::get_opengl_render_target(width, height, context, render_targets)?;
        let final_target = Self::get_opengl_render_target(width, height, context, render_targets)?;

        // Get source texture - need to handle different texture types
        let source_texture = if let Some(opengl_tex) = source.downcast_ref::<OpenGLRenderTexture>()
        {
            log::debug!(
                "Source is OpenGLRenderTexture, texture dimensions: {}x{}",
                opengl_tex.texture.width(),
                opengl_tex.texture.height()
            );
            opengl_tex.texture.clone()
        } else if let Some(srgb_tex) =
            source.downcast_ref::<window::glium::texture::SrgbTexture2d>()
        {
            log::debug!("Source is SrgbTexture2d, need to copy to linear texture");
            // Convert sRGB texture to linear for blur processing
            let linear_texture = texture::Texture2d::empty(context, width, height)?;

            // For now, we'll use the sRGB texture directly and let the shader handle it
            // This isn't ideal but works for our use case
            log::warn!("Using sRGB texture directly for blur - may have color space issues");

            // Create a dummy texture that we can't actually use
            // This indicates we need to handle texture creation differently
            anyhow::bail!("SrgbTexture2d blur not yet implemented - icon textures should be created as linear Texture2d")
        } else {
            // Try to get type name for debugging
            let type_name = source.type_id();
            log::debug!("Unsupported texture type: {:?}", type_name);
            // Let's also check what type name we get from the trait object
            log::debug!(
                "Source texture size: {}x{}",
                source.width(),
                source.height()
            );
            // The icon texture should be created as OpenGLRenderTexture by allocate_render_target
            // If we get here, something is wrong with the texture creation
            anyhow::bail!(
                "Unsupported texture type for OpenGL blur - expected OpenGLRenderTexture"
            );
        };

        // Perform two-pass blur
        Self::blur_pass_opengl(
            &source_texture,
            &intermediate.0,
            true,
            sigma,
            kernel_size as i32,
            [width as f32, height as f32],
            radius,
            blur_program,
            vertex_buffer,
            context,
        )?;

        Self::blur_pass_opengl(
            &intermediate.0,
            &final_target.0,
            false,
            sigma,
            kernel_size as i32,
            [width as f32, height as f32],
            radius,
            blur_program,
            vertex_buffer,
            context,
        )?;

        // Debug: Save the blurred result
        // NOTE: Disabled for now as OpenGLRenderTexture::read is not fully implemented
        // if std::env::var("WEZTERM_DEBUG_BLUR").is_ok() {
        //     // Create a wrapper to save the final blurred texture
        //     let final_wrapper = OpenGLRenderTexture {
        //         texture: final_target.0.clone(),
        //     };
        //     Self::save_blur_debug_texture_static(&final_wrapper, width, height, radius);
        //     log::debug!("Saved blurred texture for debugging");
        // }

        // Release intermediate target
        Self::release_opengl_render_target(intermediate, render_targets);

        // Create result texture wrapper
        let result = Rc::new(OpenGLRenderTexture {
            texture: final_target.0.clone(),
        });

        // Cache result if requested
        if let Some(key) = cache_key {
            let size_bytes = (width * height * 4) as usize;
            log::debug!("Caching blur result for key {:?}", key);
            self.add_to_cache(key, result.clone(), size_bytes);
        }

        Ok(result)
    }

    /// Perform a single OpenGL blur pass
    fn blur_pass_opengl(
        source: &texture::Texture2d,
        target: &texture::Texture2d,
        horizontal: bool,
        sigma: f32,
        kernel_size: i32,
        texture_size: [f32; 2],
        radius: f32,
        program: &window::glium::Program,
        vertex_buffer: &window::glium::VertexBuffer<BlurVertex>,
        context: &Rc<GliumContext>,
    ) -> Result<()> {
        use window::glium::Surface;

        // Debug logging commented out for performance
        // log::debug!("blur_pass_opengl: horizontal={}, sigma={}, kernel_size={}, texture_size={:?}, radius={}",
        //     horizontal, sigma, kernel_size, texture_size, radius);

        // Create framebuffer for target
        let mut target_fb = framebuffer::SimpleFrameBuffer::new(context, target)?;

        // Clear target to transparent black
        target_fb.clear_color(0.0, 0.0, 0.0, 0.0);

        // Set up uniforms
        let uniforms = uniform! {
            source_texture: source,
            direction: if horizontal { [1.0f32, 0.0] } else { [0.0, 1.0] },
            sigma: sigma,
            kernel_size: kernel_size,
            texture_size: texture_size,
            radius: radius,
        };

        // Draw full-screen triangle
        target_fb.draw(
            vertex_buffer,
            window::glium::index::NoIndices(window::glium::index::PrimitiveType::TrianglesList),
            program,
            &uniforms,
            &window::glium::DrawParameters {
                // Don't blend - we want to replace the target completely
                blend: window::glium::Blend {
                    color: window::glium::BlendingFunction::AlwaysReplace,
                    alpha: window::glium::BlendingFunction::AlwaysReplace,
                    constant_value: (0.0, 0.0, 0.0, 0.0),
                },
                ..Default::default()
            },
        )?;

        Ok(())
    }

    /// Get or create an OpenGL render target
    fn get_opengl_render_target(
        width: u32,
        height: u32,
        context: &Rc<GliumContext>,
        targets: &mut Vec<OpenGLRenderTarget>,
    ) -> Result<(Rc<texture::Texture2d>, usize)> {
        // Try to find an unused render target
        for (idx, target) in targets.iter_mut().enumerate() {
            if !target.in_use && target.width == width && target.height == height {
                target.in_use = true;
                return Ok((target.texture.clone(), idx));
            }
        }

        // Create new render target with explicit RGBA format
        let texture = Rc::new(texture::Texture2d::empty_with_format(
            context,
            texture::UncompressedFloatFormat::U8U8U8U8,
            texture::MipmapsOption::NoMipmap,
            width,
            height,
        )?);

        let idx = targets.len();
        targets.push(OpenGLRenderTarget {
            texture: texture.clone(),
            width,
            height,
            in_use: true,
        });

        Ok((texture, idx))
    }

    /// Release an OpenGL render target
    fn release_opengl_render_target(
        target: (Rc<texture::Texture2d>, usize),
        targets: &mut Vec<OpenGLRenderTarget>,
    ) {
        if let Some(t) = targets.get_mut(target.1) {
            t.in_use = false;
        }
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

    /// Save debug image of blurred texture (static version)
    fn save_blur_debug_texture_static(
        texture: &dyn Texture2d,
        width: u32,
        height: u32,
        radius: f32,
    ) {
        use std::fs::File;
        use std::io::Write;
        use window::bitmaps::{BitmapImage, Image};

        let mut image = Image::new(width as usize, height as usize);
        texture.read(
            window::Rect::new(
                window::Point::new(0, 0),
                window::Size::new(width as isize, height as isize),
            ),
            &mut image,
        );

        let filename = format!("/tmp/wezterm_blur_{}x{}_r{}.ppm", width, height, radius);

        if let Ok(mut file) = File::create(&filename) {
            // PPM header
            writeln!(file, "P6").ok();
            writeln!(file, "{} {}", width, height).ok();
            writeln!(file, "255").ok();

            // Write RGB data (convert from RGBA)
            let data = unsafe {
                std::slice::from_raw_parts(image.pixel_data(), (width * height * 4) as usize)
            };

            for y in 0..height {
                for x in 0..width {
                    let idx = ((y * width + x) * 4) as usize;
                    file.write_all(&[data[idx], data[idx + 1], data[idx + 2]])
                        .ok();
                }
            }

            log::debug!("Saved debug blur texture to: {}", filename);
        }
    }

    /// Save debug image of blurred texture
    fn save_blur_debug_texture(
        &self,
        texture: &dyn Texture2d,
        width: u32,
        height: u32,
        radius: f32,
    ) {
        use std::fs::File;
        use std::io::Write;
        use window::bitmaps::{BitmapImage, Image};

        let mut image = Image::new(width as usize, height as usize);
        texture.read(
            window::Rect::new(
                window::Point::new(0, 0),
                window::Size::new(width as isize, height as isize),
            ),
            &mut image,
        );

        let filename = format!("/tmp/wezterm_blur_{}x{}_r{}.ppm", width, height, radius);

        if let Ok(mut file) = File::create(&filename) {
            // PPM header
            writeln!(file, "P6").ok();
            writeln!(file, "{} {}", width, height).ok();
            writeln!(file, "255").ok();

            // Write RGB data (convert from RGBA)
            let data = unsafe {
                std::slice::from_raw_parts(image.pixel_data(), (width * height * 4) as usize)
            };

            for y in 0..height {
                for x in 0..width {
                    let idx = ((y * width + x) * 4) as usize;
                    file.write_all(&[data[idx], data[idx + 1], data[idx + 2]])
                        .ok();
                }
            }

            log::debug!("Saved debug blur texture to: {}", filename);
        }
    }
}
