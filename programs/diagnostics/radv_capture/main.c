/* radv_capture — Vulkan headless compute that mirrors mabda's
 * `native_pm4_build_compute_store_deadbeef` shape. Dispatch a 1x1x1
 * compute that writes 0xDEADBEEF to slot[0] of a storage buffer.
 *
 * Purpose: reference IB byte-stream against which mabda's pure-Cyrius
 * PM4 composer can be byte-diff'd. Run with RADV_DEBUG=ibs to dump
 * the indirect buffer contents to stderr, then `diff` against
 * mabda's `programs/native_compute_store` PM4 dump.
 *
 *   RADV_DEBUG=ibs ./radv_capture 2>radv.ib.txt
 *   ../../native_compute_store --dump-pm4 >mabda.ib.txt   # see README
 *   diff radv.ib.txt mabda.ib.txt
 *
 * Phase 1 minimum-viable: only proves the toolchain works + dispatch
 * runs + readback returns 0xDEADBEEF + RADV emits the IB dump. The
 * actual byte-diff against mabda's composer is a Phase 2 reduction.
 *
 * Built against vulkan-headers / loader >= 1.4.341 + radv (Mesa).
 * Other ICDs (NVIDIA, AMDVLK) work but won't emit the IB dump.
 */

#include <vulkan/vulkan.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define EXPECTED 0xDEADBEEFu
#define BUF_SIZE 64

static const char *err_str(VkResult r) {
    switch (r) {
        case VK_SUCCESS: return "VK_SUCCESS";
        case VK_ERROR_OUT_OF_HOST_MEMORY: return "OUT_OF_HOST_MEMORY";
        case VK_ERROR_OUT_OF_DEVICE_MEMORY: return "OUT_OF_DEVICE_MEMORY";
        case VK_ERROR_INITIALIZATION_FAILED: return "INIT_FAILED";
        case VK_ERROR_LAYER_NOT_PRESENT: return "LAYER_NOT_PRESENT";
        case VK_ERROR_EXTENSION_NOT_PRESENT: return "EXT_NOT_PRESENT";
        case VK_ERROR_FEATURE_NOT_PRESENT: return "FEATURE_NOT_PRESENT";
        case VK_ERROR_INCOMPATIBLE_DRIVER: return "INCOMPATIBLE_DRIVER";
        default: return "OTHER";
    }
}

#define VK_CHECK(call) do { \
    VkResult _r = (call); \
    if (_r != VK_SUCCESS) { \
        fprintf(stderr, "FAIL %s:%d %s -> %s (%d)\n", \
                __FILE__, __LINE__, #call, err_str(_r), _r); \
        return 1; \
    } \
} while (0)

static uint32_t pick_memory_type(VkPhysicalDevice phys, uint32_t mask,
                                 VkMemoryPropertyFlags want) {
    VkPhysicalDeviceMemoryProperties props;
    vkGetPhysicalDeviceMemoryProperties(phys, &props);
    for (uint32_t i = 0; i < props.memoryTypeCount; i++) {
        if (!(mask & (1u << i))) continue;
        if ((props.memoryTypes[i].propertyFlags & want) == want) return i;
    }
    return UINT32_MAX;
}

static uint32_t pick_compute_queue(VkPhysicalDevice phys) {
    uint32_t n = 0;
    vkGetPhysicalDeviceQueueFamilyProperties(phys, &n, NULL);
    if (n == 0) return UINT32_MAX;
    VkQueueFamilyProperties *q = malloc(sizeof(*q) * n);
    vkGetPhysicalDeviceQueueFamilyProperties(phys, &n, q);
    uint32_t pick = UINT32_MAX;
    for (uint32_t i = 0; i < n; i++) {
        if (q[i].queueFlags & VK_QUEUE_COMPUTE_BIT) { pick = i; break; }
    }
    free(q);
    return pick;
}

static int load_spv(const char *path, uint32_t **out, size_t *out_words) {
    FILE *f = fopen(path, "rb");
    if (!f) { fprintf(stderr, "FAIL open %s: errno=%d\n", path, 0); return 1; }
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    if (n <= 0 || (n % 4) != 0) {
        fprintf(stderr, "FAIL %s size=%ld (must be >0 and multiple of 4)\n", path, n);
        fclose(f);
        return 1;
    }
    *out = malloc((size_t)n);
    if (fread(*out, 1, (size_t)n, f) != (size_t)n) {
        fprintf(stderr, "FAIL read %s\n", path);
        fclose(f);
        free(*out);
        return 1;
    }
    fclose(f);
    *out_words = (size_t)n / 4;
    return 0;
}

