// vk_render.c — minimal HEADLESS NVK Vulkan TRIANGLE draw (mabda N7.2a capture).
// Renders a vertex-less fullscreen triangle (probe_render.vert) in a solid color
// (probe_render.frag, vec4(0.2,0.4,0.6,1.0)) into an offscreen 64x64
// R8G8B8A8_UNORM color target, then copies the image to a host-visible buffer and
// reads back the CENTER pixel. Run under nouveau_capture.so to grab NVK's
// known-good Turing 3D (0xC597) color-target setup + draw pushbuffer, so we can
// byte-diff a hand-built pure-ioctl render pushbuffer against it.
//
// Build: gcc -O2 vk_render.c -o vk_render -lvulkan
// Run:   NV_CAP_DIR=./nvcaprender LD_PRELOAD=./nouveau_capture.so ./vk_render
#include <vulkan/vulkan.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#define CK(x) do{ VkResult _r=(x); if(_r){ fprintf(stderr,"%s = %d\n",#x,_r); exit(2);} }while(0)

#define W 64
#define H 64
#define FMT VK_FORMAT_R8G8B8A8_UNORM

static uint32_t* load_spv(const char* path, size_t* nbytes){
    FILE* f=fopen(path,"rb"); if(!f){ perror(path); exit(2); }
    fseek(f,0,SEEK_END); long n=ftell(f); fseek(f,0,SEEK_SET);
    uint32_t* b=malloc(n); fread(b,1,n,f); fclose(f); *nbytes=n; return b;
}
static uint32_t mem_type(VkPhysicalDevice pd, uint32_t bits, VkMemoryPropertyFlags want){
    VkPhysicalDeviceMemoryProperties mp; vkGetPhysicalDeviceMemoryProperties(pd,&mp);
    for(uint32_t i=0;i<mp.memoryTypeCount;i++)
        if((bits&(1u<<i)) && (mp.memoryTypes[i].propertyFlags&want)==want) return i;
    fprintf(stderr,"no mem type %x\n",want); exit(2);
}
static VkShaderModule load_module(VkDevice dev, const char* path){
    size_t n; uint32_t* spv=load_spv(path,&n);
    VkShaderModuleCreateInfo smci={.sType=VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,.codeSize=n,.pCode=spv};
    VkShaderModule sm; CK(vkCreateShaderModule(dev,&smci,0,&sm)); free(spv); return sm;
}

