/* radv_capture_triangle — Vulkan headless graphics reference for
 * mabda's `native_pm4_build_render_clear_triangle`. Draws a solid-red
 * fullscreen triangle into a 256x256 RGBA8_UNORM RT, waits for
 * completion, reads back pixel(0,0), asserts red.
 *
 * Purpose: provide a byte-exact radv IB capture for the SAME workload
 * mabda emits, so the two can be diffed packet-by-packet. The existing
 * radv_capture (compute) gives the compute reference; this gives the
 * graphics reference. vkcube was previously used and is the wrong
 * scope — textured cube + depth + uniform buffers diverge in too many
 * places from mabda's clear-triangle PM4.
 *
 *   RADV_DEBUG=ibs ./radv_capture_triangle 2>radv.ib.txt
 *   ../../build/dump_render_pm4 >mabda.ib.txt
 *   # diff the two — see Makefile `compare` target
 *
 * Built against Vulkan-Loader. radv (Mesa) ICD must be active for the
 * IB dump.
 */

#include <vulkan/vulkan.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define RT_WIDTH  256
#define RT_HEIGHT 256
#define RT_FORMAT VK_FORMAT_R8G8B8A8_UNORM

static const char *err_str(VkResult r) {
    switch (r) {
        case VK_SUCCESS: return "VK_SUCCESS";
        case VK_ERROR_OUT_OF_HOST_MEMORY: return "OUT_OF_HOST_MEMORY";
        case VK_ERROR_OUT_OF_DEVICE_MEMORY: return "OUT_OF_DEVICE_MEMORY";
        case VK_ERROR_INITIALIZATION_FAILED: return "INIT_FAILED";
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

static int read_file(const char *path, void **out_data, size_t *out_size) {
    FILE *f = fopen(path, "rb");
    if (!f) { perror(path); return -1; }
    fseek(f, 0, SEEK_END);
    long n = ftell(f);
    fseek(f, 0, SEEK_SET);
    void *buf = malloc((size_t)n);
    if (fread(buf, 1, (size_t)n, f) != (size_t)n) { fclose(f); free(buf); return -1; }
    fclose(f);
    *out_data = buf;
    *out_size = (size_t)n;
    return 0;
}

int main(void) {
    /* ----- Instance ----- */
    VkApplicationInfo app = {
        .sType = VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName = "radv_capture_triangle",
        .apiVersion = VK_API_VERSION_1_2,
    };
    VkInstanceCreateInfo ici = {
        .sType = VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,
        .pApplicationInfo = &app,
    };
    VkInstance instance;
    VK_CHECK(vkCreateInstance(&ici, NULL, &instance));

    /* ----- Pick first physical device (radv on this box) ----- */
    uint32_t pd_count = 1;
    VkPhysicalDevice phys;
    VK_CHECK(vkEnumeratePhysicalDevices(instance, &pd_count, &phys));

    uint32_t qf_count;
    vkGetPhysicalDeviceQueueFamilyProperties(phys, &qf_count, NULL);
    VkQueueFamilyProperties *qfp = calloc(qf_count, sizeof(*qfp));
    vkGetPhysicalDeviceQueueFamilyProperties(phys, &qf_count, qfp);
    uint32_t qfi = UINT32_MAX;
    for (uint32_t i = 0; i < qf_count; i++)
        if (qfp[i].queueFlags & VK_QUEUE_GRAPHICS_BIT) { qfi = i; break; }
    free(qfp);

    float prio = 1.0f;
    VkDeviceQueueCreateInfo dqci = {
        .sType = VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex = qfi,
        .queueCount = 1,
        .pQueuePriorities = &prio,
    };
    VkDeviceCreateInfo dci = {
        .sType = VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount = 1,
        .pQueueCreateInfos = &dqci,
    };
    VkDevice device;
    VK_CHECK(vkCreateDevice(phys, &dci, NULL, &device));
    VkQueue queue;
    vkGetDeviceQueue(device, qfi, 0, &queue);

    /* ----- Render target: VkImage + memory + view ----- */
    VkImageCreateInfo rti = {
        .sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,
        .imageType = VK_IMAGE_TYPE_2D,
        .format = RT_FORMAT,
        .extent = { RT_WIDTH, RT_HEIGHT, 1 },
        .mipLevels = 1, .arrayLayers = 1,
        .samples = VK_SAMPLE_COUNT_1_BIT,
        .tiling = VK_IMAGE_TILING_LINEAR,
        .usage = VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT
               | VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
        .sharingMode = VK_SHARING_MODE_EXCLUSIVE,
        .initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
    };
    VkImage rt;
    VK_CHECK(vkCreateImage(device, &rti, NULL, &rt));
    VkMemoryRequirements rt_mr;
    vkGetImageMemoryRequirements(device, rt, &rt_mr);
    uint32_t rt_mt = pick_memory_type(phys, rt_mr.memoryTypeBits,
        VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT
        | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT);
    if (rt_mt == UINT32_MAX) {
        fprintf(stderr, "FAIL: no host-visible memory for RT\n");
        return 1;
    }
    VkMemoryAllocateInfo rt_mai = {
        .sType = VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize = rt_mr.size,
        .memoryTypeIndex = rt_mt,
    };
    VkDeviceMemory rt_mem;
    VK_CHECK(vkAllocateMemory(device, &rt_mai, NULL, &rt_mem));
    VK_CHECK(vkBindImageMemory(device, rt, rt_mem, 0));

    VkImageViewCreateInfo rtvi = {
        .sType = VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,
        .image = rt,
        .viewType = VK_IMAGE_VIEW_TYPE_2D,
        .format = RT_FORMAT,
        .subresourceRange = { VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1 },
    };
    VkImageView rt_view;
    VK_CHECK(vkCreateImageView(device, &rtvi, NULL, &rt_view));

    /* ----- Render pass: single subpass, single color attachment ----- */
    VkAttachmentDescription ad = {
        .format = RT_FORMAT,
        .samples = VK_SAMPLE_COUNT_1_BIT,
        .loadOp = VK_ATTACHMENT_LOAD_OP_CLEAR,
        .storeOp = VK_ATTACHMENT_STORE_OP_STORE,
        .stencilLoadOp = VK_ATTACHMENT_LOAD_OP_DONT_CARE,
        .stencilStoreOp = VK_ATTACHMENT_STORE_OP_DONT_CARE,
        .initialLayout = VK_IMAGE_LAYOUT_UNDEFINED,
        .finalLayout = VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
    };
    VkAttachmentReference ar = {
        .attachment = 0,
        .layout = VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL,
    };
    VkSubpassDescription sd = {
        .pipelineBindPoint = VK_PIPELINE_BIND_POINT_GRAPHICS,
        .colorAttachmentCount = 1,
        .pColorAttachments = &ar,
    };
    VkRenderPassCreateInfo rpci = {
        .sType = VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
        .attachmentCount = 1,
        .pAttachments = &ad,
        .subpassCount = 1,
        .pSubpasses = &sd,
    };
    VkRenderPass rpass;
    VK_CHECK(vkCreateRenderPass(device, &rpci, NULL, &rpass));

    VkFramebufferCreateInfo fbci = {
        .sType = VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
        .renderPass = rpass,
        .attachmentCount = 1,
        .pAttachments = &rt_view,
        .width = RT_WIDTH, .height = RT_HEIGHT, .layers = 1,
    };
    VkFramebuffer fb;
    VK_CHECK(vkCreateFramebuffer(device, &fbci, NULL, &fb));

    /* ----- Shader modules ----- */
    void *vs_blob; size_t vs_size;
    if (read_file("triangle.vert.spv", &vs_blob, &vs_size) != 0) return 1;
    void *fs_blob; size_t fs_size;
    if (read_file("triangle.frag.spv", &fs_blob, &fs_size) != 0) return 1;

    VkShaderModuleCreateInfo vsmci = {
        .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = vs_size, .pCode = vs_blob,
    };
    VkShaderModule vs_mod;
    VK_CHECK(vkCreateShaderModule(device, &vsmci, NULL, &vs_mod));
    VkShaderModuleCreateInfo fsmci = {
        .sType = VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize = fs_size, .pCode = fs_blob,
    };
    VkShaderModule fs_mod;
    VK_CHECK(vkCreateShaderModule(device, &fsmci, NULL, &fs_mod));

    /* ----- Pipeline layout (empty — no descriptors, no push consts) ----- */
    VkPipelineLayoutCreateInfo plci = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
    };
    VkPipelineLayout plyt;
    VK_CHECK(vkCreatePipelineLayout(device, &plci, NULL, &plyt));

    /* ----- Graphics pipeline ----- */
    VkPipelineShaderStageCreateInfo stages[2] = {
        {
            .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            .stage = VK_SHADER_STAGE_VERTEX_BIT,
            .module = vs_mod, .pName = "main",
        },
        {
            .sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
            .stage = VK_SHADER_STAGE_FRAGMENT_BIT,
            .module = fs_mod, .pName = "main",
        },
    };
    VkPipelineVertexInputStateCreateInfo vis = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO,
    };
    VkPipelineInputAssemblyStateCreateInfo ias = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
        .topology = VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST,
    };
    VkViewport vp = { 0, 0, RT_WIDTH, RT_HEIGHT, 0, 1 };
    VkRect2D sc = { {0,0}, {RT_WIDTH, RT_HEIGHT} };
    VkPipelineViewportStateCreateInfo vps = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
        .viewportCount = 1, .pViewports = &vp,
        .scissorCount = 1, .pScissors = &sc,
    };
    VkPipelineRasterizationStateCreateInfo rs = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
        .polygonMode = VK_POLYGON_MODE_FILL,
        .cullMode = VK_CULL_MODE_NONE,
        .frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE,
        .lineWidth = 1.0f,
    };
    VkPipelineMultisampleStateCreateInfo ms = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
        .rasterizationSamples = VK_SAMPLE_COUNT_1_BIT,
    };
    VkPipelineColorBlendAttachmentState cba = {
        .colorWriteMask = VK_COLOR_COMPONENT_R_BIT
                        | VK_COLOR_COMPONENT_G_BIT
                        | VK_COLOR_COMPONENT_B_BIT
                        | VK_COLOR_COMPONENT_A_BIT,
    };
    VkPipelineColorBlendStateCreateInfo cbs = {
        .sType = VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
        .attachmentCount = 1, .pAttachments = &cba,
    };
    VkGraphicsPipelineCreateInfo gpci = {
        .sType = VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
        .stageCount = 2, .pStages = stages,
        .pVertexInputState = &vis,
        .pInputAssemblyState = &ias,
        .pViewportState = &vps,
        .pRasterizationState = &rs,
        .pMultisampleState = &ms,
        .pColorBlendState = &cbs,
        .layout = plyt,
        .renderPass = rpass,
        .subpass = 0,
    };
    VkPipeline pipe;
    VK_CHECK(vkCreateGraphicsPipelines(device, VK_NULL_HANDLE, 1, &gpci, NULL, &pipe));

    /* ----- Command buffer + record ----- */
    VkCommandPoolCreateInfo cpci = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,
        .queueFamilyIndex = qfi,
    };
    VkCommandPool cpool;
    VK_CHECK(vkCreateCommandPool(device, &cpci, NULL, &cpool));
    VkCommandBufferAllocateInfo cbai = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool = cpool,
        .level = VK_COMMAND_BUFFER_LEVEL_PRIMARY,
        .commandBufferCount = 1,
    };
    VkCommandBuffer cb;
    VK_CHECK(vkAllocateCommandBuffers(device, &cbai, &cb));

    VkCommandBufferBeginInfo cbbi = {
        .sType = VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
    };
    VK_CHECK(vkBeginCommandBuffer(cb, &cbbi));

    VkClearValue clear = { .color = { .float32 = {0.33f, 0.33f, 0.33f, 1.0f} } };
    VkRenderPassBeginInfo rpbi = {
        .sType = VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
        .renderPass = rpass,
        .framebuffer = fb,
        .renderArea = { {0,0}, {RT_WIDTH, RT_HEIGHT} },
        .clearValueCount = 1, .pClearValues = &clear,
    };
    vkCmdBeginRenderPass(cb, &rpbi, VK_SUBPASS_CONTENTS_INLINE);
    vkCmdBindPipeline(cb, VK_PIPELINE_BIND_POINT_GRAPHICS, pipe);
    vkCmdDraw(cb, 3, 1, 0, 0);
    vkCmdEndRenderPass(cb);

    VK_CHECK(vkEndCommandBuffer(cb));

    /* ----- Submit + wait ----- */
    VkSubmitInfo si = {
        .sType = VK_STRUCTURE_TYPE_SUBMIT_INFO,
        .commandBufferCount = 1, .pCommandBuffers = &cb,
    };
    VkFenceCreateInfo fci = { .sType = VK_STRUCTURE_TYPE_FENCE_CREATE_INFO };
    VkFence fence;
    VK_CHECK(vkCreateFence(device, &fci, NULL, &fence));
    VK_CHECK(vkQueueSubmit(queue, 1, &si, fence));
    VK_CHECK(vkWaitForFences(device, 1, &fence, VK_TRUE, 2000000000ull));

    /* ----- Map RT memory, read pixel(0,0) ----- */
    void *mapped;
    VK_CHECK(vkMapMemory(device, rt_mem, 0, VK_WHOLE_SIZE, 0, &mapped));
    VkSubresourceLayout layout;
    VkImageSubresource subr = {
        .aspectMask = VK_IMAGE_ASPECT_COLOR_BIT, .mipLevel = 0, .arrayLayer = 0,
    };
    vkGetImageSubresourceLayout(device, rt, &subr, &layout);
    uint8_t *base = (uint8_t *)mapped + layout.offset;
    uint8_t r = base[0], g = base[1], b = base[2], a = base[3];
    printf("pixel(0,0) = (0x%02X, 0x%02X, 0x%02X, 0x%02X)\n", r, g, b, a);
    vkUnmapMemory(device, rt_mem);

    int ok = (r == 0xFF && g == 0x00 && b == 0x00 && a == 0xFF);
    printf("%s — solid red triangle round-trip\n", ok ? "PASS" : "FAIL");

    /* ----- Cleanup (lightweight; exit will reap) ----- */
    vkDestroyFence(device, fence, NULL);
    vkDestroyCommandPool(device, cpool, NULL);
    vkDestroyPipeline(device, pipe, NULL);
    vkDestroyPipelineLayout(device, plyt, NULL);
    vkDestroyShaderModule(device, vs_mod, NULL);
    vkDestroyShaderModule(device, fs_mod, NULL);
    free(vs_blob); free(fs_blob);
    vkDestroyFramebuffer(device, fb, NULL);
    vkDestroyRenderPass(device, rpass, NULL);
    vkDestroyImageView(device, rt_view, NULL);
    vkFreeMemory(device, rt_mem, NULL);
    vkDestroyImage(device, rt, NULL);
    vkDestroyDevice(device, NULL);
    vkDestroyInstance(instance, NULL);
    return ok ? 0 : 1;
}
