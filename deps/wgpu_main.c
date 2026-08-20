// wgpu_main.c — C launcher for mabda Cyrius tests
// Links Cyrius .o with wgpu-native, provides function table + callbacks
#include "wgpu-native/include/webgpu/webgpu.h"
#include "wgpu-native/include/webgpu/wgpu.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>
#include <stdbool.h>

// === C callbacks ===

static void c_on_adapter(WGPURequestAdapterStatus status, WGPUAdapter adapter,
    WGPUStringView message, void* ud1, void* ud2) {
    if (status == WGPURequestAdapterStatus_Success && ud1)
        *(long*)ud1 = (long)adapter;
}

static void c_on_device(WGPURequestDeviceStatus status, WGPUDevice device,
    WGPUStringView message, void* ud1, void* ud2) {
    if (status == WGPURequestDeviceStatus_Success && ud1)
        *(long*)ud1 = (long)device;
}

static void c_on_buffer_mapped(WGPUMapAsyncStatus status,
    WGPUStringView message, void* ud1, void* ud2) {
    if (ud1) *(long*)ud1 = (long)status;
}

// === Simplified shims — Cyrius calls these, C handles callback structs ===

// Request adapter: (instance, power_pref, result_ptr) → adapter handle in *result_ptr
long wgpu_shim_request_adapter(WGPUInstance instance, long power_pref, long* result_ptr) {
    WGPURequestAdapterOptions opts = {0};
    opts.powerPreference = (WGPUPowerPreference)power_pref;
    WGPURequestAdapterCallbackInfo cb = {
        .mode = WGPUCallbackMode_AllowSpontaneous,
        .callback = c_on_adapter,
        .userdata1 = result_ptr,
    };
    wgpuInstanceRequestAdapter(instance, &opts, cb);
    wgpuInstanceProcessEvents(instance);
    return *result_ptr;
}

// Request device: (adapter, result_ptr) → device handle in *result_ptr
long wgpu_shim_request_device(WGPUAdapter adapter, long* result_ptr) {
    WGPUDeviceDescriptor desc = WGPU_DEVICE_DESCRIPTOR_INIT;
    // v3.2 (T.8): enable the block-compressed texture device features the
    // adapter supports, so consumers can create BC/ETC2/ASTC textures
    // (wgpuDeviceCreateTexture aborts on a format whose feature is not
    // enabled at device creation). Requesting a feature the adapter lacks
    // fails device creation, so filter by wgpuAdapterHasFeature first.
    // v3.2 (F.8b): enable ShaderF64 — the f64 ROUTE on wgpu. HW-verified on
    // Cezanne: wgpu-native's WGPUNativeFeature_ShaderF64 (0x0003001D) is what makes
    // f64 SPIR-V usable, and naga's SPIR-V frontend ACCEPTS f64 once it is enabled —
    // so f64 rides the ordinary gpu_shader_module_create_spirv path, NOT passthrough.
    // v3.2 (F.8a): SpirvShaderPassthrough is a SEPARATE, general raw-SPIR-V escape
    // hatch (naga-bypass via wgpuDeviceCreateShaderModuleSpirV) — kept for shaders
    // naga cannot carry / adapters that prefer it; it is NOT the f64 path and is
    // unsupported on this RADV adapter (wgpuAdapterHasFeature returns 0 there).
    // All gated by wgpuAdapterHasFeature (requesting a feature the adapter lacks
    // fails device creation); a launcher without these edits simply has no wgpu f64.
    WGPUFeatureName feats[8];
    size_t nfeat = 0;
    if (wgpuAdapterHasFeature(adapter, WGPUFeatureName_TextureCompressionBC))
        feats[nfeat++] = WGPUFeatureName_TextureCompressionBC;
    if (wgpuAdapterHasFeature(adapter, WGPUFeatureName_TextureCompressionETC2))
        feats[nfeat++] = WGPUFeatureName_TextureCompressionETC2;
    if (wgpuAdapterHasFeature(adapter, WGPUFeatureName_TextureCompressionASTC))
        feats[nfeat++] = WGPUFeatureName_TextureCompressionASTC;
    if (wgpuAdapterHasFeature(adapter,
            (WGPUFeatureName)WGPUNativeFeature_ShaderF64))
        feats[nfeat++] = (WGPUFeatureName)WGPUNativeFeature_ShaderF64;
    // v4.1.0 / wgpu-native v29.0.1.1: WGPUNativeFeature_SpirvShaderPassthrough is
    // GONE from wgpu.h. It was not renamed — its slot 0x00030017 is now
    // ClearTexture, and the intended replacement
    // (WGPUNativeFeature_PassthroughShaders = 0x00030036) ships COMMENTED OUT with
    // a "requires wgpu.h api change" TODO. So raw SPIR-V passthrough does not exist
    // upstream at this version, at any spelling.
    //
    // Nothing of mabda's is lost: passthrough was the naga-BYPASS escape hatch, and
    // it already returned 0 from wgpuAdapterHasFeature on RADV/Cezanne. The paths
    // that matter are untouched — WGPUInstanceFeatureName_ShaderSourceSPIRV (the
    // ordinary naga SPIR-V route) and WGPUNativeFeature_ShaderF64, which is still
    // 0x0003001D, byte-identical to v29.0.0.0.
    //
    // Re-add a request here if upstream un-comments PassthroughShaders.
    desc.requiredFeatures = feats;
    desc.requiredFeatureCount = nfeat;
    WGPURequestDeviceCallbackInfo cb = {
        .mode = WGPUCallbackMode_AllowSpontaneous,
        .callback = c_on_device,
        .userdata1 = result_ptr,
    };
    wgpuAdapterRequestDevice(adapter, &desc, cb);
    return *result_ptr;
}

