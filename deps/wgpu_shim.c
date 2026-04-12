// wgpu_shim.c — C bridge for wgpu-native, loadable by Cyrius
//
// This gets compiled as a shared library with wgpu-native statically linked.
// All wgpu functions are re-exported, plus wrapper functions for those that
// pass structs by value (which Cyrius's fncall cannot handle).
//
// Build: gcc -shared -fPIC -o libwgpu_shim.so wgpu_shim.c \
//        wgpu-native/lib/libwgpu_native.a -Iwgpu-native/include \
//        -lpthread -ldl -lm
#include "wgpu-native/include/webgpu/webgpu.h"
#include "wgpu-native/include/webgpu/wgpu.h"
#include <stdint.h>
#include <string.h>

// === Shim wrappers for by-value struct args ===

// Adapter request: callbackInfo passed by pointer instead of by value
WGPUFuture wgpu_shim_instance_request_adapter(
    WGPUInstance instance,
    WGPURequestAdapterOptions const *options,
    WGPURequestAdapterCallbackInfo const *callbackInfo
) {
    return wgpuInstanceRequestAdapter(instance, options, *callbackInfo);
}

// Device request: callbackInfo by pointer
WGPUFuture wgpu_shim_adapter_request_device(
    WGPUAdapter adapter,
    WGPUDeviceDescriptor const *descriptor,
    WGPURequestDeviceCallbackInfo const *callbackInfo
) {
    return wgpuAdapterRequestDevice(adapter, descriptor, *callbackInfo);
}

// Buffer map async: callbackInfo by pointer
WGPUFuture wgpu_shim_buffer_map_async(
    WGPUBuffer buffer,
    WGPUMapMode mode,
    size_t offset,
    size_t size,
    WGPUBufferMapCallbackInfo const *callbackInfo
) {
    return wgpuBufferMapAsync(buffer, mode, offset, size, *callbackInfo);
}

// Device poll simplified (no submission index)
WGPUBool wgpu_shim_device_poll(WGPUDevice device, WGPUBool wait) {
    return wgpuDevicePoll(device, wait, NULL);
}

// === Function table for Cyrius ===
// Instead of Cyrius calling dlopen (which needs libc init), we provide
// a single function that returns a table of all function pointers.
// Cyrius loads this .so via its bootstrapped syslib, calls this function
// once, and gets all pointers.

typedef struct {
    // Core lifecycle
    void* createInstance;
    void* shimInstanceRequestAdapter;
    void* shimAdapterRequestDevice;
    void* deviceGetQueue;
    void* instanceProcessEvents;
    void* instanceRelease;
    void* adapterRelease;
    void* deviceRelease;
    // Buffer
    void* deviceCreateBuffer;
    void* queueWriteBuffer;
    void* shimBufferMapAsync;
    void* bufferGetConstMappedRange;
    void* bufferUnmap;
    void* bufferGetSize;
    void* bufferDestroy;
    void* bufferRelease;
    // Shader/Pipeline
    void* deviceCreateShaderModule;
    void* deviceCreateComputePipeline;
    void* deviceCreateBindGroupLayout;
    void* deviceCreatePipelineLayout;
    void* deviceCreateBindGroup;
    void* shaderModuleRelease;
    void* computePipelineRelease;
    void* bindGroupRelease;
    void* bindGroupLayoutRelease;
    void* pipelineLayoutRelease;
    // Command encoding
    void* deviceCreateCommandEncoder;
    void* commandEncoderBeginComputePass;
    void* commandEncoderCopyBufferToBuffer;
    void* commandEncoderFinish;
    void* computePassEncoderSetPipeline;
    void* computePassEncoderSetBindGroup;
    void* computePassEncoderDispatchWorkgroups;
    void* computePassEncoderEnd;
    void* commandEncoderRelease;
    // Queue
    void* queueSubmit;
    // Adapter info
    void* adapterGetInfo;
    void* adapterGetLimits;
    void* adapterHasFeature;
    // wgpu extensions
    void* shimDevicePoll;
} WgpuFnTable;

static WgpuFnTable fn_table;

