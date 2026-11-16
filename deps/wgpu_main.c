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
    (void)message;
    (void)ud2;
    if (status == WGPURequestAdapterStatus_Success && ud1)
        *(long*)ud1 = (long)adapter;
}

static void c_on_device(WGPURequestDeviceStatus status, WGPUDevice device,
    WGPUStringView message, void* ud1, void* ud2) {
    (void)message;
    (void)ud2;
    if (status == WGPURequestDeviceStatus_Success && ud1)
        *(long*)ud1 = (long)device;
}

static void c_on_buffer_mapped(WGPUMapAsyncStatus status,
    WGPUStringView message, void* ud1, void* ud2) {
    (void)message;
    (void)ud2;
    if (ud1) *(long*)ud1 = (long)status;
}

// === v4.3.0 error channel ===
//
// wgpu-native's default uncaptured-error handler PANICS ("Handling wgpu
// uncaptured errors as fatal by default"), so before 4.3.0 any validation error
// (a bad shader, a pipeline whose layout does not match its WGSL, a bind group
// with the wrong entry count) aborted the host process with exit 134. The device
// is now created with an uncaptured-error callback that RECORDS the error here
// instead; Cyrius reads and clears it through slot 75. Error scopes (slots 73/74)
// attribute an error to the exact call that raised it.

// Copy a WGPUStringView into a NUL-terminated buffer of `cap` bytes (cap >= 1).
// A view whose length is WGPU_STRLEN is itself NUL-terminated.
static void copy_string_view(char* dst, long cap, WGPUStringView sv) {
    if (!dst || cap <= 0) return;
    dst[0] = 0;
    if (!sv.data) return;
    size_t n = sv.length;
    if (n == WGPU_STRLEN) n = strlen(sv.data);
    if (n > (size_t)(cap - 1)) n = (size_t)(cap - 1);
    memcpy(dst, sv.data, n);
    dst[n] = 0;
}

#define MABDA_ERR_MSG_CAP 2048
static long g_uncaptured_type = 0;    // latest WGPUErrorType since the last take; 0 = none
static long g_uncaptured_count = 0;   // errors since the last take
static char g_uncaptured_msg[MABDA_ERR_MSG_CAP];

static void c_on_uncaptured_error(WGPUDevice const* device, WGPUErrorType type,
    WGPUStringView message, void* ud1, void* ud2) {
    (void)device;
    (void)ud1;
    (void)ud2;
    g_uncaptured_type = (long)type;
    g_uncaptured_count++;
    copy_string_view(g_uncaptured_msg, MABDA_ERR_MSG_CAP, message);
}

// Slot 75: return the latest uncaptured error's WGPUErrorType (0 when none
// arrived since the last call), copy its message into buf (cap bytes, NUL-
// terminated; buf may be NULL), store the number of errors since the last call
// in *count_out (may be NULL), and clear all three.
long wgpu_shim_take_uncaptured_error(char* buf, long cap, long* count_out) {
    long type = g_uncaptured_type;
    if (buf && cap > 0) {
        long n = (long)strlen(g_uncaptured_msg);
        if (n > cap - 1) n = cap - 1;
        memcpy(buf, g_uncaptured_msg, (size_t)n);
        buf[n] = 0;
    }
    if (count_out) *count_out = g_uncaptured_count;
    g_uncaptured_type = 0;
    g_uncaptured_count = 0;
    g_uncaptured_msg[0] = 0;
    return type;
}

typedef struct {
    long done;
    long status;    // WGPUPopErrorScopeStatus
    long type;      // WGPUErrorType
    char* buf;
    long cap;
} PopScopeResult;

static void c_on_pop_error_scope(WGPUPopErrorScopeStatus status, WGPUErrorType type,
    WGPUStringView message, void* ud1, void* ud2) {
    (void)ud2;
    PopScopeResult* r = (PopScopeResult*)ud1;
    if (!r) return;
    r->status = (long)status;
    r->type = (long)type;
    copy_string_view(r->buf, r->cap, message);
    r->done = 1;
}

