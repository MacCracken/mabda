/* mabda diagnostic (4.1.4): an explicit Vulkan layer that counts the device memory a process
 * holds. Every VkDeviceMemory comes from vkAllocateMemory and returns via vkFreeMemory, so
 * live bytes = sum(allocated) - sum(freed): exact for vkAllocateMemory-backed memory (driver
 * allocations that bypass it are not counted). One event line per call goes straight to fd 2
 * so it interleaves in order with the program output. Used by check_bench_memory.sh. */
#include <vulkan/vulkan.h>
#include <vulkan/vk_layer.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <stdint.h>

static VkInstance g_instance;
static VkPhysicalDeviceMemoryProperties g_memprops;
static int g_have_props;
static PFN_vkGetInstanceProcAddr next_gipa;
static PFN_vkGetDeviceProcAddr next_gdpa;
typedef VkResult (VKAPI_PTR *CreateInstanceFn)(const VkInstanceCreateInfo *, const VkAllocationCallbacks *,
                                                VkInstance *);
typedef VkResult (VKAPI_PTR *CreateDeviceFn)(VkPhysicalDevice, const VkDeviceCreateInfo *,
                                              const VkAllocationCallbacks *, VkDevice *);
typedef VkResult (VKAPI_PTR *AllocFn)(VkDevice, const VkMemoryAllocateInfo *, const VkAllocationCallbacks *,
                                      VkDeviceMemory *);
typedef void (VKAPI_PTR *FreeFn)(VkDevice, VkDeviceMemory, const VkAllocationCallbacks *);
static AllocFn next_alloc;
static FreeFn next_free;

#define MAPN 65536
static uint64_t map_key[MAPN];
static uint64_t map_size[MAPN];
static uint32_t map_type[MAPN];
static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
static uint64_t live_total, live_devlocal, peak_total, n_alloc, n_free;

static int is_devlocal(uint32_t type_index) {
    if (!g_have_props || type_index >= g_memprops.memoryTypeCount) return -1;
    VkMemoryPropertyFlags f = g_memprops.memoryTypes[type_index].propertyFlags;
    return (f & VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT) && !(f & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT);
}

static VkResult VKAPI_CALL mt_AllocateMemory(VkDevice device, const VkMemoryAllocateInfo *info,
                                             const VkAllocationCallbacks *alloc, VkDeviceMemory *mem) {
    VkResult r = next_alloc(device, info, alloc, mem);
    if (r != VK_SUCCESS) {
        dprintf(2, "MEMTRACK alloc-FAILED r=%d size=%llu type=%u\n", r,
                (unsigned long long)info->allocationSize, info->memoryTypeIndex);
        return r;
    }
    pthread_mutex_lock(&mu);
    uint64_t k = (uint64_t)*mem;
    uint64_t h = (k * 0x9E3779B97F4A7C15ull) >> 48;
    for (uint64_t i = 0; i < MAPN; i++) {
        uint64_t s = (h + i) & (MAPN - 1);
        if (map_key[s] == 0) { map_key[s] = k; map_size[s] = info->allocationSize; map_type[s] = info->memoryTypeIndex; break; }
    }
    live_total += info->allocationSize;
    if (is_devlocal(info->memoryTypeIndex) == 1) live_devlocal += info->allocationSize;
    if (live_total > peak_total) peak_total = live_total;
    n_alloc++;
    dprintf(2, "MEMTRACK alloc size=%llu type=%u devlocal=%d live=%llu live_devlocal=%llu n_alloc=%llu n_free=%llu\n",
            (unsigned long long)info->allocationSize, info->memoryTypeIndex, is_devlocal(info->memoryTypeIndex),
            (unsigned long long)live_total, (unsigned long long)live_devlocal,
            (unsigned long long)n_alloc, (unsigned long long)n_free);
    pthread_mutex_unlock(&mu);
    return r;
}

static void VKAPI_CALL mt_FreeMemory(VkDevice device, VkDeviceMemory mem, const VkAllocationCallbacks *alloc) {
    if (mem != VK_NULL_HANDLE) {
        pthread_mutex_lock(&mu);
        uint64_t k = (uint64_t)mem;
        uint64_t h = (k * 0x9E3779B97F4A7C15ull) >> 48;
        uint64_t size = 0; uint32_t type = 0; int found = 0;
        for (uint64_t i = 0; i < MAPN; i++) {
            uint64_t s = (h + i) & (MAPN - 1);
            if (map_key[s] == k) { size = map_size[s]; type = map_type[s]; map_key[s] = 1; found = 1; break; }
            if (map_key[s] == 0) break;
        }
        /* key 1 = tombstone (real handles are never 1) */
        live_total -= size;
        if (is_devlocal(type) == 1) live_devlocal -= size;
        n_free++;
        dprintf(2, "MEMTRACK free size=%llu type=%u found=%d live=%llu live_devlocal=%llu n_alloc=%llu n_free=%llu\n",
                (unsigned long long)size, type, found, (unsigned long long)live_total,
                (unsigned long long)live_devlocal, (unsigned long long)n_alloc, (unsigned long long)n_free);
        pthread_mutex_unlock(&mu);
    }
    next_free(device, mem, alloc);
}

