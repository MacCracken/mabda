#version 450

// Fullscreen triangle from gl_VertexIndex — matches mabda's
// native_gfx9_shader_fullscreen_triangle_vs (vid_to_pos).
//   vid=0 -> (-1,-1)
//   vid=1 -> ( 3,-1)
//   vid=2 -> (-1, 3)
// Rasterizer clips to viewport, covering the full RT.
void main() {
    vec2 xy = vec2(
        float(int(gl_VertexIndex & 1) * 4 - 1),
        float(int((gl_VertexIndex >> 1) & 1) * 4 - 1)
    );
    gl_Position = vec4(xy, 0.0, 1.0);
}
