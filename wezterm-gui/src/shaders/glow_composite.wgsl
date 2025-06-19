// Shader for compositing glow effects onto the main render

struct ShaderUniform {
    intensity: f32, // Using first component of foreground_text_hsb
    _padding1: f32,
    _padding2: f32,
    _padding3: u32,
    projection: mat4x4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: ShaderUniform;

@group(1) @binding(0)
var glow_texture: texture_2d<f32>;

@group(1) @binding(1)
var texture_sampler: sampler;

// Generate full-screen triangle
@vertex
fn vs_glow(@builtin(vertex_index) vertex_idx: u32) -> VertexOutput {
    var out: VertexOutput;
    
    // Generate full-screen triangle
    let x = f32((vertex_idx << 1u) & 2u);
    let y = f32(vertex_idx & 2u);
    
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.tex_coords = vec2<f32>(x, y);
    
    return out;
}

// Composite glow with additive blending
@fragment
fn fs_glow(in: VertexOutput) -> @location(0) vec4<f32> {
    let glow_sample = textureSample(glow_texture, texture_sampler, in.tex_coords);
    
    // Apply intensity to alpha channel for glow strength
    var result = glow_sample;
    result.a = result.a * uniforms.intensity;
    
    // For additive glow, premultiply RGB by alpha
    result.r = result.r * result.a;
    result.g = result.g * result.a;
    result.b = result.b * result.a;
    
    return result;
}