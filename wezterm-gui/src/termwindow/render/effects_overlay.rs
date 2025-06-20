use crate::renderstate::RenderContext;
use crate::termwindow::render::blur::{BlurCacheKey, BlurRenderer};
use crate::termwindow::webgpu::{WebGpuState, WebGpuTexture};
use anyhow::Result;
use std::rc::Rc;
use wgpu::util::DeviceExt;
use window::color::LinearRgba;
use window::{Point, Rect, RectF};

/// Simple effects overlay system for rendering glows and other effects
pub struct EffectsOverlay {
    blur_renderer: BlurRenderer,
    active_effects: Vec<GlowEffect>,
    // Composite shader pipeline (to be implemented)
    composite_pipeline: Option<wgpu::RenderPipeline>,
    uniform_bind_group_layout: Option<wgpu::BindGroupLayout>,
}

#[derive(Clone)]
pub struct GlowEffect {
    pub texture: Rc<dyn window::bitmaps::Texture2d>,
    /// Window-relative position where the glow should be rendered (top-left of glow area)
    pub window_position: Point,
    pub intensity: f32,
}

impl EffectsOverlay {
    pub fn new() -> Self {
        Self {
            blur_renderer: BlurRenderer::new(50), // 50MB cache
            active_effects: Vec::new(),
            composite_pipeline: None,
            uniform_bind_group_layout: None,
        }
    }

