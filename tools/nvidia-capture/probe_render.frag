#version 450
// probe_render.frag — solid color write (mabda N7.2a capture).
// Writes vec4(0.2,0.4,0.6,1.0) -> R8G8B8A8_UNORM bytes (51,102,153,255) =
// 0x33,0x66,0x99,0xFF; little-endian u32 readback 0xFF996633.
layout(location = 0) out vec4 o_color;
void main() {
    o_color = vec4(0.2, 0.4, 0.6, 1.0);
}