// Slot 74: pop the device's innermost error scope (pushed with slot 73,
// wgpuDevicePushErrorScope) and wait for its result. Returns the WGPUErrorType
// (1 = NoError, 2 = Validation, 3 = OutOfMemory, 4 = Internal, 5 = Unknown) when
// the pop succeeded; the negated WGPUPopErrorScopeStatus when it did not (e.g.
// -3 = Error: the scope stack was empty); -100 if the callback never fired. The
// message (if any) lands in buf. The callback info is passed BY VALUE, hence the
// shim.
long wgpu_shim_pop_error_scope(WGPUDevice device, char* buf, long cap) {
    PopScopeResult r = {0};
    r.buf = buf;
    r.cap = cap;
    if (buf && cap > 0) buf[0] = 0;
    WGPUPopErrorScopeCallbackInfo cb = {
        .mode = WGPUCallbackMode_AllowSpontaneous,
        .callback = c_on_pop_error_scope,
        .userdata1 = &r,
    };
    wgpuDevicePopErrorScope(device, cb);
    for (int i = 0; i < 64 && !r.done; i++) wgpuDevicePoll(device, false, NULL);
    if (!r.done) return -100;
    if (r.status != WGPUPopErrorScopeStatus_Success) return -r.status;
    return r.type;
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
    // GONE from wgpu.h. It was not renamed — its old slot 0x00030017 is left
    // UNASSIGNED (ClearTexture, commented out at 0x00030016 in v29.0.0.0, is now
    // live at 0x00030016; Multiview follows at 0x00030018), and the intended
    // replacement (WGPUNativeFeature_PassthroughShaders = 0x00030036) ships
    // COMMENTED OUT ("requires wgpu.h api change"). So raw SPIR-V passthrough does not
    // exist upstream at this version, at any spelling, and the Cyrius-side gate
    // (gpu_wgpu_spirv_passthrough_supported) answers 0 without probing the old number.
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
    // v4.3.0: record uncaptured errors instead of wgpu-native's default panic
    // (see the error channel above).
    desc.uncapturedErrorCallbackInfo.callback = c_on_uncaptured_error;
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

// The device error scopes are pushed on (set by preinit_gpu). NULL until then.
static WGPUDevice g_scope_device = NULL;

// Finish the command encoder with a zero-inited descriptor.
//
// v4.3.0: an encoding error (a copy with a bad row pitch, a draw with a missing
// bind group, ...) is reported at finish, and the command buffer finish returns
// is then INVALID. wgpu-native treats submitting an invalid command buffer as
// FATAL (handle_error_fatal in wgpuQueueSubmit: it aborts even with an
// uncaptured-error callback installed). So finish runs under its own Validation
// error scope: on an error the message goes to the uncaptured-error record (slot
// 75 reads it), the invalid buffer is released, and NULL is returned. The submit
// shim (slot 35) skips NULL, so an encoding error costs the frame, not the process.
WGPUCommandBuffer wgpu_shim_command_encoder_finish(WGPUCommandEncoder encoder, const char* label) {
    WGPUCommandBufferDescriptor desc = WGPU_COMMAND_BUFFER_DESCRIPTOR_INIT;
    if (label) {
        desc.label.data = label;
        desc.label.length = strlen(label);
    }
    if (!g_scope_device) return wgpuCommandEncoderFinish(encoder, &desc);
    wgpuDevicePushErrorScope(g_scope_device, WGPUErrorFilter_Validation);
    WGPUCommandBuffer cmd = wgpuCommandEncoderFinish(encoder, &desc);
    char msg[MABDA_ERR_MSG_CAP];
    long t = wgpu_shim_pop_error_scope(g_scope_device, msg, MABDA_ERR_MSG_CAP);
    if (t > WGPUErrorType_NoError) {
        g_uncaptured_type = t;
        g_uncaptured_count++;
        memcpy(g_uncaptured_msg, msg, MABDA_ERR_MSG_CAP);
        if (cmd) wgpuCommandBufferRelease(cmd);
        return NULL;
    }
    return cmd;
}

// Submit a single command buffer. Simpler than passing an array handle from
// Cyrius when the count is always small.
//
// v4.3.0 (L17): the shim now RELEASES the command buffer after submitting it.
// Before, nothing released a submitted WGPUCommandBuffer (no slot could), so
// every submit leaked ~1 KB of native heap: a 60 fps render loop grew ~200 MB an
// hour from this alone. wgpu keeps its own reference until the GPU is done with
// the work, and a WebGPU command buffer cannot be submitted twice, so the
// caller's handle is dead after this call. Every submit site in mabda goes
// through this slot; slot 76 (wgpuCommandBufferRelease) is for a buffer that is
// finished but never submitted (an error path).
//
// A NULL cmd (the finish shim refused an invalid buffer) submits nothing.
void wgpu_shim_queue_submit_one(WGPUQueue queue, WGPUCommandBuffer cmd) {
    if (!cmd) return;
    wgpuQueueSubmit(queue, 1, &cmd);
    wgpuCommandBufferRelease(cmd);
}

// === v4.3.0 render-tier shims ===

// Slot 70: wgpuRenderPassEncoderSetViewport takes six `float`s, which fncallN
// cannot pass (they go in xmm registers). Cyrius writes them as f32 into a
// 24-byte block: x, y, width, height, minDepth, maxDepth.
void wgpu_shim_render_pass_set_viewport(WGPURenderPassEncoder pass, const float* v) {
    wgpuRenderPassEncoderSetViewport(pass, v[0], v[1], v[2], v[3], v[4], v[5]);
}

// Slot 78 (MB5): create a WGPUSurface from a Wayland wl_display / wl_surface
// pair. The application owns the window and passes the two raw pointers; mabda
// does not link libwayland. The descriptor is a chained struct, hence the shim.
WGPUSurface wgpu_shim_instance_create_surface_wayland(WGPUInstance instance,
                                                      void* wl_display, void* wl_surface) {
    WGPUSurfaceSourceWaylandSurface src = {0};
    src.chain.sType = WGPUSType_SurfaceSourceWaylandSurface;
    src.display = wl_display;
    src.surface = wl_surface;
    WGPUSurfaceDescriptor desc = {0};
    desc.nextInChain = &src.chain;
    return wgpuInstanceCreateSurface(instance, &desc);
}

// Slot 79 (MB5): wgpuSurfaceGetCapabilities fills arrays wgpu owns, and freeing
// them (wgpuSurfaceCapabilitiesFreeMembers) takes the struct BY VALUE, hence the
// shim. It copies the result into a flat block the caller owns and frees wgpu's.
// Each count is the TRUE count; at most the array's capacity is copied.
//
// WgpuSurfaceCapsOut layout (168 bytes, Cyrius-side matches):
//   +0:   status              (i64: WGPUStatus, 1 = Success)
//   +8:   usages              (u64: WGPUTextureUsage bits)
//   +16:  format_count        (u64)
//   +24:  formats[16]         (u32 each: WGPUTextureFormat)
//   +88:  present_mode_count  (u64)
//   +96:  present_modes[8]    (u32 each: WGPUPresentMode)
//   +128: alpha_mode_count    (u64)
//   +136: alpha_modes[8]      (u32 each: WGPUCompositeAlphaMode)
typedef struct {
    int64_t  status;
    uint64_t usages;
    uint64_t format_count;
    uint32_t formats[16];
    uint64_t present_mode_count;
    uint32_t present_modes[8];
    uint64_t alpha_mode_count;
    uint32_t alpha_modes[8];
} WgpuSurfaceCapsOut;

long wgpu_shim_surface_get_capabilities(WGPUSurface surface, WGPUAdapter adapter,
                                        WgpuSurfaceCapsOut* out) {
    memset(out, 0, sizeof(*out));
    WGPUSurfaceCapabilities caps = {0};
    WGPUStatus st = wgpuSurfaceGetCapabilities(surface, adapter, &caps);
    out->status = (int64_t)st;
    if (st != WGPUStatus_Success) return (long)st;
    out->usages = (uint64_t)caps.usages;
    out->format_count = caps.formatCount;
    for (size_t i = 0; i < caps.formatCount && i < 16; i++)
        out->formats[i] = (uint32_t)caps.formats[i];
    out->present_mode_count = caps.presentModeCount;
    for (size_t i = 0; i < caps.presentModeCount && i < 8; i++)
        out->present_modes[i] = (uint32_t)caps.presentModes[i];
    out->alpha_mode_count = caps.alphaModeCount;
    for (size_t i = 0; i < caps.alphaModeCount && i < 8; i++)
        out->alpha_modes[i] = (uint32_t)caps.alphaModes[i];
    wgpuSurfaceCapabilitiesFreeMembers(caps);
    return (long)st;
}

// Slot 80: every debug-group operation through one slot. Labels are
// WGPUStringView BY VALUE, hence the shim; one shim instead of nine slots.
//   target: 0 = WGPUCommandEncoder, 1 = WGPURenderPassEncoder, 2 = WGPUComputePassEncoder
//   op:     0 = push group (label), 1 = pop group, 2 = insert marker (label)
// Returns 0, or -1 for an unknown target/op (nothing is called).
long wgpu_shim_debug_op(long target, void* encoder, long op, const char* label) {
    WGPUStringView sv = { label, label ? strlen(label) : 0 };
    if (!encoder) return -1;
    if (target == 0) {
        WGPUCommandEncoder e = (WGPUCommandEncoder)encoder;
        if (op == 0) { wgpuCommandEncoderPushDebugGroup(e, sv); return 0; }
        if (op == 1) { wgpuCommandEncoderPopDebugGroup(e); return 0; }
        if (op == 2) { wgpuCommandEncoderInsertDebugMarker(e, sv); return 0; }
    } else if (target == 1) {
        WGPURenderPassEncoder p = (WGPURenderPassEncoder)encoder;
        if (op == 0) { wgpuRenderPassEncoderPushDebugGroup(p, sv); return 0; }
        if (op == 1) { wgpuRenderPassEncoderPopDebugGroup(p); return 0; }
        if (op == 2) { wgpuRenderPassEncoderInsertDebugMarker(p, sv); return 0; }
    } else if (target == 2) {
        WGPUComputePassEncoder p = (WGPUComputePassEncoder)encoder;
        if (op == 0) { wgpuComputePassEncoderPushDebugGroup(p, sv); return 0; }
        if (op == 1) { wgpuComputePassEncoderPopDebugGroup(p); return 0; }
        if (op == 2) { wgpuComputePassEncoderInsertDebugMarker(p, sv); return 0; }
    }
    return -1;
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
// v4.3.0: 67 -> 81 slots, in ONE revision covering mabda's whole render tier
// (MB1-MB5) plus the L17 command-buffer release, so consumers rebuild their
// launcher copy once. A consumer that keeps an older launcher still runs: the
// launcher tells mabda its slot count (mabda_wgpu_launcher_slots, below), and
// mabda's wrappers for slots it does not have return an error instead of
// calling through a pointer past the end of this table.
// v4.3.1: 81 -> 83. The 4.3.0 revision missed two entry points the compute tier
// (MB4) needs: the compute pass encoder's release (without it every compute pass
// leaked its encoder) and the compute pipeline's bind group layout (the auto
// layout gpu_compute_dispatch binds through).
#define FN_COUNT 83
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
    fn_table[i++] = (void*)wgpu_shim_queue_submit_one;                // 35 (shim: single cmd buffer; releases it after submit, 4.3.0)
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
    // v4.3.0 — MB1 draw-time calls (67-72). Direct: scalar / pointer arguments.
    fn_table[i++] = (void*)wgpuRenderPassEncoderSetVertexBuffer;          // 67
    fn_table[i++] = (void*)wgpuRenderPassEncoderSetIndexBuffer;           // 68
    fn_table[i++] = (void*)wgpuRenderPassEncoderDrawIndexed;              // 69
    fn_table[i++] = (void*)wgpu_shim_render_pass_set_viewport;            // 70 (shim: six floats)
    fn_table[i++] = (void*)wgpuRenderPassEncoderSetScissorRect;           // 71
    fn_table[i++] = (void*)wgpuRenderPassEncoderSetBlendConstant;         // 72 (WGPUColor by pointer)
    // v4.3.0 — error channel (73-75).
    fn_table[i++] = (void*)wgpuDevicePushErrorScope;                      // 73
    fn_table[i++] = (void*)wgpu_shim_pop_error_scope;                     // 74 (shim: callback info by value)
    fn_table[i++] = (void*)wgpu_shim_take_uncaptured_error;               // 75 (shim: launcher-recorded error)
    // v4.3.0 — L17: release a finished command buffer that was never submitted.
    fn_table[i++] = (void*)wgpuCommandBufferRelease;                      // 76
    // v4.3.0 — MB3: texture-to-texture copy (all arguments by pointer).
    fn_table[i++] = (void*)wgpuCommandEncoderCopyTextureToTexture;        // 77
    // v4.3.0 — MB5: Wayland surface + capabilities.
    fn_table[i++] = (void*)wgpu_shim_instance_create_surface_wayland;     // 78 (shim: chained descriptor)
    fn_table[i++] = (void*)wgpu_shim_surface_get_capabilities;            // 79 (shim: FreeMembers by value)
    // v4.3.0 — debug groups / markers on every encoder kind (roadmap backlog).
    fn_table[i++] = (void*)wgpu_shim_debug_op;                            // 80 (shim: labels by value)
    // v4.3.1 — MB4 compute tier.
    fn_table[i++] = (void*)wgpuComputePassEncoderRelease;                 // 81
    fn_table[i++] = (void*)wgpuComputePipelineGetBindGroupLayout;         // 82
    // A slot added above without bumping FN_COUNT (or the reverse) would hand
    // mabda a table whose tail is NULL or overruns: refuse to start instead.
    if (i != FN_COUNT) {
        fprintf(stderr, "wgpu_main.c: fn_table has %d entries, FN_COUNT is %d\n", i, FN_COUNT);
        abort();
    }
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
    // v29 webgpu.h declares WGPUInstanceDescriptor.nextInChain as a NON-const
    // `WGPUChainedStruct *` (the input-struct const was dropped upstream). The old
    // `(const WGPUChainedStruct*)&extras` cast re-added a const the field cannot hold,
    // so gcc warned (-Wdiscarded-qualifiers) on every build. Point at the embedded
    // header by name instead: `extras` is a mutable local, `extras.chain` is its first
    // member, and wgpuCreateInstance only reads the chain — no cast, no const to lose.
    desc.nextInChain = &extras.chain;
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
    g_scope_device = preinit.device;
    return 1;
}

extern long mabda_main(long fn_table_ptr, long preinit_ptr);
extern void _cyrius_init(void);
extern void alloc_init(void);
extern long thread_local_use_foreign_tls(void);
// v4.3.0: tells mabda how many fn_table slots this launcher has. WEAK, so this
// launcher still links against a mabda older than 4.3.0 (which lacks the
// symbol); a mabda that never hears from it treats the launcher as the 67-slot
// pre-4.3.0 table and refuses the newer calls with an error.
extern long mabda_wgpu_launcher_slots(long slot_count) __attribute__((weak));

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
    // After _cyrius_init (it resets cyrius globals, including the slot count).
    if (mabda_wgpu_launcher_slots) mabda_wgpu_launcher_slots(FN_COUNT);
    long result = mabda_main((long)(void*)fn_table, gpu_ok ? (long)(void*)&preinit : 0);
    return (int)result;
}