int main(int argc, char** argv){
    const char* vert_path = argc>1 ? argv[1] : "probe_render.vert.spv";
    const char* frag_path = argc>2 ? argv[2] : "probe_render.frag.spv";

    // --- instance / physical device / GRAPHICS queue ---
    VkApplicationInfo app={.sType=VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName="mabda-render",.apiVersion=VK_API_VERSION_1_1};
    VkInstanceCreateInfo ici={.sType=VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,.pApplicationInfo=&app};
    VkInstance inst; CK(vkCreateInstance(&ici,0,&inst));
    uint32_t npd=1; VkPhysicalDevice pd; CK(vkEnumeratePhysicalDevices(inst,&npd,&pd));
    VkPhysicalDeviceProperties pp; vkGetPhysicalDeviceProperties(pd,&pp);
    fprintf(stderr,"device: %s\n",pp.deviceName);

    uint32_t nqf=0; vkGetPhysicalDeviceQueueFamilyProperties(pd,&nqf,0);
    VkQueueFamilyProperties qf[16]; if(nqf>16)nqf=16; vkGetPhysicalDeviceQueueFamilyProperties(pd,&nqf,qf);
    uint32_t gq=~0u; for(uint32_t i=0;i<nqf;i++) if(qf[i].queueFlags&VK_QUEUE_GRAPHICS_BIT){gq=i;break;}
    if(gq==~0u){ fprintf(stderr,"no graphics queue family\n"); exit(2); }
    fprintf(stderr,"graphics queue family: %u\n",gq);
    float prio=1.0f;
    VkDeviceQueueCreateInfo qci={.sType=VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex=gq,.queueCount=1,.pQueuePriorities=&prio};
    VkDeviceCreateInfo dci={.sType=VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount=1,.pQueueCreateInfos=&qci};
    VkDevice dev; CK(vkCreateDevice(pd,&dci,0,&dev));
    VkQueue q; vkGetDeviceQueue(dev,gq,0,&q);

    // --- offscreen 64x64 RGBA8 color target (OPTIMAL, DEVICE_LOCAL) ---
    VkImageCreateInfo imci={.sType=VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,.imageType=VK_IMAGE_TYPE_2D,
        .format=FMT,.extent={W,H,1},.mipLevels=1,.arrayLayers=1,
        .samples=VK_SAMPLE_COUNT_1_BIT,.tiling=VK_IMAGE_TILING_OPTIMAL,
        .usage=VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT|VK_IMAGE_USAGE_TRANSFER_SRC_BIT,
        .initialLayout=VK_IMAGE_LAYOUT_UNDEFINED};
    VkImage img; CK(vkCreateImage(dev,&imci,0,&img));
    VkMemoryRequirements imr; vkGetImageMemoryRequirements(dev,img,&imr);
    VkMemoryAllocateInfo imai={.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,.allocationSize=imr.size,
        .memoryTypeIndex=mem_type(pd,imr.memoryTypeBits,VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT)};
    VkDeviceMemory imem; CK(vkAllocateMemory(dev,&imai,0,&imem)); CK(vkBindImageMemory(dev,img,imem,0));

    VkImageViewCreateInfo ivci={.sType=VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,.image=img,
        .viewType=VK_IMAGE_VIEW_TYPE_2D,.format=FMT,
        .subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1}};
    VkImageView view; CK(vkCreateImageView(dev,&ivci,0,&view));

    // --- render pass: 1 color attachment, CLEAR->STORE, final TRANSFER_SRC ---
    VkAttachmentDescription att={.format=FMT,.samples=VK_SAMPLE_COUNT_1_BIT,
        .loadOp=VK_ATTACHMENT_LOAD_OP_CLEAR,.storeOp=VK_ATTACHMENT_STORE_OP_STORE,
        .stencilLoadOp=VK_ATTACHMENT_LOAD_OP_DONT_CARE,.stencilStoreOp=VK_ATTACHMENT_STORE_OP_DONT_CARE,
        .initialLayout=VK_IMAGE_LAYOUT_UNDEFINED,.finalLayout=VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL};
    VkAttachmentReference cref={.attachment=0,.layout=VK_IMAGE_LAYOUT_COLOR_ATTACHMENT_OPTIMAL};
    VkSubpassDescription sub={.pipelineBindPoint=VK_PIPELINE_BIND_POINT_GRAPHICS,
        .colorAttachmentCount=1,.pColorAttachments=&cref};
    VkRenderPassCreateInfo rpci={.sType=VK_STRUCTURE_TYPE_RENDER_PASS_CREATE_INFO,
        .attachmentCount=1,.pAttachments=&att,.subpassCount=1,.pSubpasses=&sub};
    VkRenderPass rp; CK(vkCreateRenderPass(dev,&rpci,0,&rp));

    VkFramebufferCreateInfo fbci={.sType=VK_STRUCTURE_TYPE_FRAMEBUFFER_CREATE_INFO,
        .renderPass=rp,.attachmentCount=1,.pAttachments=&view,.width=W,.height=H,.layers=1};
    VkFramebuffer fb; CK(vkCreateFramebuffer(dev,&fbci,0,&fb));

    // --- graphics pipeline: empty vertex input, triangle list, no depth/blend ---
    VkShaderModule vs=load_module(dev,vert_path);
    VkShaderModule fs=load_module(dev,frag_path);
    VkPipelineShaderStageCreateInfo stages[2]={
        {.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,.stage=VK_SHADER_STAGE_VERTEX_BIT,.module=vs,.pName="main"},
        {.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,.stage=VK_SHADER_STAGE_FRAGMENT_BIT,.module=fs,.pName="main"}};
    VkPipelineVertexInputStateCreateInfo vi={.sType=VK_STRUCTURE_TYPE_PIPELINE_VERTEX_INPUT_STATE_CREATE_INFO};
    VkPipelineInputAssemblyStateCreateInfo ia={.sType=VK_STRUCTURE_TYPE_PIPELINE_INPUT_ASSEMBLY_STATE_CREATE_INFO,
        .topology=VK_PRIMITIVE_TOPOLOGY_TRIANGLE_LIST};
    VkViewport vp={.x=0,.y=0,.width=W,.height=H,.minDepth=0,.maxDepth=1};
    VkRect2D sc={.offset={0,0},.extent={W,H}};
    VkPipelineViewportStateCreateInfo vps={.sType=VK_STRUCTURE_TYPE_PIPELINE_VIEWPORT_STATE_CREATE_INFO,
        .viewportCount=1,.pViewports=&vp,.scissorCount=1,.pScissors=&sc};
    VkPipelineRasterizationStateCreateInfo rs={.sType=VK_STRUCTURE_TYPE_PIPELINE_RASTERIZATION_STATE_CREATE_INFO,
        .polygonMode=VK_POLYGON_MODE_FILL,.cullMode=VK_CULL_MODE_NONE,
        .frontFace=VK_FRONT_FACE_COUNTER_CLOCKWISE,.lineWidth=1.0f};
    VkPipelineMultisampleStateCreateInfo ms={.sType=VK_STRUCTURE_TYPE_PIPELINE_MULTISAMPLE_STATE_CREATE_INFO,
        .rasterizationSamples=VK_SAMPLE_COUNT_1_BIT};
    VkPipelineDepthStencilStateCreateInfo dsst={.sType=VK_STRUCTURE_TYPE_PIPELINE_DEPTH_STENCIL_STATE_CREATE_INFO,
        .depthTestEnable=VK_FALSE,.depthWriteEnable=VK_FALSE};
    VkPipelineColorBlendAttachmentState cba={.blendEnable=VK_FALSE,
        .colorWriteMask=VK_COLOR_COMPONENT_R_BIT|VK_COLOR_COMPONENT_G_BIT|VK_COLOR_COMPONENT_B_BIT|VK_COLOR_COMPONENT_A_BIT};
    VkPipelineColorBlendStateCreateInfo cbs={.sType=VK_STRUCTURE_TYPE_PIPELINE_COLOR_BLEND_STATE_CREATE_INFO,
        .attachmentCount=1,.pAttachments=&cba};
    VkPipelineLayoutCreateInfo plci={.sType=VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};
    VkPipelineLayout pl; CK(vkCreatePipelineLayout(dev,&plci,0,&pl));
    VkGraphicsPipelineCreateInfo gpci={.sType=VK_STRUCTURE_TYPE_GRAPHICS_PIPELINE_CREATE_INFO,
        .stageCount=2,.pStages=stages,.pVertexInputState=&vi,.pInputAssemblyState=&ia,
        .pViewportState=&vps,.pRasterizationState=&rs,.pMultisampleState=&ms,
        .pDepthStencilState=&dsst,.pColorBlendState=&cbs,.layout=pl,.renderPass=rp,.subpass=0};
    VkPipeline pipe; CK(vkCreateGraphicsPipelines(dev,0,1,&gpci,0,&pipe));

    // --- host-visible readback buffer (64*64*4) ---
    uint32_t RBN=W*H*4;
    VkBufferCreateInfo bci={.sType=VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,.size=RBN,
        .usage=VK_BUFFER_USAGE_TRANSFER_DST_BIT};
    VkBuffer rbuf; CK(vkCreateBuffer(dev,&bci,0,&rbuf));
    VkMemoryRequirements bmr; vkGetBufferMemoryRequirements(dev,rbuf,&bmr);
    VkMemoryAllocateInfo bmai={.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,.allocationSize=bmr.size,
        .memoryTypeIndex=mem_type(pd,bmr.memoryTypeBits,VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)};
    VkDeviceMemory bmem; CK(vkAllocateMemory(dev,&bmai,0,&bmem)); CK(vkBindBufferMemory(dev,rbuf,bmem,0));
    uint8_t* bp; CK(vkMapMemory(dev,bmem,0,RBN,0,(void**)&bp)); memset(bp,0,RBN);

    // --- record: beginRP(CLEAR black) -> bind -> draw(3) -> endRP -> barrier -> copy ---
    VkCommandPoolCreateInfo cpc={.sType=VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,.queueFamilyIndex=gq};
    VkCommandPool cp; CK(vkCreateCommandPool(dev,&cpc,0,&cp));
    VkCommandBufferAllocateInfo cbai={.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool=cp,.level=VK_COMMAND_BUFFER_LEVEL_PRIMARY,.commandBufferCount=1};
    VkCommandBuffer cb; CK(vkAllocateCommandBuffers(dev,&cbai,&cb));

    VkCommandBufferBeginInfo cbbi={.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags=VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT};
    CK(vkBeginCommandBuffer(cb,&cbbi));
    VkClearValue clr={.color={.float32={0.0f,0.0f,0.0f,1.0f}}};
    VkRenderPassBeginInfo rpbi={.sType=VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO,
        .renderPass=rp,.framebuffer=fb,.renderArea={{0,0},{W,H}},.clearValueCount=1,.pClearValues=&clr};
    vkCmdBeginRenderPass(cb,&rpbi,VK_SUBPASS_CONTENTS_INLINE);
    vkCmdBindPipeline(cb,VK_PIPELINE_BIND_POINT_GRAPHICS,pipe);
    vkCmdDraw(cb,3,1,0,0);
    vkCmdEndRenderPass(cb);
    // barrier: color-attachment writes -> transfer read (image already in TRANSFER_SRC_OPTIMAL via finalLayout)
    VkImageMemoryBarrier ib={.sType=VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .oldLayout=VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,.newLayout=VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,
        .image=img,.subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1},
        .srcAccessMask=VK_ACCESS_COLOR_ATTACHMENT_WRITE_BIT,.dstAccessMask=VK_ACCESS_TRANSFER_READ_BIT};
    vkCmdPipelineBarrier(cb,VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT,VK_PIPELINE_STAGE_TRANSFER_BIT,0,0,0,0,0,1,&ib);
    VkBufferImageCopy bic={.imageSubresource={VK_IMAGE_ASPECT_COLOR_BIT,0,0,1},.imageExtent={W,H,1}};
    vkCmdCopyImageToBuffer(cb,img,VK_IMAGE_LAYOUT_TRANSFER_SRC_OPTIMAL,rbuf,1,&bic);
    CK(vkEndCommandBuffer(cb));

    VkFenceCreateInfo fci={.sType=VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
    VkFence fence; CK(vkCreateFence(dev,&fci,0,&fence));
    VkSubmitInfo si={.sType=VK_STRUCTURE_TYPE_SUBMIT_INFO,.commandBufferCount=1,.pCommandBuffers=&cb};
    fprintf(stderr,"=== submitting triangle draw ===\n");
    CK(vkQueueSubmit(q,1,&si,fence));
    CK(vkWaitForFences(dev,1,&fence,VK_TRUE,~0ull));

    // --- read back center pixel (32,32) ---
    uint32_t off=(32*W+32)*4;
    uint8_t r=bp[off+0],g=bp[off+1],b=bp[off+2],a=bp[off+3];
    uint32_t px=*(uint32_t*)(bp+off);
    fprintf(stderr,"center pixel @ (%d) = 0x%08x  RGBA=(%u,%u,%u,%u)\n",off,px,r,g,b,a);
    fprintf(stderr,"corner (0,0)        = 0x%08x\n",*(uint32_t*)(bp+0));
    // expect (51,102,153,255); PASS if clearly the solid color (within +-3, non-black)
    int pass = (a==255) && (r>=48&&r<=54) && (g>=99&&g<=105) && (b>=150&&b<=156);
    fprintf(stderr,"readback: 0x%08x (expect ~0xFF996633 / RGBA 51,102,153,255) -> %s\n",
            px, pass?"PASS":"FAIL");
    return pass?0:1;
}
