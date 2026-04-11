#include <stdio.h>
#include <string.h>
#include "wgpu-native/include/webgpu/webgpu.h"

// Same shim function
WGPUFuture wgpu_shim_instance_request_adapter(
    WGPUInstance instance,
    WGPURequestAdapterOptions const *options,
    WGPURequestAdapterCallbackInfo const *callbackInfo
) {
    return wgpuInstanceRequestAdapter(instance, options, *callbackInfo);
}

static void on_adapter(WGPURequestAdapterStatus status, WGPUAdapter adapter, WGPUStringView message, void* ud1, void* ud2) {
    printf("callback: status=%d adapter=%p\n", status, (void*)adapter);
    if (ud1) *(long*)ud1 = (long)adapter;
}

int main() {
    WGPUInstance instance = wgpuCreateInstance(NULL);
    printf("instance=%p\n", (void*)instance);

    long adapter_ptr = 0;
    WGPURequestAdapterCallbackInfo cb;
    memset(&cb, 0, sizeof(cb));
    cb.mode = WGPUCallbackMode_AllowSpontaneous;
    cb.callback = on_adapter;
    cb.userdata1 = &adapter_ptr;

    // Call shim (by pointer)
    wgpu_shim_instance_request_adapter(instance, NULL, &cb);
    wgpuInstanceProcessEvents(instance);

    printf("adapter=%ld (0x%lx)\n", adapter_ptr, adapter_ptr);
    if (adapter_ptr) printf("SUCCESS via shim!\n");
    else printf("FAILED via shim\n");
    return 0;
}
