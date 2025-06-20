// Blur fragment shader for GPU-based Gaussian blur effect
// Direct port of blur.wgsl for OpenGL backend

precision highp float;

// BlurUniforms structure from WGSL
uniform vec2 direction;      // Direction of blur: (1,0) for horizontal, (0,1) for vertical
uniform float sigma;         // Standard deviation for Gaussian distribution
uniform int kernel_size;     // Number of samples in kernel (must be odd)
uniform vec2 texture_size;   // Size of the texture being blurred
uniform float radius;        // Blur radius in pixels

uniform sampler2D source_texture;

in vec2 tex_coords;
out vec4 frag_color;

// Calculate Gaussian weight for a given offset
float gaussian_weight(float x, float sigma_val) {
    float sigma2 = sigma_val * sigma_val;
    return exp(-(x * x) / (2.0 * sigma2)) / (sqrt(2.0 * 3.14159265359) * sigma_val);
}

void main() {
    vec2 texel_size = 1.0 / texture_size;
    vec4 color = vec4(0.0);
    float total_weight = 0.0;
    
    // Calculate half kernel size
    int half_kernel = kernel_size / 2;
    
    // Pre-calculate all weights to ensure symmetry
    float weights[63]; // Max kernel size we support (matching WGSL)
    float weight_sum = 0.0;
    
    // Calculate weights
    for (int i = -half_kernel; i <= half_kernel; i++) {
        int idx = i + half_kernel;
        weights[idx] = gaussian_weight(float(i), sigma);
        weight_sum += weights[idx];
    }
    
    // Normalize weights to ensure they sum to 1.0
    if (weight_sum > 0.0) {
        for (int i = 0; i <= 2 * half_kernel; i++) {
            weights[i] = weights[i] / weight_sum;
        }
    }
    
    // Perform blur in specified direction
    // Sample at every pixel within the blur radius
    float step_size = 1.0;
    
    for (int i = -half_kernel; i <= half_kernel; i++) {
        // Calculate offset in pixels
        float pixel_offset = float(i) * step_size;
        vec2 offset = pixel_offset * direction * texel_size;
        vec2 sample_pos = tex_coords + offset;
        int idx = i + half_kernel;
        
        // Always sample and apply weight - the sampler will clamp to edge
        vec4 sample = texture(source_texture, sample_pos);
        color += sample * weights[idx];
    }
    
    // Color already has normalized weights, so just return it
    frag_color = color;
}