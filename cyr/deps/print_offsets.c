#include <stdio.h>
#include <stddef.h>
#include "wgpu-native/include/webgpu/webgpu.h"
#include "wgpu-native/include/webgpu/wgpu.h"

#define P(type, field) printf("  %-30s +%-3zu (%zu bytes)\n", #field, offsetof(type, field), sizeof(((type*)0)->field))
#define S(type) printf("\n%s: %zu bytes\n", #type, sizeof(type))

int main() {
    printf("=== POINTER/HANDLE SIZES ===\n");
    printf("sizeof(WGPUInstance):       %zu\n", sizeof(WGPUInstance));
    printf("sizeof(WGPUAdapter):        %zu\n", sizeof(WGPUAdapter));
    printf("sizeof(WGPUDevice):         %zu\n", sizeof(WGPUDevice));
    printf("sizeof(WGPUQueue):          %zu\n", sizeof(WGPUQueue));
    printf("sizeof(WGPUBuffer):         %zu\n", sizeof(WGPUBuffer));
    printf("sizeof(WGPUTexture):        %zu\n", sizeof(WGPUTexture));
    printf("sizeof(WGPUShaderModule):   %zu\n", sizeof(WGPUShaderModule));
    printf("sizeof(WGPUComputePipeline):%zu\n", sizeof(WGPUComputePipeline));
    printf("sizeof(WGPURenderPipeline): %zu\n", sizeof(WGPURenderPipeline));

    S(WGPUStringView);
    P(WGPUStringView, data);
    P(WGPUStringView, length);

    S(WGPUInstanceDescriptor);
    P(WGPUInstanceDescriptor, nextInChain);

    S(WGPURequestAdapterOptions);
    P(WGPURequestAdapterOptions, nextInChain);
    P(WGPURequestAdapterOptions, compatibleSurface);
    P(WGPURequestAdapterOptions, powerPreference);
    P(WGPURequestAdapterOptions, backendType);
    P(WGPURequestAdapterOptions, forceFallbackAdapter);

    S(WGPUDeviceDescriptor);
    P(WGPUDeviceDescriptor, nextInChain);
    P(WGPUDeviceDescriptor, label);
    P(WGPUDeviceDescriptor, requiredFeatureCount);
    P(WGPUDeviceDescriptor, requiredFeatures);
    P(WGPUDeviceDescriptor, requiredLimits);
    P(WGPUDeviceDescriptor, defaultQueue);
    P(WGPUDeviceDescriptor, deviceLostCallbackInfo);
    P(WGPUDeviceDescriptor, uncapturedErrorCallbackInfo);

    S(WGPUBufferDescriptor);
    P(WGPUBufferDescriptor, nextInChain);
    P(WGPUBufferDescriptor, label);
    P(WGPUBufferDescriptor, usage);
    P(WGPUBufferDescriptor, size);
    P(WGPUBufferDescriptor, mappedAtCreation);

    S(WGPUQueueDescriptor);
    P(WGPUQueueDescriptor, nextInChain);
    P(WGPUQueueDescriptor, label);

    S(WGPUShaderSourceWGSL);
    P(WGPUShaderSourceWGSL, chain);
    P(WGPUShaderSourceWGSL, code);

    S(WGPUShaderModuleDescriptor);
    P(WGPUShaderModuleDescriptor, nextInChain);
    P(WGPUShaderModuleDescriptor, label);

    S(WGPUColor);
    P(WGPUColor, r);
    P(WGPUColor, g);
    P(WGPUColor, b);
    P(WGPUColor, a);

    S(WGPUExtent3D);
    P(WGPUExtent3D, width);
    P(WGPUExtent3D, height);
    P(WGPUExtent3D, depthOrArrayLayers);

    S(WGPULimits);
    P(WGPULimits, maxTextureDimension1D);
    P(WGPULimits, maxTextureDimension2D);
    P(WGPULimits, maxTextureDimension3D);
    P(WGPULimits, maxBufferSize);
    P(WGPULimits, maxBindGroups);
    P(WGPULimits, maxComputeWorkgroupSizeX);
    P(WGPULimits, maxComputeWorkgroupSizeY);
    P(WGPULimits, maxComputeWorkgroupSizeZ);
    P(WGPULimits, maxComputeWorkgroupsPerDimension);
    P(WGPULimits, maxUniformBufferBindingSize);
    P(WGPULimits, maxStorageBufferBindingSize);

    S(WGPUAdapterInfo);
    P(WGPUAdapterInfo, nextInChain);
    P(WGPUAdapterInfo, vendor);
    P(WGPUAdapterInfo, architecture);
    P(WGPUAdapterInfo, device);
    P(WGPUAdapterInfo, description);
    P(WGPUAdapterInfo, backendType);
    P(WGPUAdapterInfo, adapterType);
    P(WGPUAdapterInfo, vendorID);
    P(WGPUAdapterInfo, deviceID);

    S(WGPUBindGroupLayoutEntry);
    P(WGPUBindGroupLayoutEntry, nextInChain);
    P(WGPUBindGroupLayoutEntry, binding);
    P(WGPUBindGroupLayoutEntry, visibility);
    P(WGPUBindGroupLayoutEntry, buffer);
    P(WGPUBindGroupLayoutEntry, sampler);
    P(WGPUBindGroupLayoutEntry, texture);
    P(WGPUBindGroupLayoutEntry, storageTexture);

    S(WGPUComputeState);
    P(WGPUComputeState, nextInChain);
    P(WGPUComputeState, module);
    P(WGPUComputeState, entryPoint);
    P(WGPUComputeState, constantCount);
    P(WGPUComputeState, constants);

    S(WGPUComputePipelineDescriptor);
    P(WGPUComputePipelineDescriptor, nextInChain);
    P(WGPUComputePipelineDescriptor, label);
    P(WGPUComputePipelineDescriptor, layout);
    P(WGPUComputePipelineDescriptor, compute);

    S(WGPUBufferMapCallbackInfo);
    P(WGPUBufferMapCallbackInfo, mode);
    P(WGPUBufferMapCallbackInfo, callback);
    P(WGPUBufferMapCallbackInfo, userdata1);
    P(WGPUBufferMapCallbackInfo, userdata2);

    S(WGPURequestAdapterCallbackInfo);
    P(WGPURequestAdapterCallbackInfo, mode);
    P(WGPURequestAdapterCallbackInfo, callback);
    P(WGPURequestAdapterCallbackInfo, userdata1);
    P(WGPURequestAdapterCallbackInfo, userdata2);

    S(WGPURequestDeviceCallbackInfo);
    P(WGPURequestDeviceCallbackInfo, mode);
    P(WGPURequestDeviceCallbackInfo, callback);
    P(WGPURequestDeviceCallbackInfo, userdata1);
    P(WGPURequestDeviceCallbackInfo, userdata2);

    S(WGPUDeviceLostCallbackInfo);
    P(WGPUDeviceLostCallbackInfo, mode);
    P(WGPUDeviceLostCallbackInfo, callback);
    P(WGPUDeviceLostCallbackInfo, userdata1);
    P(WGPUDeviceLostCallbackInfo, userdata2);

    S(WGPUBindGroupLayoutDescriptor);
    P(WGPUBindGroupLayoutDescriptor, nextInChain);
    P(WGPUBindGroupLayoutDescriptor, label);
    P(WGPUBindGroupLayoutDescriptor, entryCount);
    P(WGPUBindGroupLayoutDescriptor, entries);

    S(WGPUPipelineLayoutDescriptor);
    P(WGPUPipelineLayoutDescriptor, nextInChain);
    P(WGPUPipelineLayoutDescriptor, label);
    P(WGPUPipelineLayoutDescriptor, bindGroupLayoutCount);
    P(WGPUPipelineLayoutDescriptor, bindGroupLayouts);

    S(WGPUBindGroupDescriptor);
    P(WGPUBindGroupDescriptor, nextInChain);
    P(WGPUBindGroupDescriptor, label);
    P(WGPUBindGroupDescriptor, layout);
    P(WGPUBindGroupDescriptor, entryCount);
    P(WGPUBindGroupDescriptor, entries);

    S(WGPUBindGroupEntry);
    P(WGPUBindGroupEntry, nextInChain);
    P(WGPUBindGroupEntry, binding);
    P(WGPUBindGroupEntry, buffer);
    P(WGPUBindGroupEntry, offset);
    P(WGPUBindGroupEntry, size);
    P(WGPUBindGroupEntry, sampler);
    P(WGPUBindGroupEntry, textureView);

    S(WGPUCommandEncoderDescriptor);
    P(WGPUCommandEncoderDescriptor, nextInChain);
    P(WGPUCommandEncoderDescriptor, label);

    S(WGPUComputePassDescriptor);
    P(WGPUComputePassDescriptor, nextInChain);
    P(WGPUComputePassDescriptor, label);
    P(WGPUComputePassDescriptor, timestampWrites);

    S(WGPUCommandBufferDescriptor);
    P(WGPUCommandBufferDescriptor, nextInChain);
    P(WGPUCommandBufferDescriptor, label);

    return 0;
}
