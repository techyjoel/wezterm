// Blur shader for GPU-based Gaussian blur effect
// Used for neon glow effects with high performance

struct BlurUniforms {
    // Direction of blur: (1,0) for horizontal, (0,1) for vertical
    direction: vec2<f32>,
    // Standard deviation for Gaussian distribution
    sigma: f32,
    // Number of samples in kernel (must be odd)
    kernel_size: u32,
    // Size of the texture being blurred
    texture_size: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@group(0) @binding(0)
var<uniform> blur_uniforms: BlurUniforms;

@group(1) @binding(0)
var source_texture: texture_2d<f32>;

@group(1) @binding(1)
var texture_sampler: sampler;

// Vertex shader - renders a full-screen quad
@vertex
fn vs_blur(
    @builtin(vertex_index) vertex_idx: u32,
) -> VertexOutput {
    var out: VertexOutput;
    
    // Generate a full-screen triangle using vertex index
    // This creates a triangle that covers the entire screen
    let x = f32((vertex_idx << 1u) & 2u);
    let y = f32(vertex_idx & 2u);
    
    out.clip_position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.tex_coords = vec2<f32>(x, y);
    
    return out;
}

// Calculate Gaussian weight for a given offset
fn gaussian_weight(x: f32, sigma: f32) -> f32 {
    let sigma2 = sigma * sigma;
    return exp(-(x * x) / (2.0 * sigma2)) / (sqrt(2.0 * 3.14159265359) * sigma);
}

// Fragment shader - performs separable Gaussian blur
@fragment
fn fs_blur(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = 1.0 / blur_uniforms.texture_size;
    var color = vec4<f32>(0.0);
    var total_weight = 0.0;
    
    // Calculate half kernel size
    let half_kernel = i32(blur_uniforms.kernel_size / 2u);
    
    // Perform blur in specified direction
    for (var i = -half_kernel; i <= half_kernel; i++) {
        let offset = vec2<f32>(f32(i)) * blur_uniforms.direction * texel_size;
        let sample_pos = in.tex_coords + offset;
        
        // Clamp to texture bounds
        let clamped_pos = clamp(sample_pos, vec2<f32>(0.0), vec2<f32>(1.0));
        
        // Calculate Gaussian weight
        let weight = gaussian_weight(f32(i), blur_uniforms.sigma);
        
        // Sample texture and accumulate
        color += textureSample(source_texture, texture_sampler, clamped_pos) * weight;
        total_weight += weight;
    }
    
    // Normalize by total weight
    return color / total_weight;
}

// Optimized fragment shader for small kernels (5-9 samples)
@fragment
fn fs_blur_small(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = 1.0 / blur_uniforms.texture_size;
    
    // Pre-calculated weights for 9-tap Gaussian blur (sigma ~1.5)
    let weights = array<f32, 5>(
        0.227027, // Center
        0.1945946, // +/- 1
        0.1216216, // +/- 2
        0.054054,  // +/- 3
        0.016216   // +/- 4
    );
    
    var color = textureSample(source_texture, texture_sampler, in.tex_coords) * weights[0];
    
    for (var i = 1; i < 5; i++) {
        let offset = vec2<f32>(f32(i)) * blur_uniforms.direction * texel_size;
        
        // Sample both positive and negative offsets
        let sample_pos_p = in.tex_coords + offset;
        let sample_pos_n = in.tex_coords - offset;
        
        // Clamp positions
        let clamped_p = clamp(sample_pos_p, vec2<f32>(0.0), vec2<f32>(1.0));
        let clamped_n = clamp(sample_pos_n, vec2<f32>(0.0), vec2<f32>(1.0));
        
        // Accumulate weighted samples
        color += textureSample(source_texture, texture_sampler, clamped_p) * weights[i];
        color += textureSample(source_texture, texture_sampler, clamped_n) * weights[i];
    }
    
    return color;
}