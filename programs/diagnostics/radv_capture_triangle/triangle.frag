#version 450

// Solid red FS — matches mabda's native_gfx9_shader_solid_red.
// No interpolated inputs, just exp mrt0 (1.0, 0.0, 0.0, 1.0) done vm.
layout(location = 0) out vec4 frag_color;

void main() {
    frag_color = vec4(1.0, 0.0, 0.0, 1.0);
}
