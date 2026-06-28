// vk_compute_tex.c — minimal HEADLESS Vulkan compute dispatch that SAMPLES a
// 2D RGBA8 texture (mabda N6.2 capture). Creates a 1x1 R8G8B8A8_UNORM image,
// uploads a known texel, samples it (NEAREST) in a compute shader, packs to
// RGBA8 and stores to a buffer, reads it back. Run under nouveau_capture.so to
// grab NVK's known-good TIC (texture image control) + TSC (sampler control)
// descriptors + the sampling SASS + the SET_TEX_*_POOL binding.
//
// Build: gcc -O2 vk_compute_tex.c -o vk_compute_tex -lvulkan
// Run:   NV_CAP_DIR=./nvcaptex LD_PRELOAD=./nouveau_capture.so ./vk_compute_tex probe_tex.spv
#include <vulkan/vulkan.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

#define CK(x) do{ VkResult _r=(x); if(_r){ fprintf(stderr,"%s = %d\n",#x,_r); exit(2);} }while(0)

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

int main(int argc, char** argv){
    const char* spv_path = argc>1 ? argv[1] : "probe_tex.spv";
    // Optional W H (default 1x1) — used to triangulate the TIC width/height/
    // pitch fields by diffing TICs across sizes (mabda N6.2b decode).
    uint32_t TW = argc>2 ? (uint32_t)atoi(argv[2]) : 1;
    uint32_t TH = argc>3 ? (uint32_t)atoi(argv[3]) : 1;
    if(!TW)TW=1; if(!TH)TH=1;
    fprintf(stderr,"texture %ux%u RGBA8\n",TW,TH);

    VkApplicationInfo app={.sType=VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName="mabda-captex",.apiVersion=VK_API_VERSION_1_1};
    VkInstanceCreateInfo ici={.sType=VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,.pApplicationInfo=&app};
    VkInstance inst; CK(vkCreateInstance(&ici,0,&inst));
    uint32_t npd=1; VkPhysicalDevice pd; CK(vkEnumeratePhysicalDevices(inst,&npd,&pd));
    VkPhysicalDeviceProperties pp; vkGetPhysicalDeviceProperties(pd,&pp);
    fprintf(stderr,"device: %s\n",pp.deviceName);

    uint32_t nqf=0; vkGetPhysicalDeviceQueueFamilyProperties(pd,&nqf,0);
    VkQueueFamilyProperties qf[16]; if(nqf>16)nqf=16; vkGetPhysicalDeviceQueueFamilyProperties(pd,&nqf,qf);
    uint32_t cq=~0u; for(uint32_t i=0;i<nqf;i++) if(qf[i].queueFlags&VK_QUEUE_COMPUTE_BIT){cq=i;break;}
    float prio=1.0f;
    VkDeviceQueueCreateInfo qci={.sType=VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex=cq,.queueCount=1,.pQueuePriorities=&prio};
    VkDeviceCreateInfo dci={.sType=VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount=1,.pQueueCreateInfos=&qci};
    VkDevice dev; CK(vkCreateDevice(pd,&dci,0,&dev));
    VkQueue q; vkGetDeviceQueue(dev,cq,0,&q);

    // --- 1x1 RGBA8 sampled image (OPTIMAL tiling) ---
    VkImageCreateInfo ici2={.sType=VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO,.imageType=VK_IMAGE_TYPE_2D,
        .format=VK_FORMAT_R8G8B8A8_UNORM,.extent={TW,TH,1},.mipLevels=1,.arrayLayers=1,
        .samples=VK_SAMPLE_COUNT_1_BIT,.tiling=VK_IMAGE_TILING_OPTIMAL,
        .usage=VK_IMAGE_USAGE_SAMPLED_BIT|VK_IMAGE_USAGE_TRANSFER_DST_BIT,
        .initialLayout=VK_IMAGE_LAYOUT_UNDEFINED};
    VkImage img; CK(vkCreateImage(dev,&ici2,0,&img));
    VkMemoryRequirements imr; vkGetImageMemoryRequirements(dev,img,&imr);
    VkMemoryAllocateInfo imai={.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,.allocationSize=imr.size,
        .memoryTypeIndex=mem_type(pd,imr.memoryTypeBits,VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT)};
    VkDeviceMemory imem; CK(vkAllocateMemory(dev,&imai,0,&imem)); CK(vkBindImageMemory(dev,img,imem,0));

    // staging buffer; texel (0,0) = RGBA (0x11,0x22,0x33,0x44), rest a ramp.
    uint32_t TXN = TW*TH*4;
    VkBufferCreateInfo sbci={.sType=VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,.size=TXN,
        .usage=VK_BUFFER_USAGE_TRANSFER_SRC_BIT};
    VkBuffer sbuf; CK(vkCreateBuffer(dev,&sbci,0,&sbuf));
    VkMemoryRequirements smr; vkGetBufferMemoryRequirements(dev,sbuf,&smr);
    VkMemoryAllocateInfo smai={.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,.allocationSize=smr.size,
        .memoryTypeIndex=mem_type(pd,smr.memoryTypeBits,VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)};
    VkDeviceMemory smem; CK(vkAllocateMemory(dev,&smai,0,&smem)); CK(vkBindBufferMemory(dev,sbuf,smem,0));
    uint8_t* sp; CK(vkMapMemory(dev,smem,0,TXN,0,(void**)&sp));
    for(uint32_t i=0;i<TXN;i++) sp[i]=(uint8_t)i;
    sp[0]=0x11;sp[1]=0x22;sp[2]=0x33;sp[3]=0x44;

    // output storage buffer (host-visible, 4 bytes).
    VkBufferCreateInfo obci={.sType=VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,.size=4,
        .usage=VK_BUFFER_USAGE_STORAGE_BUFFER_BIT};
    VkBuffer obuf; CK(vkCreateBuffer(dev,&obci,0,&obuf));
    VkMemoryRequirements omr; vkGetBufferMemoryRequirements(dev,obuf,&omr);
    VkMemoryAllocateInfo omai={.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,.allocationSize=omr.size,
        .memoryTypeIndex=mem_type(pd,omr.memoryTypeBits,VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT|VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)};
    VkDeviceMemory omem; CK(vkAllocateMemory(dev,&omai,0,&omem)); CK(vkBindBufferMemory(dev,obuf,omem,0));
    uint32_t* op; CK(vkMapMemory(dev,omem,0,4,0,(void**)&op)); *op=0;

    VkCommandPoolCreateInfo cpc={.sType=VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,.queueFamilyIndex=cq};
    VkCommandPool cp; CK(vkCreateCommandPool(dev,&cpc,0,&cp));
    VkCommandBufferAllocateInfo cbai={.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool=cp,.level=VK_COMMAND_BUFFER_LEVEL_PRIMARY,.commandBufferCount=1};
    VkCommandBuffer cb; CK(vkAllocateCommandBuffers(dev,&cbai,&cb));

    // --- upload: transition + copy + transition to SHADER_READ_ONLY ---
    VkCommandBufferBeginInfo cbbi={.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags=VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT};
    CK(vkBeginCommandBuffer(cb,&cbbi));
    VkImageMemoryBarrier b1={.sType=VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .oldLayout=VK_IMAGE_LAYOUT_UNDEFINED,.newLayout=VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,
        .image=img,.subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1},.dstAccessMask=VK_ACCESS_TRANSFER_WRITE_BIT};
    vkCmdPipelineBarrier(cb,VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT,VK_PIPELINE_STAGE_TRANSFER_BIT,0,0,0,0,0,1,&b1);
    VkBufferImageCopy bic={.imageSubresource={VK_IMAGE_ASPECT_COLOR_BIT,0,0,1},.imageExtent={TW,TH,1}};
    vkCmdCopyBufferToImage(cb,sbuf,img,VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,1,&bic);
    VkImageMemoryBarrier b2={.sType=VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER,
        .oldLayout=VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL,.newLayout=VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL,
        .image=img,.subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1},
        .srcAccessMask=VK_ACCESS_TRANSFER_WRITE_BIT,.dstAccessMask=VK_ACCESS_SHADER_READ_BIT};
    vkCmdPipelineBarrier(cb,VK_PIPELINE_STAGE_TRANSFER_BIT,VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,0,0,0,0,0,1,&b2);
    CK(vkEndCommandBuffer(cb));
    VkSubmitInfo si0={.sType=VK_STRUCTURE_TYPE_SUBMIT_INFO,.commandBufferCount=1,.pCommandBuffers=&cb};
    CK(vkQueueSubmit(q,1,&si0,0)); CK(vkQueueWaitIdle(q));

    // image view + sampler (NEAREST / clamp).
    VkImageViewCreateInfo ivci={.sType=VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO,.image=img,
        .viewType=VK_IMAGE_VIEW_TYPE_2D,.format=VK_FORMAT_R8G8B8A8_UNORM,
        .subresourceRange={VK_IMAGE_ASPECT_COLOR_BIT,0,1,0,1}};
    VkImageView view; CK(vkCreateImageView(dev,&ivci,0,&view));
    VkSamplerCreateInfo sci={.sType=VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO,
        .magFilter=VK_FILTER_NEAREST,.minFilter=VK_FILTER_NEAREST,.mipmapMode=VK_SAMPLER_MIPMAP_MODE_NEAREST,
        .addressModeU=VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,.addressModeV=VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,
        .addressModeW=VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE,.maxLod=0.0f};
    VkSampler samp; CK(vkCreateSampler(dev,&sci,0,&samp));

    // descriptors: binding0 = combined image sampler, binding1 = storage buffer.
    VkDescriptorSetLayoutBinding lb[2]={
        {.binding=0,.descriptorType=VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,.descriptorCount=1,.stageFlags=VK_SHADER_STAGE_COMPUTE_BIT},
        {.binding=1,.descriptorType=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,.descriptorCount=1,.stageFlags=VK_SHADER_STAGE_COMPUTE_BIT}};
    VkDescriptorSetLayoutCreateInfo dslci={.sType=VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,.bindingCount=2,.pBindings=lb};
    VkDescriptorSetLayout dsl; CK(vkCreateDescriptorSetLayout(dev,&dslci,0,&dsl));
    VkPipelineLayoutCreateInfo plci={.sType=VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,.setLayoutCount=1,.pSetLayouts=&dsl};
    VkPipelineLayout pl; CK(vkCreatePipelineLayout(dev,&plci,0,&pl));

    size_t spvn; uint32_t* spv=load_spv(spv_path,&spvn);
    VkShaderModuleCreateInfo smci={.sType=VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,.codeSize=spvn,.pCode=spv};
    VkShaderModule sm; CK(vkCreateShaderModule(dev,&smci,0,&sm));
    VkComputePipelineCreateInfo cpci={.sType=VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
        .stage={.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,.stage=VK_SHADER_STAGE_COMPUTE_BIT,.module=sm,.pName="main"},.layout=pl};
    VkPipeline pipe; CK(vkCreateComputePipelines(dev,0,1,&cpci,0,&pipe));

    VkDescriptorPoolSize dps[2]={{.type=VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,.descriptorCount=1},
        {.type=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,.descriptorCount=1}};
    VkDescriptorPoolCreateInfo dpci={.sType=VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,.maxSets=1,.poolSizeCount=2,.pPoolSizes=dps};
    VkDescriptorPool dp; CK(vkCreateDescriptorPool(dev,&dpci,0,&dp));
    VkDescriptorSetAllocateInfo dsai={.sType=VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,.descriptorPool=dp,.descriptorSetCount=1,.pSetLayouts=&dsl};
    VkDescriptorSet ds; CK(vkAllocateDescriptorSets(dev,&dsai,&ds));
    VkDescriptorImageInfo dii={.sampler=samp,.imageView=view,.imageLayout=VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL};
    VkDescriptorBufferInfo dbi={.buffer=obuf,.offset=0,.range=4};
    VkWriteDescriptorSet w[2]={
        {.sType=VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,.dstSet=ds,.dstBinding=0,.descriptorCount=1,.descriptorType=VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER,.pImageInfo=&dii},
        {.sType=VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,.dstSet=ds,.dstBinding=1,.descriptorCount=1,.descriptorType=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,.pBufferInfo=&dbi}};
    vkUpdateDescriptorSets(dev,2,w,0,0);

    // --- dispatch the sampling shader ---
    CK(vkResetCommandBuffer(cb,0));
    CK(vkBeginCommandBuffer(cb,&cbbi));
    vkCmdBindPipeline(cb,VK_PIPELINE_BIND_POINT_COMPUTE,pipe);
    vkCmdBindDescriptorSets(cb,VK_PIPELINE_BIND_POINT_COMPUTE,pl,0,1,&ds,0,0);
    vkCmdDispatch(cb,1,1,1);
    CK(vkEndCommandBuffer(cb));
    VkFenceCreateInfo fci={.sType=VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
    VkFence fence; CK(vkCreateFence(dev,&fci,0,&fence));
    VkSubmitInfo si={.sType=VK_STRUCTURE_TYPE_SUBMIT_INFO,.commandBufferCount=1,.pCommandBuffers=&cb};
    fprintf(stderr,"=== submitting sampling dispatch ===\n");
    CK(vkQueueSubmit(q,1,&si,fence));
    CK(vkWaitForFences(dev,1,&fence,VK_TRUE,~0ull));

    fprintf(stderr,"readback: 0x%08x (expect 0x44332211) -> %s\n",*op,*op==0x44332211u?"PASS":"FAIL");
    return *op==0x44332211u?0:1;
}