    /// Initialize the composite pipeline for rendering effects
    pub fn init_pipeline(
        webgpu: &WebGpuState,
    ) -> Result<(wgpu::RenderPipeline, wgpu::BindGroupLayout)> {
        // Create shader for compositing glows
        let shader = webgpu
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glow Composite Shader"),
                source: wgpu::ShaderSource::Wgsl(
                    include_str!("../../shaders/glow_composite.wgsl").into(),
                ),
            });

        // Create uniform bind group layout to match the shader
        let uniform_bind_group_layout =
            webgpu
                .device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                    label: Some("Glow uniform bind group layout"),
                });

        let pipeline_layout =
            webgpu
                .device
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("Glow Pipeline Layout"),
                    bind_group_layouts: &[
                        &uniform_bind_group_layout,
                        &webgpu.texture_bind_group_layout,
                    ],
                    push_constant_ranges: &[],
                });

        let pipeline = webgpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Glow Composite Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_glow"),
                    buffers: &[], // No vertex buffer - generate vertices in shader
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_glow"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: webgpu.config.borrow().format,
                        blend: Some(wgpu::BlendState {
                            color: wgpu::BlendComponent {
                                src_factor: wgpu::BlendFactor::SrcAlpha,
                                dst_factor: wgpu::BlendFactor::One, // Additive for glow
                                operation: wgpu::BlendOperation::Add,
                            },
                            alpha: wgpu::BlendComponent::OVER,
                        }),
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

        Ok((pipeline, uniform_bind_group_layout))
    }

    /// Add a glow effect to be rendered this frame
    pub fn add_glow(&mut self, effect: GlowEffect) {
        log::trace!(
            "Adding glow effect at window position {:?}, intensity: {}, texture size: {}x{}",
            effect.window_position,
            effect.intensity,
            effect.texture.width(),
            effect.texture.height()
        );
        self.active_effects.push(effect);
    }

    /// Clear effects for next frame
    pub fn clear_effects(&mut self) {
        self.active_effects.clear();
    }

    /// Render all active effects
    pub fn render(
        &mut self,
        webgpu: &WebGpuState,
        output_view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        dimensions: &window::Dimensions,
    ) -> Result<()> {
        if self.active_effects.is_empty() {
            return Ok(());
        }

        log::trace!("Rendering {} glow effects", self.active_effects.len());

        // Initialize pipeline if needed
        if self.composite_pipeline.is_none() {
            match Self::init_pipeline(webgpu) {
                Ok((pipeline, layout)) => {
                    self.composite_pipeline = Some(pipeline);
                    self.uniform_bind_group_layout = Some(layout);
                }
                Err(e) => {
                    log::error!("Failed to init glow pipeline: {}", e);
                    return Ok(());
                }
            }
        }

        // Process each glow effect
        for effect in &self.active_effects {
            // Composite the pre-blurred glow texture
            if let Err(e) = self.composite_glow(
                webgpu,
                &effect.texture,
                &effect,
                output_view,
                encoder,
                dimensions,
            ) {
                log::warn!("Failed to composite glow: {}", e);
            }
        }

        Ok(())
    }

    /// Composite a single glow effect
    fn composite_glow(
        &self,
        webgpu: &WebGpuState,
        glow_texture: &Rc<dyn window::bitmaps::Texture2d>,
        effect: &GlowEffect,
        output_view: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        dimensions: &window::Dimensions,
    ) -> Result<()> {
        let glow_webgpu = glow_texture
            .downcast_ref::<WebGpuTexture>()
            .ok_or_else(|| anyhow::anyhow!("Glow texture is not WebGPU"))?;

        // Create render pass that preserves existing content
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Glow Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load, // Preserve existing content
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        // Set pipeline
        render_pass.set_pipeline(self.composite_pipeline.as_ref().unwrap());

        // Create custom uniform structure for glow shader
        #[repr(C)]
        #[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
        struct GlowUniforms {
            intensity: f32,
            glow_x: f32,
            glow_y: f32,
            glow_width: f32,
            glow_height: f32,
            screen_width: f32,
            screen_height: f32,
            _padding: u32,
            projection: [[f32; 4]; 4],
        }

        // Get glow texture dimensions
        let glow_width = glow_webgpu.width() as f32;
        let glow_height = glow_webgpu.height() as f32;

        // Use the pre-calculated window position directly
        // The caller is responsible for positioning the glow correctly relative to the content
        let glow_x = effect.window_position.x as f32;
        let glow_y = effect.window_position.y as f32;

        log::error!(
            "COMPOSITE DEBUG: Compositing glow: texture {}x{}, position ({}, {}), screen {}x{}",
            glow_width,
            glow_height,
            glow_x,
            glow_y,
            dimensions.pixel_width,
            dimensions.pixel_height
        );

        let uniforms = GlowUniforms {
            intensity: effect.intensity,
            glow_x,
            glow_y,
            glow_width,
            glow_height,
            screen_width: dimensions.pixel_width as f32,
            screen_height: dimensions.pixel_height as f32,
            _padding: 0,
            projection: self.create_projection_matrix(dimensions),
        };

        // Create uniform buffer and bind group
        let uniform_buffer = webgpu
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Glow Uniform Buffer"),
                contents: bytemuck::cast_slice(&[uniforms]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let uniform_bind_group = webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: self.uniform_bind_group_layout.as_ref().unwrap(),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
            label: Some("Glow uniform bind group"),
        });

        // Create texture bind group for glow
        let glow_view = glow_webgpu.create_view();
        let texture_bind_group = webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &webgpu.texture_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&glow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&webgpu.texture_linear_sampler),
                },
            ],
            label: Some("Glow texture bind group"),
        });

        render_pass.set_bind_group(0, &uniform_bind_group, &[]);
        render_pass.set_bind_group(1, &texture_bind_group, &[]);

        // Draw a quad (6 vertices for 2 triangles)
        render_pass.draw(0..6, 0..1);

        Ok(())
    }

    fn create_projection_matrix(&self, dimensions: &window::Dimensions) -> [[f32; 4]; 4] {
        euclid::Transform3D::<f32, f32, f32>::ortho(
            -(dimensions.pixel_width as f32) / 2.0,
            dimensions.pixel_width as f32 / 2.0,
            dimensions.pixel_height as f32 / 2.0,
            -(dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays_transposed()
    }
}
