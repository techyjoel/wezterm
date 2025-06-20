// Shader for compositing glow effects onto the main render

struct ShaderUniform {
    intensity: f32,
    // Position and size of the glow texture in screen space
    glow_x: f32,
    glow_y: f32,
    glow_width: f32,
    glow_height: f32,
    // Screen dimensions for coordinate transformation
    screen_width: f32,
    screen_height: f32,
    _padding: u32,
    projection: mat4x4<f32>,
}

struct VertexInput {
    @builtin(vertex_index) vertex_idx: u32,
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

// Generate a quad at the glow position
@vertex
fn vs_glow(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    // Generate quad vertices (2 triangles)
    // 0: top-left, 1: top-right, 2: bottom-left
    // 3: bottom-left, 4: top-right, 5: bottom-right
    let vertex_positions = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),    // top-left
        vec2<f32>(1.0, 0.0),    // top-right
        vec2<f32>(0.0, 1.0),    // bottom-left
        vec2<f32>(0.0, 1.0),    // bottom-left
        vec2<f32>(1.0, 0.0),    // top-right
        vec2<f32>(1.0, 1.0)     // bottom-right
    );
    
    let pos = vertex_positions[in.vertex_idx];
    
    // Calculate screen position
    let screen_x = uniforms.glow_x + pos.x * uniforms.glow_width;
    let screen_y = uniforms.glow_y + pos.y * uniforms.glow_height;
    
    // Convert to normalized device coordinates (-1 to 1)
    let ndc_x = (screen_x / uniforms.screen_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (screen_y / uniforms.screen_height) * 2.0;
    
    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    // Don't flip Y - both the blur texture and screen use the same coordinate system
    out.tex_coords = vec2<f32>(pos.x, pos.y);
    
    return out;
}

// Composite glow with additive blending
@fragment
fn fs_glow(in: VertexOutput) -> @location(0) vec4<f32> {
    let glow_sample = textureSample(glow_texture, texture_sampler, in.tex_coords);
    
    // Apply intensity to the glow
    var result = glow_sample;
    result = result * uniforms.intensity;
    
    // For additive blending, we want to add the glow color to the existing content
    // The alpha channel controls how much glow to add
    return result;
}