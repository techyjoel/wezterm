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
    // Blur radius in pixels
    radius: f32,
    _padding: f32,
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
    // Don't flip Y - both source texture and render target use same coordinate system
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
    
    // Pre-calculate all weights to ensure symmetry
    var weights: array<f32, 31>; // Max kernel size we support
    var weight_sum = 0.0;
    
    // Center weight (i=0) should be the highest
    let center_idx = half_kernel;
    
    for (var i = -half_kernel; i <= half_kernel; i++) {
        let idx = i + half_kernel;
        weights[idx] = gaussian_weight(f32(i), blur_uniforms.sigma);
        weight_sum += weights[idx];
    }
    
    // Normalize weights to ensure they sum to 1.0
    if (weight_sum > 0.0) {
        for (var i = 0; i <= 2 * half_kernel; i++) {
            weights[i] = weights[i] / weight_sum;
        }
    }
    
    // Perform blur in specified direction
    // Sample at every pixel within the blur radius
    let step_size = 1.0;
    
    for (var i = -half_kernel; i <= half_kernel; i++) {
        // Calculate offset in pixels
        let pixel_offset = f32(i) * step_size;
        let offset = pixel_offset * blur_uniforms.direction * texel_size;
        let sample_pos = in.tex_coords + offset;
        let idx = i + half_kernel;
        
        // Always sample and apply weight - the sampler will clamp to edge
        let sample = textureSample(source_texture, texture_sampler, sample_pos);
        color += sample * weights[idx];
    }
    
    // Color already has normalized weights, so just return it
    return color;
}

// Optimized fragment shader for small kernels (5-9 samples)
@fragment
fn fs_blur_small(in: VertexOutput) -> @location(0) vec4<f32> {
    let texel_size = 1.0 / blur_uniforms.texture_size;
    
    // Calculate weights dynamically based on actual sigma for better accuracy
    var weights: array<f32, 5>;
    var weight_sum = gaussian_weight(0.0, blur_uniforms.sigma);
    weights[0] = weight_sum;
    
    for (var i = 1; i < 5; i++) {
        let w = gaussian_weight(f32(i), blur_uniforms.sigma);
        weights[i] = w;
        weight_sum += 2.0 * w; // Account for both positive and negative offsets
    }
    
    // Normalize weights
    for (var i = 0; i < 5; i++) {
        weights[i] = weights[i] / weight_sum;
    }
    
    var color = textureSample(source_texture, texture_sampler, in.tex_coords) * weights[0];
    
    for (var i = 1; i < 5; i++) {
        let offset = vec2<f32>(f32(i)) * blur_uniforms.direction * texel_size;
        
        // Sample both positive and negative offsets
        // Always sample - let the sampler handle edge clamping
        let sample_pos_p = in.tex_coords + offset;
        let sample_pos_n = in.tex_coords - offset;
        
        color += textureSample(source_texture, texture_sampler, sample_pos_p) * weights[i];
        color += textureSample(source_texture, texture_sampler, sample_pos_n) * weights[i];
    }
    
    // Weights are already normalized, just return the color
    return color;
}