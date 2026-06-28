#version 450
// probe_render.vert — vertex-less fullscreen-triangle (mabda N7.2a capture).
// Positions derived from gl_VertexIndex; NO vertex buffers / attributes, so the
// captured pushbuffer is about color-target + program + draw, not vertex streams.
//   idx 0 -> (-1,-1)   idx 1 -> ( 3,-1)   idx 2 -> (-1, 3)
// The oversized triangle covers the whole [-1,1] clip square (center pixel hit).
void main() {
    vec2 p = vec2((gl_VertexIndex << 1) & 2, gl_VertexIndex & 2);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