// Buffer map sync — bridges async wgpuBufferMapAsync + wgpuDevicePoll into one
// blocking call. Natural 6-arg C signature, called from Cyrius via fncall6: the
// old "fncall6 into extern-C misbehaves" belief was a %fs/TLS misdiagnosis, put
// to rest in cyrius 6.3.26 (this glibc C launcher owns %fs, so every fncallN
// into wgpu is sound). See cyrius docs/ffi/fncall-abi.md.
void wgpu_shim_buffer_map(WGPUDevice device, WGPUBuffer buffer, uint32_t mode,
                          size_t offset, size_t size, long* status_ptr) {
    WGPUBufferMapCallbackInfo cb = {
        .mode = WGPUCallbackMode_AllowSpontaneous,
        .callback = c_on_buffer_mapped,
        .userdata1 = status_ptr,
    };
    wgpuBufferMapAsync(buffer, (WGPUMapMode)mode, offset, size, cb);
    wgpuDevicePoll(device, true, NULL);
}

// Device poll simplified
WGPUBool wgpu_shim_device_poll(WGPUDevice device, WGPUBool wait) {
    return wgpuDevicePoll(device, wait, NULL);
}

// Create command encoder with a valid (possibly NULL) descriptor. wgpu-native
// v29 is sensitive to uninitialized struct padding reaching its dispatch layer,
// so we build the descriptor locally from C where the ABI matches exactly.
WGPUCommandEncoder wgpu_shim_create_command_encoder(WGPUDevice device, const char* label) {
    WGPUCommandEncoderDescriptor desc = WGPU_COMMAND_ENCODER_DESCRIPTOR_INIT;
    if (label) {
        desc.label.data = label;
        desc.label.length = strlen(label);
    }
    return wgpuDeviceCreateCommandEncoder(device, &desc);
}

// Finish the command encoder with a zero-inited descriptor.
WGPUCommandBuffer wgpu_shim_command_encoder_finish(WGPUCommandEncoder encoder, const char* label) {
    WGPUCommandBufferDescriptor desc = WGPU_COMMAND_BUFFER_DESCRIPTOR_INIT;
    if (label) {
        desc.label.data = label;
        desc.label.length = strlen(label);
    }
    return wgpuCommandEncoderFinish(encoder, &desc);
}

// Submit a single command buffer. Simpler than passing an array handle from
// Cyrius when the count is always small.
void wgpu_shim_queue_submit_one(WGPUQueue queue, WGPUCommandBuffer cmd) {
    wgpuQueueSubmit(queue, 1, &cmd);
}

// wgpuCommandEncoderCopyBufferToBuffer, wgpuQueueWriteTexture, and
// wgpuCommandEncoderResolveQuerySet are all 6-arg all-scalar (handle/u32/u64)
// C entry points — no struct-by-value, no float — so Cyrius calls them
// DIRECTLY via fncall6 (slots 28/48/42). The former wgpu_shim_* struct-packing
// wrappers were removed once cyrius 6.3.26 disproved the "fncall6 into extern-C
// wgpu is unreliable" folklore (it was a %fs/TLS init problem the C launcher
// already satisfies; see cyrius docs/ffi/fncall-abi.md). begin_render_pass
// (slot 58) and copy_texture_to_buffer (slot 64) below KEEP their shims — those
// genuinely pass struct-by-value descriptors, which fncallN cannot marshal.

