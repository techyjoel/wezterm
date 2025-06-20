// Glow composite fragment shader for compositing glow effects
// Direct port of glow_composite.wgsl for OpenGL backend

precision highp float;

uniform float intensity;
uniform sampler2D glow_texture;

in vec2 tex_coords;
out vec4 frag_color;

// Composite glow with additive blending
void main() {
    vec4 glow_sample = texture(glow_texture, tex_coords);
    
    // Apply intensity to the glow
    vec4 result = glow_sample;
    result = result * intensity;
    
    // For additive blending, we want to add the glow color to the existing content
    // The alpha channel controls how much glow to add
    frag_color = result;
}