int main(int argc, char **argv) {
    const char *spv_path = (argc > 1) ? argv[1] : "shader.spv";

    uint32_t *spv = NULL;
    size_t spv_words = 0;
    if (load_spv(spv_path, &spv, &spv_words)) return 1;

    /* ---- instance ---- */
    VkApplicationInfo app = {
        .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName = "radv_capture",
        .applicationVersion = 1,
        .pEngineName = "mabda-diagnostics",
        .engineVersion = 1,
        .apiVersion = VK_API_VERSION_1_2,
    };
    VkInstanceCreateInfo ici = {
        .sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        .pApplicationInfo = &app,
    };
    VkInstance inst;
    VK_CHECK(vkCreateInstance(&ici, NULL, &inst));

    /* ---- physical device (first AMD-class) ---- */
    uint32_t phys_n = 0;
    VK_CHECK(vkEnumeratePhysicalDevices(inst, &phys_n, NULL));
    if (phys_n == 0) { fprintf(stderr, "FAIL no physical devices\n"); return 1; }
    VkPhysicalDevice *physes = malloc(sizeof(*physes) * phys_n);
    VK_CHECK(vkEnumeratePhysicalDevices(inst, &phys_n, physes));
    VkPhysicalDevice phys = VK_NULL_HANDLE;
    VkPhysicalDeviceProperties props;
    for (uint32_t i = 0; i < phys_n; i++) {
        vkGetPhysicalDeviceProperties(physes[i], &props);
        if (props.vendorID == 0x1002) { phys = physes[i]; break; }   /* AMD */
    }
    if (phys == VK_NULL_HANDLE) {
        /* fall through to first device — RADV may not always report 0x1002 in test envs */
        phys = physes[0];
        vkGetPhysicalDeviceProperties(phys, &props);
    }
    free(physes);
    fprintf(stderr, "device: %s (vendor 0x%04x)\n", props.deviceName, props.vendorID);

    /* ---- queue + device ---- */
    uint32_t qfam = pick_compute_queue(phys);
    if (qfam == UINT32_MAX) { fprintf(stderr, "FAIL no compute queue\n"); return 1; }
    float prio = 1.0f;
    VkDeviceQueueCreateInfo qci = {
        .sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex = qfam,
        .queueCount = 1,
        .pQueuePriorities = &prio,
    };
    VkDeviceCreateInfo dci = {
        .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount = 1,
        .pQueueCreateInfos = &qci,
    };
    VkDevice dev;
    VK_CHECK(vkCreateDevice(phys, &dci, NULL, &dev));
    VkQueue queue;
    vkGetDeviceQueue(dev, qfam, 0, &queue);

    /* ---- storage buffer ---- */
    VkBufferCreateInfo bci = {
        .sType = VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        .size = BUF_SIZE,
        .usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT,
        .sharingMode = VK_SHARING_MODE_EXCLUSIVE,
    };
    VkBuffer buf;
    VK_CHECK(vkCreateBuffer(dev, &bci, NULL, &buf));
    VkMemoryRequirements mreq;
    vkGetBufferMemoryRequirements(dev, buf, &mreq);
    uint32_t mtype = pick_memory_type(phys, mreq.memoryTypeBits,
        VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT);
    if (mtype == UINT32_MAX) { fprintf(stderr, "FAIL no host-visible memtype\n"); return 1; }
    VkMemoryAllocateInfo mai = {
        .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize = mreq.size,
        .memoryTypeIndex = mtype,
    };
    VkDeviceMemory mem;
    VK_CHECK(vkAllocateMemory(dev, &mai, NULL, &mem));
    VK_CHECK(vkBindBufferMemory(dev, buf, mem, 0));

    /* ---- pre-zero the buffer so the post-dispatch readback is unambiguous ---- */
    void *map = NULL;
    VK_CHECK(vkMapMemory(dev, mem, 0, BUF_SIZE, 0, &map));
    memset(map, 0, BUF_SIZE);
    vkUnmapMemory(dev, mem);

    /* ---- shader module ---- */
    VkShaderModuleCreateInfo smci = {
        .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = spv_words * 4,
        .pCode = spv,
    };
    VkShaderModule shader;
    VK_CHECK(vkCreateShaderModule(dev, &smci, NULL, &shader));

    /* ---- descriptor set layout ---- */
    VkDescriptorSetLayoutBinding dslb = {
        .binding = 0,
        .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
        .descriptorCount = 1,
        .stageFlags = VK_SHADER_STAGE_COMPUTE_BIT,
    };
    VkDescriptorSetLayoutCreateInfo dslci = {
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        .bindingCount = 1,
        .pBindings = &dslb,
    };
    VkDescriptorSetLayout dsl;
    VK_CHECK(vkCreateDescriptorSetLayout(dev, &dslci, NULL, &dsl));

    /* ---- pipeline layout ---- */
    VkPipelineLayoutCreateInfo plci = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        .setLayoutCount = 1,
        .pSetLayouts = &dsl,
    };
    VkPipelineLayout layout;
    VK_CHECK(vkCreatePipelineLayout(dev, &plci, NULL, &layout));

    /* ---- compute pipeline ---- */
    VkComputePipelineCreateInfo cpci = {
        .sType = VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
        .stage = {
            .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            .stage = VK_SHADER_STAGE_COMPUTE_BIT,
            .module = shader,
            .pName = "main",
        },
        .layout = layout,
    };
    VkPipeline pipe;
    VK_CHECK(vkCreateComputePipelines(dev, VK_NULL_HANDLE, 1, &cpci, NULL, &pipe));

    /* ---- descriptor pool + set ---- */
    VkDescriptorPoolSize dps = {
        .type = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .descriptorCount = 1,
    };
    VkDescriptorPoolCreateInfo dpci = {
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        .maxSets = 1, .poolSizeCount = 1, .pPoolSizes = &dps,
    };
    VkDescriptorPool pool;
    VK_CHECK(vkCreateDescriptorPool(dev, &dpci, NULL, &pool));
    VkDescriptorSetAllocateInfo dsai = {
        .sType = VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        .descriptorPool = pool, .descriptorSetCount = 1, .pSetLayouts = &dsl,
    };
    VkDescriptorSet ds;
    VK_CHECK(vkAllocateDescriptorSets(dev, &dsai, &ds));
    VkDescriptorBufferInfo dbi = { .buffer = buf, .offset = 0, .range = BUF_SIZE };
    VkWriteDescriptorSet wds = {
        .sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,
        .dstSet = ds, .dstBinding = 0, .descriptorCount = 1,
        .descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, .pBufferInfo = &dbi,
    };
    vkUpdateDescriptorSets(dev, 1, &wds, 0, NULL);

    /* ---- command pool + buffer ---- */
    VkCommandPoolCreateInfo cpoolci = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
        .queueFamilyIndex = qfam,
    };
    VkCommandPool cpool;
    VK_CHECK(vkCreateCommandPool(dev, &cpoolci, NULL, &cpool));
    VkCommandBufferAllocateInfo cbai = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool = cpool, .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY,
        .commandBufferCount = 1,
    };
    VkCommandBuffer cmd;
    VK_CHECK(vkAllocateCommandBuffers(dev, &cbai, &cmd));

    VkCommandBufferBeginInfo cbi = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT,
    };
    VK_CHECK(vkBeginCommandBuffer(cmd, &cbi));
    vkCmdBindPipeline(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, pipe);
    vkCmdBindDescriptorSets(cmd, VK_PIPELINE_BIND_POINT_COMPUTE, layout, 0, 1, &ds, 0, NULL);
    vkCmdDispatch(cmd, 1, 1, 1);
    VK_CHECK(vkEndCommandBuffer(cmd));

    /* ---- submit + wait ---- */
    VkSubmitInfo si = {
        .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO,
        .commandBufferCount = 1, .pCommandBuffers = &cmd,
    };
    VK_CHECK(vkQueueSubmit(queue, 1, &si, VK_NULL_HANDLE));
    VK_CHECK(vkQueueWaitIdle(queue));

    /* ---- readback ---- */
    VK_CHECK(vkMapMemory(dev, mem, 0, BUF_SIZE, 0, &map));
    uint32_t got = ((uint32_t*)map)[0];
    vkUnmapMemory(dev, mem);

    fprintf(stdout, "slot[0] = 0x%08x (expected 0x%08x)\n", got, EXPECTED);
    int rc = (got == EXPECTED) ? 0 : 1;

    /* ---- cleanup ---- */
    vkDestroyCommandPool(dev, cpool, NULL);
    vkDestroyDescriptorPool(dev, pool, NULL);
    vkDestroyPipeline(dev, pipe, NULL);
    vkDestroyPipelineLayout(dev, layout, NULL);
    vkDestroyDescriptorSetLayout(dev, dsl, NULL);
    vkDestroyShaderModule(dev, shader, NULL);
    vkDestroyBuffer(dev, buf, NULL);
    vkFreeMemory(dev, mem, NULL);
    vkDestroyDevice(dev, NULL);
    vkDestroyInstance(inst, NULL);
    free(spv);

    return rc;
}