// BeginRenderPass takes a WGPURenderPassDescriptor by pointer — the descriptor
// itself is built from a mabda-provided packed args struct so the Cyrius side
// does not have to assemble nested struct-by-value fields itself. The packed
// color-attachment array Cyrius supplies is already v29-layout-compatible (72
// bytes per entry, matching WGPURenderPassColorAttachment) so we pass it
// straight through without repacking. See src/render_pass.cyr for the builder.
//
// WgpuBeginPassArgs layout (40 bytes, Cyrius-side matches):
//   +0:  color_attachments     (const WGPURenderPassColorAttachment*)
//   +8:  color_attachment_count (u64)
//   +16: depth_stencil          (const WGPURenderPassDepthStencilAttachment* or NULL)
//   +24: timestamp_writes       (const WGPUPassTimestampWrites* or NULL)
//   +32: label                  (const char* or NULL)
typedef struct {
    const WGPURenderPassColorAttachment* color_attachments;
    uint64_t                             color_attachment_count;
    const WGPURenderPassDepthStencilAttachment* depth_stencil;
    const WGPUPassTimestampWrites*       timestamp_writes;
    const char*                          label;
} WgpuBeginPassArgs;

WGPURenderPassEncoder wgpu_shim_command_encoder_begin_render_pass(
    WGPUCommandEncoder encoder, const WgpuBeginPassArgs* args) {
    WGPURenderPassDescriptor desc = {0};
    if (args->label) {
        desc.label.data = args->label;
        desc.label.length = strlen(args->label);
    }
    desc.colorAttachmentCount = (size_t)args->color_attachment_count;
    desc.colorAttachments = args->color_attachments;
    desc.depthStencilAttachment = args->depth_stencil;
    desc.timestampWrites = args->timestamp_writes;
    return wgpuCommandEncoderBeginRenderPass(encoder, &desc);
}

// CopyTextureToBuffer takes WGPUTexelCopyTextureInfo (src) +
// WGPUTexelCopyBufferInfo (dst) + WGPUExtent3D (copySize), all
// struct-by-value in Cyrius-terms. We pack everything flat so the Cyrius side
// only has to fncall2(_fp, encoder, &args). Layout pinned explicitly — if
// webgpu.h v29 grows a new field, update here and in Cyrius in lockstep.
//
// WgpuCopyTexToBufArgs layout (72 bytes, Cyrius-side matches):
//   +0:  src_texture        (WGPUTexture)
//   +8:  src_mip_level      (u32)
//   +12: src_origin_x       (u32)
//   +16: src_origin_y       (u32)
//   +20: src_origin_z       (u32)
//   +24: aspect             (u32, WGPUTextureAspect_All = 0)
//   +28: (pad 4)
//   +32: dst_buffer         (WGPUBuffer)
//   +40: dst_offset         (u64)
//   +48: dst_bytes_per_row  (u32)
//   +52: dst_rows_per_image (u32)
//   +56: copy_w             (u32)
//   +60: copy_h             (u32)
//   +64: copy_depth         (u32)
//   +68: (pad 4)
typedef struct {
    WGPUTexture src_texture;
    uint32_t    src_mip_level;
    uint32_t    src_origin_x;
    uint32_t    src_origin_y;
    uint32_t    src_origin_z;
    uint32_t    aspect;
    uint32_t    _pad0;
    WGPUBuffer  dst_buffer;
    uint64_t    dst_offset;
    uint32_t    dst_bytes_per_row;
    uint32_t    dst_rows_per_image;
    uint32_t    copy_w;
    uint32_t    copy_h;
    uint32_t    copy_depth;
    uint32_t    _pad1;
} WgpuCopyTexToBufArgs;

void wgpu_shim_command_encoder_copy_texture_to_buffer(
    WGPUCommandEncoder encoder, const WgpuCopyTexToBufArgs* args) {
    WGPUTexelCopyTextureInfo src = {0};
    src.texture  = args->src_texture;
    src.mipLevel = args->src_mip_level;
    src.origin.x = args->src_origin_x;
    src.origin.y = args->src_origin_y;
    src.origin.z = args->src_origin_z;
    src.aspect   = (WGPUTextureAspect)args->aspect;

    WGPUTexelCopyBufferInfo dst = {0};
    dst.layout.offset       = args->dst_offset;
    dst.layout.bytesPerRow  = args->dst_bytes_per_row;
    dst.layout.rowsPerImage = args->dst_rows_per_image;
    dst.buffer              = args->dst_buffer;

    WGPUExtent3D size = { args->copy_w, args->copy_h, args->copy_depth };

    wgpuCommandEncoderCopyTextureToBuffer(encoder, &src, &dst, &size);
}

