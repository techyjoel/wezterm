// Blur vertex shader for GPU-based Gaussian blur effect
// Direct port of blur.wgsl for OpenGL backend

precision highp float;

// Vertex position input
in vec2 position;

out vec2 tex_coords;

void main() {
    // Use the provided vertex position
    gl_Position = vec4(position, 0.0, 1.0);
    
    // Calculate texture coordinates from position
    // Position ranges from (-3,-1) to (1,3) for the full-screen triangle
    // We need to map this to (0,0) to (1,1) for texture coordinates
    tex_coords = vec2((position.x + 1.0) * 0.5, (position.y + 1.0) * 0.5);
}