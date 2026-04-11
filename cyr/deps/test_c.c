#include <stdio.h>
#include "wgpu-native/include/webgpu/webgpu.h"
#include "wgpu-native/include/webgpu/wgpu.h"

static void on_adapter(WGPURequestAdapterStatus status, WGPUAdapter adapter, WGPUStringView message, void* ud1, void* ud2) {
    printf("callback: status=%d adapter=%p\n", status, (void*)adapter);
    if (ud1) *(WGPUAdapter*)ud1 = adapter;
}

int main() {
    WGPUInstance instance = wgpuCreateInstance(NULL);
    printf("instance=%p\n", (void*)instance);

    WGPUAdapter adapter = NULL;
    WGPURequestAdapterCallbackInfo cb = {
        .mode = WGPUCallbackMode_AllowSpontaneous,
        .callback = on_adapter,
        .userdata1 = &adapter,
    };
    wgpuInstanceRequestAdapter(instance, NULL, cb);
    wgpuInstanceProcessEvents(instance);

    printf("adapter=%p\n", (void*)adapter);
    if (adapter) {
        printf("SUCCESS: Got GPU adapter!\n");
        WGPUAdapterInfo info = {};
        wgpuAdapterGetInfo(adapter, &info);
        printf("GPU: %.*s\n", (int)info.description.length, info.description.data);
    } else {
        printf("FAILED: No adapter\n");
    }
    return 0;
}