// Queue timestamp period returns a float. Cyrius's fncall1 always reads rax
// as i64, so we reinterpret the f32 as its i32 bit-pattern (zero-extended).
// Cyrius can decode with f64_from(f32_to_f64_bits(load_as_f32(bits))) if it
// ever needs the actual value, but for feature detection just checking that
// the period is non-zero is sufficient.
long wgpu_shim_get_timestamp_period_bits(WGPUQueue queue) {
    // wgpu-native v29 doesn't expose wgpuQueueGetTimestampPeriod directly on
    // all backends. Return 1.0f as a safe default when the entry is missing.
    // The Cyrius wrapper treats a zero return as "feature unavailable".
    (void)queue;
    float one = 1.0f;
    uint32_t bits;
    memcpy(&bits, &one, 4);
    return (long)bits;
}

// v3.2 Phase F.8a: create a shader module from pre-compiled SPIR-V words via the
// passthrough ESCAPE HATCH (wgpuDeviceCreateShaderModuleSpirV) — raw words bypass
// naga, for SPIR-V naga cannot carry. NOTE (F.8b): this is NOT the f64 path — f64
// rides ShaderF64 + the ordinary naga SPIR-V path. Requires the SpirvShaderPassthrough
// device feature (requested in wgpu_shim_request_device above; unsupported on RADV).
// word_count is the number of 32-bit words. Returns NULL if the feature is absent or
// the module is rejected; the Cyrius wrapper surfaces that as a 0 handle (fail-loud).
WGPUShaderModule wgpu_shim_create_shader_module_spirv(WGPUDevice device,
                                                      const uint32_t* words,
                                                      long word_count) {
    WGPUShaderModuleDescriptorSpirV desc = {0};
    desc.label.data = NULL;
    desc.label.length = 0;
    desc.sourceSize = (uint32_t)word_count;
    desc.source = words;
    return wgpuDeviceCreateShaderModuleSpirV(device, &desc);
}

// === Function table ===
#define FN_COUNT 67
static void* fn_table[FN_COUNT];

