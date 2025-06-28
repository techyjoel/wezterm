use crate::colorease::ColorEaseUniform;
use crate::renderstate::RenderContext;
use crate::termwindow::webgpu::ShaderUniform;
use crate::termwindow::RenderFrame;
use crate::uniforms::UniformBuilder;
use ::window::glium;
use ::window::glium::uniforms::{
    MagnifySamplerFilter, MinifySamplerFilter, Sampler, SamplerWrapFunction,
};
use ::window::glium::{BlendingFunction, LinearBlendingFactor, Surface};
use config::FreeTypeLoadTarget;

impl crate::TermWindow {
    pub fn call_draw(&mut self, frame: &mut RenderFrame) -> anyhow::Result<()> {
        match frame {
            RenderFrame::Glium(ref mut frame) => self.call_draw_glium(frame),
            RenderFrame::WebGpu => self.call_draw_webgpu(),
        }
    }

    fn call_draw_webgpu(&mut self) -> anyhow::Result<()> {
        use crate::termwindow::webgpu::WebGpuTexture;

        let webgpu = self.webgpu.as_mut().unwrap();
        let render_state = self.render_state.as_ref().unwrap();

        let output = webgpu.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = webgpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });
        let tex = render_state.glyph_cache.borrow().atlas.texture();
        let tex = tex.downcast_ref::<WebGpuTexture>().unwrap();
        let texture_view = tex.create_view();

        let texture_linear_bind_group =
            webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &webgpu.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&webgpu.texture_linear_sampler),
                    },
                ],
                label: Some("linear bind group"),
            });

        let texture_nearest_bind_group =
            webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &webgpu.texture_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&texture_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&webgpu.texture_nearest_sampler),
                    },
                ],
                label: Some("nearest bind group"),
            });

        let mut cleared = false;
        let foreground_text_hsb = self.config.foreground_text_hsb;
        let foreground_text_hsb = [
            foreground_text_hsb.hue,
            foreground_text_hsb.saturation,
            foreground_text_hsb.brightness,
        ];

        let milliseconds = self.created.elapsed().as_millis() as u32;
        let projection = euclid::Transform3D::<f32, f32, f32>::ortho(
            -(self.dimensions.pixel_width as f32) / 2.0,
            self.dimensions.pixel_width as f32 / 2.0,
            self.dimensions.pixel_height as f32 / 2.0,
            -(self.dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays_transposed();

        for layer in render_state.layers.borrow().iter() {
            for idx in 0..3 {
                let vb = &layer.vb.borrow()[idx];
                let (vertex_count, index_count) = vb.vertex_index_count();
                let vertex_buffer;
                let uniforms;
                if vertex_count > 0 {
                    let mut vertices = vb.current_vb_mut();
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: if cleared {
                                    wgpu::LoadOp::Load
                                } else {
                                    wgpu::LoadOp::Clear(wgpu::Color {
                                        r: 0.,
                                        g: 0.,
                                        b: 0.,
                                        a: 0.,
                                    })
                                },
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });
                    cleared = true;

                    // Apply scissor rect if we have one on the stack
                    if let Some(scissor_rect) = render_state.get_current_scissor() {
                        if scissor_rect.size.width <= 0.0 || scissor_rect.size.height <= 0.0 {
                            // Zero-sized scissor rect clips everything
                            log::trace!("WebGPU: Setting zero scissor rect (clips all)");
                            render_pass.set_scissor_rect(0, 0, 0, 0);
                        } else {
                            // Clamp to viewport bounds
                            let viewport_width = self.dimensions.pixel_width as f32;
                            let viewport_height = self.dimensions.pixel_height as f32;

                            let x = scissor_rect.origin.x.max(0.0).min(viewport_width) as u32;
                            let y = scissor_rect.origin.y.max(0.0).min(viewport_height) as u32;
                            let width = scissor_rect
                                .size
                                .width
                                .max(0.0)
                                .min(viewport_width - scissor_rect.origin.x)
                                as u32;
                            let height = scissor_rect
                                .size
                                .height
                                .max(0.0)
                                .min(viewport_height - scissor_rect.origin.y)
                                as u32;

                            log::trace!(
                                "WebGPU: Setting scissor rect x={}, y={}, w={}, h={} (from {:?})",
                                x,
                                y,
                                width,
                                height,
                                scissor_rect
                            );
                            render_pass.set_scissor_rect(x, y, width, height);
                        }
                    } else {
                        log::trace!("WebGPU: No scissor rect on stack");
                    }

                    uniforms = webgpu.create_uniform(ShaderUniform {
                        foreground_text_hsb,
                        milliseconds,
                        projection,
                    });

                    render_pass.set_pipeline(&webgpu.render_pipeline);
                    render_pass.set_bind_group(0, &uniforms, &[]);
                    render_pass.set_bind_group(1, &texture_linear_bind_group, &[]);
                    render_pass.set_bind_group(2, &texture_nearest_bind_group, &[]);
                    vertex_buffer = vertices.webgpu_mut().recreate();
                    vertex_buffer.unmap();
                    render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                    render_pass
                        .set_index_buffer(vb.indices.webgpu().slice(..), wgpu::IndexFormat::Uint32);
                    render_pass.draw_indexed(0..index_count as _, 0, 0..1);
                }

                vb.next_index();
            }
        }

        // Render effects overlay after main content
        if let Some(ref mut overlay) = self.effects_overlay.borrow_mut().as_mut() {
            if let Err(e) = overlay.render(webgpu, &view, &mut encoder, &self.dimensions) {
                log::warn!("Effects overlay render failed: {}", e);
            }
        }

        // submit will accept anything that implements IntoIter
        webgpu.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn call_draw_glium(&mut self, frame: &mut glium::Frame) -> anyhow::Result<()> {
        use window::glium::texture::SrgbTexture2d;

        let gl_state = self.render_state.as_ref().unwrap();
        let tex = gl_state.glyph_cache.borrow().atlas.texture();
        let tex = tex.downcast_ref::<SrgbTexture2d>().unwrap();

        frame.clear_color(0., 0., 0., 0.);

        let projection = euclid::Transform3D::<f32, f32, f32>::ortho(
            -(self.dimensions.pixel_width as f32) / 2.0,
            self.dimensions.pixel_width as f32 / 2.0,
            self.dimensions.pixel_height as f32 / 2.0,
            -(self.dimensions.pixel_height as f32) / 2.0,
            -1.0,
            1.0,
        )
        .to_arrays_transposed();

        let use_subpixel = match self
            .config
            .freetype_render_target
            .unwrap_or(self.config.freetype_load_target)
        {
            FreeTypeLoadTarget::HorizontalLcd | FreeTypeLoadTarget::VerticalLcd => true,
            _ => false,
        };

        let dual_source_blending = glium::DrawParameters {
            blend: glium::Blend {
                color: BlendingFunction::Addition {
                    source: LinearBlendingFactor::SourceOneColor,
                    destination: LinearBlendingFactor::OneMinusSourceOneColor,
                },
                alpha: BlendingFunction::Addition {
                    source: LinearBlendingFactor::SourceOneColor,
                    destination: LinearBlendingFactor::OneMinusSourceOneColor,
                },
                constant_value: (0.0, 0.0, 0.0, 0.0),
            },

            ..Default::default()
        };

        let alpha_blending = glium::DrawParameters {
            blend: glium::Blend {
                color: BlendingFunction::Addition {
                    source: LinearBlendingFactor::SourceAlpha,
                    destination: LinearBlendingFactor::OneMinusSourceAlpha,
                },
                alpha: BlendingFunction::Addition {
                    source: LinearBlendingFactor::One,
                    destination: LinearBlendingFactor::OneMinusSourceAlpha,
                },
                constant_value: (0.0, 0.0, 0.0, 0.0),
            },
            ..Default::default()
        };

        // Clamp and use the nearest texel rather than interpolate.
        // This prevents things like the box cursor outlines from
        // being randomly doubled in width or height
        let atlas_nearest_sampler = Sampler::new(&*tex)
            .wrap_function(SamplerWrapFunction::Clamp)
            .magnify_filter(MagnifySamplerFilter::Nearest)
            .minify_filter(MinifySamplerFilter::Nearest);

        let atlas_linear_sampler = Sampler::new(&*tex)
            .wrap_function(SamplerWrapFunction::Clamp)
            .magnify_filter(MagnifySamplerFilter::Linear)
            .minify_filter(MinifySamplerFilter::Linear);

        let foreground_text_hsb = self.config.foreground_text_hsb;
        let foreground_text_hsb = (
            foreground_text_hsb.hue,
            foreground_text_hsb.saturation,
            foreground_text_hsb.brightness,
        );

        let milliseconds = self.created.elapsed().as_millis() as u32;

        let cursor_blink: ColorEaseUniform = (*self.cursor_blink_state.borrow()).into();
        let blink: ColorEaseUniform = (*self.blink_state.borrow()).into();
        let rapid_blink: ColorEaseUniform = (*self.rapid_blink_state.borrow()).into();

        // Helper function to create draw parameters with optional scissor test
        let create_draw_params = |blend: glium::Blend| -> glium::DrawParameters {
            let mut params = glium::DrawParameters {
                blend,
                ..Default::default()
            };

            // Apply scissor test if we have a scissor rect on the stack
            if let Some(scissor_rect) = gl_state.get_current_scissor() {
                // Skip zero-sized scissor rects (no intersection)
                if scissor_rect.size.width <= 0.0 || scissor_rect.size.height <= 0.0 {
                    // Create a zero-sized scissor to clip everything
                    log::trace!("OpenGL: Setting zero scissor rect (clips all)");
                    params.scissor = Some(glium::Rect {
                        left: 0,
                        bottom: 0,
                        width: 0,
                        height: 0,
                    });
                } else {
                    // Convert from our coordinate system (top-left origin) to OpenGL (bottom-left origin)
                    let window_height = self.dimensions.pixel_height as f32;
                    let window_width = self.dimensions.pixel_width as f32;

                    // Clamp to viewport bounds and ensure non-negative
                    let x = scissor_rect.origin.x.max(0.0).min(window_width) as u32;
                    let y_top = scissor_rect.origin.y.max(0.0).min(window_height);
                    let width = scissor_rect
                        .size
                        .width
                        .max(0.0)
                        .min(window_width - scissor_rect.origin.x)
                        as u32;
                    let height = scissor_rect
                        .size
                        .height
                        .max(0.0)
                        .min(window_height - scissor_rect.origin.y)
                        as u32;

                    // Convert Y coordinate from top-left to bottom-left origin
                    let y_bottom = (window_height - y_top - height as f32).max(0.0) as u32;

                    log::trace!(
                        "OpenGL: Setting scissor rect left={}, bottom={}, w={}, h={} (from {:?})",
                        x,
                        y_bottom,
                        width,
                        height,
                        scissor_rect
                    );

                    params.scissor = Some(glium::Rect {
                        left: x,
                        bottom: y_bottom,
                        width,
                        height,
                    });
                }
            } else {
                log::trace!("OpenGL: No scissor rect on stack");
            }

            params
        };

        for layer in gl_state.layers.borrow().iter() {
            for idx in 0..3 {
                let vb = &layer.vb.borrow()[idx];
                let (vertex_count, index_count) = vb.vertex_index_count();
                if vertex_count > 0 {
                    let vertices = vb.current_vb_mut();
                    let subpixel_aa = use_subpixel && idx == 1;

                    let mut uniforms = UniformBuilder::default();

                    uniforms.add("projection", &projection);
                    uniforms.add("atlas_nearest_sampler", &atlas_nearest_sampler);
                    uniforms.add("atlas_linear_sampler", &atlas_linear_sampler);
                    uniforms.add("foreground_text_hsb", &foreground_text_hsb);
                    uniforms.add("subpixel_aa", &subpixel_aa);
                    uniforms.add("milliseconds", &milliseconds);
                    uniforms.add_struct("cursor_blink", &cursor_blink);
                    uniforms.add_struct("blink", &blink);
                    uniforms.add_struct("rapid_blink", &rapid_blink);

                    let draw_params = if subpixel_aa {
                        create_draw_params(dual_source_blending.blend)
                    } else {
                        create_draw_params(alpha_blending.blend)
                    };

                    frame.draw(
                        vertices.glium().slice(0..vertex_count).unwrap(),
                        vb.indices.glium().slice(0..index_count).unwrap(),
                        gl_state.glyph_prog.as_ref().unwrap(),
                        &uniforms,
                        &draw_params,
                    )?;
                }

                vb.next_index();
            }
        }

        // Render effects overlay after main content
        if let Some(ref mut overlay) = self.effects_overlay.borrow_mut().as_mut() {
            log::debug!(
                "Checking OpenGL effects overlay render, has {} effects",
                overlay.effect_count()
            );
            match &gl_state.context {
                RenderContext::Glium(context) => {
                    log::debug!(
                        "Calling overlay.render_opengl with {} effects",
                        overlay.effect_count()
                    );
                    if let Err(e) = overlay.render_opengl(frame, context, &self.dimensions) {
                        log::warn!("OpenGL effects overlay render failed: {}", e);
                    }
                }
                _ => {
                    log::debug!("Not a Glium context");
                }
            }
        } else {
            log::trace!("No effects overlay available");
        }

        Ok(())
    }
}