// Returns pointer to the function table (41 entries × 8 bytes = 328 bytes)
void* wgpu_shim_get_fn_table(void) {
    fn_table.createInstance                  = (void*)wgpuCreateInstance;
    fn_table.shimInstanceRequestAdapter     = (void*)wgpu_shim_instance_request_adapter;
    fn_table.shimAdapterRequestDevice       = (void*)wgpu_shim_adapter_request_device;
    fn_table.deviceGetQueue                 = (void*)wgpuDeviceGetQueue;
    fn_table.instanceProcessEvents          = (void*)wgpuInstanceProcessEvents;
    fn_table.instanceRelease                = (void*)wgpuInstanceRelease;
    fn_table.adapterRelease                 = (void*)wgpuAdapterRelease;
    fn_table.deviceRelease                  = (void*)wgpuDeviceRelease;
    // Buffer
    fn_table.deviceCreateBuffer             = (void*)wgpuDeviceCreateBuffer;
    fn_table.queueWriteBuffer               = (void*)wgpuQueueWriteBuffer;
    fn_table.shimBufferMapAsync             = (void*)wgpu_shim_buffer_map_async;
    fn_table.bufferGetConstMappedRange      = (void*)wgpuBufferGetConstMappedRange;
    fn_table.bufferUnmap                    = (void*)wgpuBufferUnmap;
    fn_table.bufferGetSize                  = (void*)wgpuBufferGetSize;
    fn_table.bufferDestroy                  = (void*)wgpuBufferDestroy;
    fn_table.bufferRelease                  = (void*)wgpuBufferRelease;
    // Shader/Pipeline
    fn_table.deviceCreateShaderModule       = (void*)wgpuDeviceCreateShaderModule;
    fn_table.deviceCreateComputePipeline    = (void*)wgpuDeviceCreateComputePipeline;
    fn_table.deviceCreateBindGroupLayout    = (void*)wgpuDeviceCreateBindGroupLayout;
    fn_table.deviceCreatePipelineLayout     = (void*)wgpuDeviceCreatePipelineLayout;
    fn_table.deviceCreateBindGroup          = (void*)wgpuDeviceCreateBindGroup;
    fn_table.shaderModuleRelease            = (void*)wgpuShaderModuleRelease;
    fn_table.computePipelineRelease         = (void*)wgpuComputePipelineRelease;
    fn_table.bindGroupRelease               = (void*)wgpuBindGroupRelease;
    fn_table.bindGroupLayoutRelease         = (void*)wgpuBindGroupLayoutRelease;
    fn_table.pipelineLayoutRelease          = (void*)wgpuPipelineLayoutRelease;
    // Command
    fn_table.deviceCreateCommandEncoder     = (void*)wgpuDeviceCreateCommandEncoder;
    fn_table.commandEncoderBeginComputePass = (void*)wgpuCommandEncoderBeginComputePass;
    fn_table.commandEncoderCopyBufferToBuffer = (void*)wgpuCommandEncoderCopyBufferToBuffer;
    fn_table.commandEncoderFinish           = (void*)wgpuCommandEncoderFinish;
    fn_table.computePassEncoderSetPipeline  = (void*)wgpuComputePassEncoderSetPipeline;
    fn_table.computePassEncoderSetBindGroup = (void*)wgpuComputePassEncoderSetBindGroup;
    fn_table.computePassEncoderDispatchWorkgroups = (void*)wgpuComputePassEncoderDispatchWorkgroups;
    fn_table.computePassEncoderEnd          = (void*)wgpuComputePassEncoderEnd;
    fn_table.commandEncoderRelease          = (void*)wgpuCommandEncoderRelease;
    // Queue
    fn_table.queueSubmit                    = (void*)wgpuQueueSubmit;
    // Adapter
    fn_table.adapterGetInfo                 = (void*)wgpuAdapterGetInfo;
    fn_table.adapterGetLimits               = (void*)wgpuAdapterGetLimits;
    fn_table.adapterHasFeature              = (void*)wgpuAdapterHasFeature;
    // Extensions
    fn_table.shimDevicePoll                 = (void*)wgpu_shim_device_poll;

    return &fn_table;
}
