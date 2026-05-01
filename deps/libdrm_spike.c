/*
 * deps/libdrm_spike.c — Session 11 diagnostic, v3 Phase B.3.d
 *
 * Minimal libdrm_amdgpu CS submission. Writes a single PKT3 WRITE_DATA
 * packet to a GTT BO via the GFX ring and reads back the value. No
 * compute dispatch, no shader, no rasterization — purely "did the CP
 * execute the IB at all" via a CPU-readable side-channel.
 *
 * Purpose: differential diagnostic against `programs/native_compute_spike.cyr`.
 * The Cyrius spike submits a byte-equivalent IB via direct ioctls and the
 * CP never executes it (10s TDR every time, dmesg silent, stub stays 0).
 * If THIS program lands `0xCAFEBABE` in dst[0] on the same hardware,
 * the bug is purely in our direct-ioctl path — strace-diff to find the
 * delta. If this also fails, there's a deeper kernel/process-state issue.
 *
 * Build:
 *   cc -O2 -Wall -o build/libdrm_spike deps/libdrm_spike.c -ldrm_amdgpu
 * Run:
 *   ./build/libdrm_spike
 * Expected:
 *   readback = 0xCAFEBABE  (PASS)
 *
 * Reference: grate-driver/libdrm tests/amdgpu/basic_tests.c (canonical
 * WRITE_DATA-readback pattern), igt-gpu-tools lib/amdgpu/.
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <fcntl.h>
#include <unistd.h>
#include <errno.h>
#include <inttypes.h>
#include <time.h>

#include <libdrm/amdgpu.h>
#include <libdrm/amdgpu_drm.h>

#define DIE(fmt, ...) do { fprintf(stderr, "FAIL: " fmt "\n", ##__VA_ARGS__); exit(1); } while (0)
#define CHECK(r, what) do { if ((r) != 0) DIE("%s: %d (%s)", what, r, strerror(-(r))); } while (0)

/* PM4 PKT3 helpers — matches our Cyrius spike byte-for-byte. */
#define PACKET3(opcode, count_minus_1) \
    (0xC0000000 | ((opcode) << 8) | (((count_minus_1) & 0x3FFF) << 16))

#define IT_WRITE_DATA  0x37
#define IT_NOP         0x10

/* WRITE_DATA control word bits. */
#define WD_DST_SEL_MEM_ASYNC  (5 << 8)
#define WD_WR_ONE_ADDR        (1 << 16)
#define WD_WR_CONFIRM         (1 << 20)

/* GFX9 ring align_mask = 0xff → IB length must be a multiple of 256 DW.
 * (kernel auto-pads via amdgpu_ring_generic_pad_ib but we pad ourselves
 * to keep this program byte-exact comparable to the Cyrius spike.) */
#define IB_DW_TOTAL    256

static uint64_t mono_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint64_t)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

