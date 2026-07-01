// vk_compute.c — minimal HEADLESS Vulkan compute dispatch for the mabda
// N0.7(c) capture. No surface/window — runs over SSH on NVK. Loads a
// SPIR-V compute shader (default probe.spv: out[0]=0xDEADBEEF), dispatches
// 1 workgroup to a host-visible buffer, reads it back. Run under the
// nouveau_capture.so interposer to grab NVK's known-good QMD + pushbuffer.
//
// Build: gcc -O2 vk_compute.c -o vk_compute -lvulkan
// Run:   NV_CAP_DIR=./nvcap LD_PRELOAD=./nouveau_capture.so ./vk_compute probe.spv
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

int main(int argc, char** argv){
    const char* spv_path = argc>1 ? argv[1] : "probe.spv";

    VkApplicationInfo app={.sType=VK_STRUCTURE_TYPE_APPLICATION_INFO,
        .pApplicationName="mabda-cap",.apiVersion=VK_API_VERSION_1_1};
    VkInstanceCreateInfo ici={.sType=VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO,.pApplicationInfo=&app};
    VkInstance inst; CK(vkCreateInstance(&ici,0,&inst));

    uint32_t npd=0; CK(vkEnumeratePhysicalDevices(inst,&npd,0));
    if(!npd){ fprintf(stderr,"no vulkan device\n"); return 2; }
    VkPhysicalDevice pds[8]; if(npd>8)npd=8; CK(vkEnumeratePhysicalDevices(inst,&npd,pds));
    VkPhysicalDevice pd=pds[0];
    VkPhysicalDeviceProperties pp; vkGetPhysicalDeviceProperties(pd,&pp);
    fprintf(stderr,"device: %s\n",pp.deviceName);

    uint32_t nqf=0; vkGetPhysicalDeviceQueueFamilyProperties(pd,&nqf,0);
    VkQueueFamilyProperties qf[16]; if(nqf>16)nqf=16; vkGetPhysicalDeviceQueueFamilyProperties(pd,&nqf,qf);
    uint32_t cq=~0u; for(uint32_t i=0;i<nqf;i++) if(qf[i].queueFlags&VK_QUEUE_COMPUTE_BIT){cq=i;break;}
    if(cq==~0u){ fprintf(stderr,"no compute queue\n"); return 2; }

    float prio=1.0f;
    VkDeviceQueueCreateInfo qci={.sType=VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO,
        .queueFamilyIndex=cq,.queueCount=1,.pQueuePriorities=&prio};
    VkDeviceCreateInfo dci={.sType=VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO,
        .queueCreateInfoCount=1,.pQueueCreateInfos=&qci};
    VkDevice dev; CK(vkCreateDevice(pd,&dci,0,&dev));
    VkQueue q; vkGetDeviceQueue(dev,cq,0,&q);

    // host-visible storage buffer (4 bytes)
    VkBufferCreateInfo bci={.sType=VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO,
        .size=4,.usage=VK_BUFFER_USAGE_STORAGE_BUFFER_BIT,.sharingMode=VK_SHARING_MODE_EXCLUSIVE};
    VkBuffer buf; CK(vkCreateBuffer(dev,&bci,0,&buf));
    VkMemoryRequirements mr; vkGetBufferMemoryRequirements(dev,buf,&mr);
    VkPhysicalDeviceMemoryProperties mp; vkGetPhysicalDeviceMemoryProperties(pd,&mp);
    uint32_t mt=~0u;
    for(uint32_t i=0;i<mp.memoryTypeCount;i++){
        if((mr.memoryTypeBits&(1u<<i)) &&
           (mp.memoryTypes[i].propertyFlags&VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT) &&
           (mp.memoryTypes[i].propertyFlags&VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)){ mt=i; break; }
    }
    if(mt==~0u){ fprintf(stderr,"no host-visible mem\n"); return 2; }
    VkMemoryAllocateInfo mai={.sType=VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO,
        .allocationSize=mr.size,.memoryTypeIndex=mt};
    VkDeviceMemory mem; CK(vkAllocateMemory(dev,&mai,0,&mem));
    CK(vkBindBufferMemory(dev,buf,mem,0));
    uint32_t* host; CK(vkMapMemory(dev,mem,0,4,0,(void**)&host)); *host=0;

    // descriptor + pipeline
    VkDescriptorSetLayoutBinding b0={.binding=0,.descriptorType=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,
        .descriptorCount=1,.stageFlags=VK_SHADER_STAGE_COMPUTE_BIT};
    VkDescriptorSetLayoutCreateInfo dslci={.sType=VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO,
        .bindingCount=1,.pBindings=&b0};
    VkDescriptorSetLayout dsl; CK(vkCreateDescriptorSetLayout(dev,&dslci,0,&dsl));
    VkPipelineLayoutCreateInfo plci={.sType=VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO,
        .setLayoutCount=1,.pSetLayouts=&dsl};
    VkPipelineLayout pl; CK(vkCreatePipelineLayout(dev,&plci,0,&pl));

    size_t spvn; uint32_t* spv=load_spv(spv_path,&spvn);
    VkShaderModuleCreateInfo smci={.sType=VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO,
        .codeSize=spvn,.pCode=spv};
    VkShaderModule sm; CK(vkCreateShaderModule(dev,&smci,0,&sm));
    VkComputePipelineCreateInfo cpci={.sType=VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO,
        .stage={.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO,
                .stage=VK_SHADER_STAGE_COMPUTE_BIT,.module=sm,.pName="main"},
        .layout=pl};
    VkPipeline pipe; CK(vkCreateComputePipelines(dev,0,1,&cpci,0,&pipe));

    VkDescriptorPoolSize dps={.type=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,.descriptorCount=1};
    VkDescriptorPoolCreateInfo dpci={.sType=VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO,
        .maxSets=1,.poolSizeCount=1,.pPoolSizes=&dps};
    VkDescriptorPool dp; CK(vkCreateDescriptorPool(dev,&dpci,0,&dp));
    VkDescriptorSetAllocateInfo dsai={.sType=VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO,
        .descriptorPool=dp,.descriptorSetCount=1,.pSetLayouts=&dsl};
    VkDescriptorSet ds; CK(vkAllocateDescriptorSets(dev,&dsai,&ds));
    VkDescriptorBufferInfo dbi={.buffer=buf,.offset=0,.range=4};
    VkWriteDescriptorSet w={.sType=VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET,.dstSet=ds,
        .dstBinding=0,.descriptorCount=1,.descriptorType=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,.pBufferInfo=&dbi};
    vkUpdateDescriptorSets(dev,1,&w,0,0);

    VkCommandPoolCreateInfo cpc={.sType=VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO,.queueFamilyIndex=cq};
    VkCommandPool cp; CK(vkCreateCommandPool(dev,&cpc,0,&cp));
    VkCommandBufferAllocateInfo cbai={.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO,
        .commandPool=cp,.level=VK_COMMAND_BUFFER_LEVEL_PRIMARY,.commandBufferCount=1};
    VkCommandBuffer cb; CK(vkAllocateCommandBuffers(dev,&cbai,&cb));
    VkCommandBufferBeginInfo cbbi={.sType=VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO,
        .flags=VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT};
    CK(vkBeginCommandBuffer(cb,&cbbi));
    vkCmdBindPipeline(cb,VK_PIPELINE_BIND_POINT_COMPUTE,pipe);
    vkCmdBindDescriptorSets(cb,VK_PIPELINE_BIND_POINT_COMPUTE,pl,0,1,&ds,0,0);
    vkCmdDispatch(cb,1,1,1);
    CK(vkEndCommandBuffer(cb));

    VkFenceCreateInfo fci={.sType=VK_STRUCTURE_TYPE_FENCE_CREATE_INFO};
    VkFence fence; CK(vkCreateFence(dev,&fci,0,&fence));
    VkSubmitInfo si={.sType=VK_STRUCTURE_TYPE_SUBMIT_INFO,.commandBufferCount=1,.pCommandBuffers=&cb};
    fprintf(stderr,"=== submitting compute dispatch ===\n");
    CK(vkQueueSubmit(q,1,&si,fence));
    CK(vkWaitForFences(dev,1,&fence,VK_TRUE,~0ull));

    fprintf(stderr,"readback: 0x%08x (expect 0xdeadbeef) -> %s\n",
            *host, *host==0xDEADBEEFu?"PASS":"FAIL");
    return *host==0xDEADBEEFu?0:1;
}