static void build_fn_table(void) {
    int i = 0;
    // Core (0-7)
    fn_table[i++] = (void*)wgpuCreateInstance;                        // 0
    fn_table[i++] = (void*)wgpu_shim_request_adapter;                 // 1 (simplified)
    fn_table[i++] = (void*)wgpu_shim_request_device;                  // 2 (simplified)
    fn_table[i++] = (void*)wgpuDeviceGetQueue;                        // 3
    fn_table[i++] = (void*)wgpuInstanceProcessEvents;                 // 4
    fn_table[i++] = (void*)wgpuInstanceRelease;                       // 5
    fn_table[i++] = (void*)wgpuAdapterRelease;                        // 6
    fn_table[i++] = (void*)wgpuDeviceRelease;                         // 7
    // Buffer (8-15)
    fn_table[i++] = (void*)wgpuDeviceCreateBuffer;                    // 8
    fn_table[i++] = (void*)wgpuQueueWriteBuffer;                      // 9
    fn_table[i++] = (void*)wgpu_shim_buffer_map;                      // 10 (6-arg async->sync bridge)
    fn_table[i++] = (void*)wgpuBufferGetConstMappedRange;             // 11
    fn_table[i++] = (void*)wgpuBufferUnmap;                           // 12
    fn_table[i++] = (void*)wgpuBufferGetSize;                         // 13
    fn_table[i++] = (void*)wgpuBufferDestroy;                         // 14
    fn_table[i++] = (void*)wgpuBufferRelease;                         // 15
    // Shader/Pipeline (16-25)
    fn_table[i++] = (void*)wgpuDeviceCreateShaderModule;              // 16
    fn_table[i++] = (void*)wgpuDeviceCreateComputePipeline;           // 17
    fn_table[i++] = (void*)wgpuDeviceCreateBindGroupLayout;           // 18
    fn_table[i++] = (void*)wgpuDeviceCreatePipelineLayout;            // 19
    fn_table[i++] = (void*)wgpuDeviceCreateBindGroup;                 // 20
    fn_table[i++] = (void*)wgpuShaderModuleRelease;                   // 21
    fn_table[i++] = (void*)wgpuComputePipelineRelease;                // 22
    fn_table[i++] = (void*)wgpuBindGroupRelease;                      // 23
    fn_table[i++] = (void*)wgpuBindGroupLayoutRelease;                // 24
    fn_table[i++] = (void*)wgpuPipelineLayoutRelease;                 // 25
    // Command (26-34)
    fn_table[i++] = (void*)wgpu_shim_create_command_encoder;          // 26 (shim: takes device, label)
    fn_table[i++] = (void*)wgpuCommandEncoderBeginComputePass;        // 27
    fn_table[i++] = (void*)wgpuCommandEncoderCopyBufferToBuffer;      // 28 (direct 6-arg via fncall6)
    fn_table[i++] = (void*)wgpu_shim_command_encoder_finish;          // 29 (shim: takes encoder, label)
    fn_table[i++] = (void*)wgpuComputePassEncoderSetPipeline;         // 30
    fn_table[i++] = (void*)wgpuComputePassEncoderSetBindGroup;        // 31
    fn_table[i++] = (void*)wgpuComputePassEncoderDispatchWorkgroups;  // 32
    fn_table[i++] = (void*)wgpuComputePassEncoderEnd;                 // 33
    fn_table[i++] = (void*)wgpuCommandEncoderRelease;                 // 34
    // Queue (35)
    fn_table[i++] = (void*)wgpu_shim_queue_submit_one;                // 35 (shim: takes queue, single cmd buffer)
    // Adapter (36-38)
    fn_table[i++] = (void*)wgpuAdapterGetInfo;                        // 36
    fn_table[i++] = (void*)wgpuAdapterGetLimits;                      // 37
    fn_table[i++] = (void*)wgpuAdapterHasFeature;                     // 38
    // Extensions (39)
    fn_table[i++] = (void*)wgpu_shim_device_poll;                     // 39
    // Query sets (40-44) — GPU timestamp profiling
    fn_table[i++] = (void*)wgpuDeviceCreateQuerySet;                  // 40
    fn_table[i++] = (void*)wgpuQuerySetRelease;                       // 41
    fn_table[i++] = (void*)wgpuCommandEncoderResolveQuerySet;         // 42 (direct 6-arg via fncall6)
    fn_table[i++] = (void*)wgpu_shim_get_timestamp_period_bits;       // 43
    fn_table[i++] = (void*)wgpuDeviceHasFeature;                      // 44
    // Texture (45-51)
    fn_table[i++] = (void*)wgpuDeviceCreateTexture;                   // 45
    fn_table[i++] = (void*)wgpuTextureCreateView;                     // 46
    fn_table[i++] = (void*)wgpuDeviceCreateSampler;                   // 47
    fn_table[i++] = (void*)wgpuQueueWriteTexture;                     // 48 (direct 6-arg via fncall6)
    fn_table[i++] = (void*)wgpuTextureRelease;                        // 49
    fn_table[i++] = (void*)wgpuTextureViewRelease;                    // 50
    fn_table[i++] = (void*)wgpuSamplerRelease;                        // 51
    // Render pipeline (52-53)
    fn_table[i++] = (void*)wgpuDeviceCreateRenderPipeline;            // 52
    fn_table[i++] = (void*)wgpuRenderPipelineRelease;                 // 53
    // Surface (54-57)
    fn_table[i++] = (void*)wgpuSurfaceConfigure;                      // 54
    fn_table[i++] = (void*)wgpuSurfaceGetCurrentTexture;              // 55
    fn_table[i++] = (void*)wgpuSurfacePresent;                        // 56
    fn_table[i++] = (void*)wgpuSurfaceRelease;                        // 57
    // Render pass execution (58-64) — v2.4.3
    fn_table[i++] = (void*)wgpu_shim_command_encoder_begin_render_pass;   // 58 (shim: packs descriptor)
    fn_table[i++] = (void*)wgpuRenderPassEncoderSetPipeline;              // 59
    fn_table[i++] = (void*)wgpuRenderPassEncoderSetBindGroup;             // 60
    fn_table[i++] = (void*)wgpuRenderPassEncoderDraw;                     // 61
    fn_table[i++] = (void*)wgpuRenderPassEncoderEnd;                      // 62
    fn_table[i++] = (void*)wgpuRenderPassEncoderRelease;                  // 63
    fn_table[i++] = (void*)wgpu_shim_command_encoder_copy_texture_to_buffer;  // 64 (shim: packs src/dst/size)
    // v3.2 TS.5 — wgpu sample render path: the textured draw fetches the
    // pipeline's auto-generated BGL to build the (texture, sampler) bind group.
    fn_table[i++] = (void*)wgpuRenderPipelineGetBindGroupLayout;          // 65
    fn_table[i++] = (void*)wgpu_shim_create_shader_module_spirv;          // 66 (shim: raw-SPIR-V passthrough)
}