int main(void) {
    int fd = open("/dev/dri/renderD128", O_RDWR | O_CLOEXEC);
    if (fd < 0) DIE("open: %s", strerror(errno));

    amdgpu_device_handle dev;
    uint32_t major, minor;
    int r = amdgpu_device_initialize(fd, &major, &minor, &dev);
    CHECK(r, "amdgpu_device_initialize");
    fprintf(stderr, "libdrm_amdgpu init OK (drm %u.%u)\n", major, minor);

    amdgpu_context_handle ctx;
    /* AMDGPU_CTX_PRIORITY_NORMAL = 0 (matches default of cs_ctx_create), but
     * Mesa uses cs_ctx_create2 explicitly. Test whether path matters. */
    r = amdgpu_cs_ctx_create2(dev, 0 /* AMDGPU_CTX_PRIORITY_NORMAL */, &ctx);
    CHECK(r, "amdgpu_cs_ctx_create2");

    /* Sanity: confirm GFX ring 0 is available (matches our Cyrius diagnostic). */
    struct drm_amdgpu_info_hw_ip hw_ip = {0};
    r = amdgpu_query_hw_ip_info(dev, AMDGPU_HW_IP_GFX, 0, &hw_ip);
    CHECK(r, "amdgpu_query_hw_ip_info GFX");
    fprintf(stderr, "GFX available_rings=0x%x ib_start_align=%u ib_size_align=%u\n",
            hw_ip.available_rings, hw_ip.ib_start_alignment, hw_ip.ib_size_alignment);
    if ((hw_ip.available_rings & 1) == 0) DIE("GFX ring 0 not available");

    /* ---- DST BO: 4 KiB GTT, holds the WRITE_DATA target word. ---- */
    struct amdgpu_bo_alloc_request dst_req = {
        .alloc_size      = 4096,
        .phys_alignment  = 4096,
        .preferred_heap  = AMDGPU_GEM_DOMAIN_GTT,
        .flags           = 0,
    };
    amdgpu_bo_handle dst_bo;
    r = amdgpu_bo_alloc(dev, &dst_req, &dst_bo);
    CHECK(r, "amdgpu_bo_alloc dst");

    uint64_t dst_va;
    amdgpu_va_handle dst_va_handle;
    r = amdgpu_va_range_alloc(dev, amdgpu_gpu_va_range_general,
                              4096, 4096, 0, &dst_va, &dst_va_handle, 0);
    CHECK(r, "amdgpu_va_range_alloc dst");
    r = amdgpu_bo_va_op(dst_bo, 0, 4096, dst_va, 0, AMDGPU_VA_OP_MAP);
    CHECK(r, "amdgpu_bo_va_op dst MAP");

    void *dst_cpu;
    r = amdgpu_bo_cpu_map(dst_bo, &dst_cpu);
    CHECK(r, "amdgpu_bo_cpu_map dst");
    memset(dst_cpu, 0, 4096);

    fprintf(stderr, "dst_va=0x%016" PRIx64 "\n", dst_va);

    /* ---- IB BO: 4 KiB GTT, holds the PM4 stream. ---- */
    struct amdgpu_bo_alloc_request ib_req = {
        .alloc_size      = 4096,
        .phys_alignment  = 4096,
        .preferred_heap  = AMDGPU_GEM_DOMAIN_GTT,
        .flags           = 0,
    };
    amdgpu_bo_handle ib_bo;
    r = amdgpu_bo_alloc(dev, &ib_req, &ib_bo);
    CHECK(r, "amdgpu_bo_alloc ib");

    uint64_t ib_va;
    amdgpu_va_handle ib_va_handle;
    r = amdgpu_va_range_alloc(dev, amdgpu_gpu_va_range_general,
                              4096, 4096, 0, &ib_va, &ib_va_handle, 0);
    CHECK(r, "amdgpu_va_range_alloc ib");
    r = amdgpu_bo_va_op(ib_bo, 0, 4096, ib_va, 0, AMDGPU_VA_OP_MAP);
    CHECK(r, "amdgpu_bo_va_op ib MAP");

    uint32_t *ib_cpu;
    r = amdgpu_bo_cpu_map(ib_bo, (void **)&ib_cpu);
    CHECK(r, "amdgpu_bo_cpu_map ib");
    memset(ib_cpu, 0, 4096);

    fprintf(stderr, "ib_va=0x%016" PRIx64 "\n", ib_va);

    /* ---- Build PM4: PKT3 WRITE_DATA(5 DW) + IT_NOP padding to IB_DW_TOTAL. ---- */
    uint32_t *p = ib_cpu;
    *p++ = PACKET3(IT_WRITE_DATA, 3);                                   /* count=3 → 4 body DWs */
    *p++ = WD_WR_CONFIRM | WD_DST_SEL_MEM_ASYNC;
    *p++ = (uint32_t)(dst_va & 0xFFFFFFFFu);
    *p++ = (uint32_t)(dst_va >> 32);
    *p++ = 0xCAFEBABEu;
    uint32_t used_dw = (uint32_t)(p - ib_cpu);
    uint32_t pad_dw  = IB_DW_TOTAL - used_dw;
    *p++ = PACKET3(IT_NOP, pad_dw - 1);
    /* The pad_dw-1 body DWs that follow are already zero from memset. */

    /* ---- BO list: IB + DST. ---- */
    amdgpu_bo_handle bos[2] = { ib_bo, dst_bo };
    amdgpu_bo_list_handle bo_list;
    r = amdgpu_bo_list_create(dev, 2, bos, NULL, &bo_list);
    CHECK(r, "amdgpu_bo_list_create");

    /* ---- Submit. ---- */
    struct amdgpu_cs_ib_info ib_info = {
        .ib_mc_address = ib_va,
        .size          = IB_DW_TOTAL,    /* in DWORDS */
        .flags         = 0,
    };
    struct amdgpu_cs_request req = {
        .flags         = 0,
        .ip_type       = AMDGPU_HW_IP_GFX,
        .ip_instance   = 0,
        .ring          = 0,
        .resources     = bo_list,
        .number_of_ibs = 1,
        .ibs           = &ib_info,
        .number_of_dependencies = 0,
        .dependencies  = NULL,
    };
    uint64_t t0 = mono_ms();
    r = amdgpu_cs_submit(ctx, 0, &req, 1);
    CHECK(r, "amdgpu_cs_submit");

    /* ---- Wait. ---- */
    struct amdgpu_cs_fence fence = {
        .context     = ctx,
        .ip_type     = AMDGPU_HW_IP_GFX,
        .ip_instance = 0,
        .ring        = 0,
        .fence       = req.seq_no,
    };
    uint32_t expired = 0;
    r = amdgpu_cs_query_fence_status(&fence, 15000000000ull /* 15s ns */, 0, &expired);
    CHECK(r, "amdgpu_cs_query_fence_status");
    if (!expired) DIE("fence not expired (timeout) after %" PRIu64 " ms", mono_ms() - t0);
    fprintf(stderr, "submit-to-fence: %" PRIu64 " ms\n", mono_ms() - t0);

    /* ---- Readback. ---- */
    uint32_t got = ((uint32_t *)dst_cpu)[0];
    printf("readback = 0x%08X (want 0xCAFEBABE) — %s\n",
           got, got == 0xCAFEBABEu ? "PASS" : "FAIL");

    /* ---- Teardown (best-effort). ---- */
    amdgpu_bo_list_destroy(bo_list);
    amdgpu_bo_cpu_unmap(ib_bo);
    amdgpu_bo_va_op(ib_bo, 0, 4096, ib_va, 0, AMDGPU_VA_OP_UNMAP);
    amdgpu_va_range_free(ib_va_handle);
    amdgpu_bo_free(ib_bo);
    amdgpu_bo_cpu_unmap(dst_bo);
    amdgpu_bo_va_op(dst_bo, 0, 4096, dst_va, 0, AMDGPU_VA_OP_UNMAP);
    amdgpu_va_range_free(dst_va_handle);
    amdgpu_bo_free(dst_bo);
    amdgpu_cs_ctx_free(ctx);
    amdgpu_device_deinitialize(dev);
    close(fd);

    return got == 0xCAFEBABEu ? 0 : 1;
}
