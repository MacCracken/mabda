// store_tex.cu — N6.2c SM75 sampling kernel. Samples a bound texture object
// at texel (0,0) and stores the raw RGBA8 as a packed u32. Build-time only.
extern "C" __global__ void sample_tex(cudaTextureObject_t tex, unsigned int *out) {
    uchar4 c = tex2D<uchar4>(tex, 0.0f, 0.0f);
    out[0] = (unsigned)c.x | ((unsigned)c.y<<8) | ((unsigned)c.z<<16) | ((unsigned)c.w<<24);
}