// Pre-initialize GPU context in C (before Cyrius code runs)
// This avoids dlopen issues from TEXTREL in Cyrius .o code
typedef struct {
    WGPUInstance instance;
    WGPUAdapter adapter;
    WGPUDevice device;
    WGPUQueue queue;
} WgpuPreinit;

static WgpuPreinit preinit;

static int preinit_gpu(void) {
    // Force Vulkan on Linux — wgpu-native's default (InstanceBackend_All)
    // tries GLES too, and Mesa's EGL init path crashes on hosts without
    // a live DISPLAY / Wayland socket (verified on RADV Mesa 26.0, kernel
    // 6.18). Vulkan-only keeps the path headless-safe and deterministic.
    WGPUInstanceExtras extras = {0};
    extras.chain.sType = (WGPUSType)WGPUSType_InstanceExtras;
    extras.backends = WGPUInstanceBackend_Vulkan;

    // v3.2 Phase S — request the SPIR-V shader-source instance feature so
    // gpu_shader_module_create_spirv works. wgpu-native v29 rejects
    // WGPUShaderSourceSPIRV unless the instance was built with this feature;
    // without it, every SPIR-V createShaderModule returns NULL (the fail-loud
    // path mabda surfaces as a 0 module). WGSL is unaffected. Consumers that
    // copy this launcher must carry this edit to use SPIR-V — see the migration
    // note in CHANGELOG / docs/archive/proposals/v3.2-spirv-ingestion-wgpu.md.
    static const WGPUInstanceFeatureName k_required_features[] = {
        WGPUInstanceFeatureName_ShaderSourceSPIRV,
    };

    WGPUInstanceDescriptor desc = {0};
    desc.nextInChain = (const WGPUChainedStruct*)&extras;
    desc.requiredFeatureCount = 1;
    desc.requiredFeatures = k_required_features;

    preinit.instance = wgpuCreateInstance(&desc);
    if (!preinit.instance) return 0;

    long adapter_ptr = 0;
    wgpu_shim_request_adapter(preinit.instance, 2, &adapter_ptr); // HIGH_PERFORMANCE
    preinit.adapter = (WGPUAdapter)adapter_ptr;
    if (!preinit.adapter) return 0;

    long device_ptr = 0;
    wgpu_shim_request_device(preinit.adapter, &device_ptr);
    preinit.device = (WGPUDevice)device_ptr;
    if (!preinit.device) return 0;

    preinit.queue = wgpuDeviceGetQueue(preinit.device);
    return 1;
}

extern long mabda_main(long fn_table_ptr, long preinit_ptr);
extern void _cyrius_init(void);
extern void alloc_init(void);
extern long thread_local_use_foreign_tls(void);

int main(void) {
    // Initialize Cyrius globals (enums, global vars), then heap
    // Order matters: _cyrius_init resets global vars, alloc_init must come after
    _cyrius_init();
    alloc_init();

    // v6.3.26: this glibc-hosted launcher owns %fs (glibc's TCB self-pointer +
    // the stack canary at %fs:0x28). Declare foreign TLS so any linked cyrius
    // thread-local user (sigil crypto banking, patra) keeps its slots in a
    // process-global fallback array instead of arch_prctl(ARCH_SET_FS)-clobbering
    // glibc's %fs — which would wipe the canary every stack-protected wgpu callee
    // reads. Must run AFTER _cyrius_init (it resets cyrius globals, including this
    // flag) and before any cyrius thread-local touch. Idempotent no-op when no
    // thread-local user is linked. See cyrius docs/ffi/fncall-abi.md.
    thread_local_use_foreign_tls();

    // Pre-init GPU in C
    int gpu_ok = preinit_gpu();

    build_fn_table();
    long result = mabda_main((long)(void*)fn_table, gpu_ok ? (long)(void*)&preinit : 0);
    return (int)result;
}
