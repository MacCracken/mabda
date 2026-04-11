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
    fprintf(stderr, "c_on_adapter: status=%d adapter=%p msg='%.*s'\n",
        status, (void*)adapter, (int)message.length, message.data);
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
    fprintf(stderr, "shim_request_adapter: inst=%p pref=%ld result_ptr=%p\n",
        (void*)instance, power_pref, (void*)result_ptr);
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
    WGPURequestDeviceCallbackInfo cb = {
        .mode = WGPUCallbackMode_AllowSpontaneous,
        .callback = c_on_device,
        .userdata1 = result_ptr,
    };
    wgpuAdapterRequestDevice(adapter, &desc, cb);
    return *result_ptr;
}

// Buffer map sync: (device, buffer, mode, offset, size, status_ptr) → status in *status_ptr
void wgpu_shim_buffer_map(WGPUDevice device, WGPUBuffer buffer,
    WGPUMapMode mode, size_t offset, size_t size, long* status_ptr) {
    WGPUBufferMapCallbackInfo cb = {
        .mode = WGPUCallbackMode_AllowSpontaneous,
        .callback = c_on_buffer_mapped,
        .userdata1 = status_ptr,
    };
    wgpuBufferMapAsync(buffer, mode, offset, size, cb);
    wgpuDevicePoll(device, true, NULL);
}

// Device poll simplified
WGPUBool wgpu_shim_device_poll(WGPUDevice device, WGPUBool wait) {
    return wgpuDevicePoll(device, wait, NULL);
}

// === Function table ===
#define FN_COUNT 44
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
    fn_table[i++] = (void*)wgpu_shim_buffer_map;                      // 10 (simplified)
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
    fn_table[i++] = (void*)wgpuDeviceCreateCommandEncoder;            // 26
    fn_table[i++] = (void*)wgpuCommandEncoderBeginComputePass;        // 27
    fn_table[i++] = (void*)wgpuCommandEncoderCopyBufferToBuffer;      // 28
    fn_table[i++] = (void*)wgpuCommandEncoderFinish;                  // 29
    fn_table[i++] = (void*)wgpuComputePassEncoderSetPipeline;         // 30
    fn_table[i++] = (void*)wgpuComputePassEncoderSetBindGroup;        // 31
    fn_table[i++] = (void*)wgpuComputePassEncoderDispatchWorkgroups;  // 32
    fn_table[i++] = (void*)wgpuComputePassEncoderEnd;                 // 33
    fn_table[i++] = (void*)wgpuCommandEncoderRelease;                 // 34
    // Queue (35)
    fn_table[i++] = (void*)wgpuQueueSubmit;                           // 35
    // Adapter (36-38)
    fn_table[i++] = (void*)wgpuAdapterGetInfo;                        // 36
    fn_table[i++] = (void*)wgpuAdapterGetLimits;                      // 37
    fn_table[i++] = (void*)wgpuAdapterHasFeature;                     // 38
    // Extensions (39)
    fn_table[i++] = (void*)wgpu_shim_device_poll;                     // 39
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
    preinit.instance = wgpuCreateInstance(NULL);
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

int main(void) {
    // Pre-init GPU in C (dlopen works here, no TEXTREL)
    int gpu_ok = preinit_gpu();

    build_fn_table();
    long result = mabda_main((long)(void*)fn_table, gpu_ok ? (long)(void*)&preinit : 0);
    return (int)result;
}