static VkResult VKAPI_CALL mt_CreateInstance(const VkInstanceCreateInfo *ci, const VkAllocationCallbacks *a,
                                             VkInstance *inst) {
    VkLayerInstanceCreateInfo *chain = (VkLayerInstanceCreateInfo *)ci->pNext;
    while (chain && !(chain->sType == VK_STRUCTURE_TYPE_LOADER_INSTANCE_CREATE_INFO &&
                      chain->function == VK_LAYER_LINK_INFO))
        chain = (VkLayerInstanceCreateInfo *)chain->pNext;
    if (!chain) return VK_ERROR_INITIALIZATION_FAILED;
    next_gipa = chain->u.pLayerInfo->pfnNextGetInstanceProcAddr;
    chain->u.pLayerInfo = chain->u.pLayerInfo->pNext;
    CreateInstanceFn create = (CreateInstanceFn)next_gipa(VK_NULL_HANDLE, "vkCreateInstance");
    VkResult r = create(ci, a, inst);
    if (r == VK_SUCCESS) g_instance = *inst;
    dprintf(2, "MEMTRACK layer active: instance r=%d\n", r);
    return r;
}

static VkResult VKAPI_CALL mt_CreateDevice(VkPhysicalDevice pd, const VkDeviceCreateInfo *ci,
                                           const VkAllocationCallbacks *a, VkDevice *dev) {
    VkLayerDeviceCreateInfo *chain = (VkLayerDeviceCreateInfo *)ci->pNext;
    while (chain && !(chain->sType == VK_STRUCTURE_TYPE_LOADER_DEVICE_CREATE_INFO &&
                      chain->function == VK_LAYER_LINK_INFO))
        chain = (VkLayerDeviceCreateInfo *)chain->pNext;
    if (!chain) return VK_ERROR_INITIALIZATION_FAILED;
    PFN_vkGetInstanceProcAddr gipa = chain->u.pLayerInfo->pfnNextGetInstanceProcAddr;
    PFN_vkGetDeviceProcAddr gdpa = chain->u.pLayerInfo->pfnNextGetDeviceProcAddr;
    chain->u.pLayerInfo = chain->u.pLayerInfo->pNext;
    CreateDeviceFn create = (CreateDeviceFn)gipa(g_instance, "vkCreateDevice");
    VkResult r = create(pd, ci, a, dev);
    if (r != VK_SUCCESS) return r;
    next_gdpa = gdpa;
    next_alloc = (AllocFn)gdpa(*dev, "vkAllocateMemory");
    next_free = (FreeFn)gdpa(*dev, "vkFreeMemory");
    void (VKAPI_PTR *getprops)(VkPhysicalDevice, VkPhysicalDeviceMemoryProperties *) =
        (void (VKAPI_PTR *)(VkPhysicalDevice, VkPhysicalDeviceMemoryProperties *))
        gipa(g_instance, "vkGetPhysicalDeviceMemoryProperties");
    if (getprops) { getprops(pd, &g_memprops); g_have_props = 1; }
    for (uint32_t i = 0; g_have_props && i < g_memprops.memoryTypeCount; i++)
        dprintf(2, "MEMTRACK memtype %u flags=0x%x heap=%u heapsize=%llu\n", i,
                g_memprops.memoryTypes[i].propertyFlags, g_memprops.memoryTypes[i].heapIndex,
                (unsigned long long)g_memprops.memoryHeaps[g_memprops.memoryTypes[i].heapIndex].size);
    dprintf(2, "MEMTRACK layer active: device created\n");
    return r;
}

VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL mt_GetDeviceProcAddr(VkDevice dev, const char *name);
VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL mt_GetInstanceProcAddr(VkInstance inst, const char *name) {
    if (!strcmp(name, "vkGetInstanceProcAddr")) return (PFN_vkVoidFunction)mt_GetInstanceProcAddr;
    if (!strcmp(name, "vkCreateInstance")) return (PFN_vkVoidFunction)mt_CreateInstance;
    if (!strcmp(name, "vkCreateDevice")) return (PFN_vkVoidFunction)mt_CreateDevice;
    if (!strcmp(name, "vkGetDeviceProcAddr")) return (PFN_vkVoidFunction)mt_GetDeviceProcAddr;
    if (!strcmp(name, "vkAllocateMemory")) return (PFN_vkVoidFunction)mt_AllocateMemory;
    if (!strcmp(name, "vkFreeMemory")) return (PFN_vkVoidFunction)mt_FreeMemory;
    return next_gipa ? next_gipa(inst, name) : NULL;
}

VKAPI_ATTR PFN_vkVoidFunction VKAPI_CALL mt_GetDeviceProcAddr(VkDevice dev, const char *name) {
    if (!strcmp(name, "vkGetDeviceProcAddr")) return (PFN_vkVoidFunction)mt_GetDeviceProcAddr;
    if (!strcmp(name, "vkAllocateMemory")) return (PFN_vkVoidFunction)mt_AllocateMemory;
    if (!strcmp(name, "vkFreeMemory")) return (PFN_vkVoidFunction)mt_FreeMemory;
    return next_gdpa ? next_gdpa(dev, name) : NULL;
}

VKAPI_ATTR VkResult VKAPI_CALL vkNegotiateLoaderLayerInterfaceVersion(VkNegotiateLayerInterface *v) {
    if (v->loaderLayerInterfaceVersion >= 2) {
        v->pfnGetInstanceProcAddr = mt_GetInstanceProcAddr;
        v->pfnGetDeviceProcAddr = mt_GetDeviceProcAddr;
        v->pfnGetPhysicalDeviceProcAddr = NULL;
        v->loaderLayerInterfaceVersion = 2;
    }
    return VK_SUCCESS;
}
