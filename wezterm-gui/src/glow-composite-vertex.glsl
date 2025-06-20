// Glow composite vertex shader for compositing glow effects
// Direct port of glow_composite.wgsl for OpenGL backend

precision highp float;

// Glow uniforms matching the WGSL ShaderUniform struct
uniform float intensity;
uniform float glow_x;
uniform float glow_y;
uniform float glow_width;
uniform float glow_height;
uniform float screen_width;
uniform float screen_height;
uniform mat4 projection;

out vec2 tex_coords;

// Generate a quad at the glow position
void main() {
    // Generate quad vertices (2 triangles)
    // 0: top-left, 1: top-right, 2: bottom-left
    // 3: bottom-left, 4: top-right, 5: bottom-right
    vec2 vertex_positions[6];
    vertex_positions[0] = vec2(0.0, 0.0);    // top-left
    vertex_positions[1] = vec2(1.0, 0.0);    // top-right
    vertex_positions[2] = vec2(0.0, 1.0);    // bottom-left
    vertex_positions[3] = vec2(0.0, 1.0);    // bottom-left
    vertex_positions[4] = vec2(1.0, 0.0);    // top-right
    vertex_positions[5] = vec2(1.0, 1.0);    // bottom-right
    
    vec2 pos = vertex_positions[gl_VertexID];
    
    // Calculate screen position
    float screen_x = glow_x + pos.x * glow_width;
    float screen_y = glow_y + pos.y * glow_height;
    
    // Convert to normalized device coordinates (-1 to 1)
    float ndc_x = (screen_x / screen_width) * 2.0 - 1.0;
    float ndc_y = 1.0 - (screen_y / screen_height) * 2.0;
    
    gl_Position = vec4(ndc_x, ndc_y, 0.0, 1.0);
    // Don't flip Y - both the blur texture and screen use the same coordinate system
    tex_coords = vec2(pos.x, pos.y);
}