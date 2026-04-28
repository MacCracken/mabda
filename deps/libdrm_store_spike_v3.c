/*
 * deps/libdrm_store_spike_v3.c — Session 24 Mesa-shape submit
 *
 * Hypothesis: the recurring 0x66d000 CPC fault (Sessions 14-23) is caused
 * by the kernel synthesizing a fallback user-fence target VA when our CS
 * submission omits the AMDGPU_CHUNK_ID_FENCE chunk. Mesa's cl_probe sends
 * 4 chunks (BO_HANDLES, SYNCOBJ_OUT, FENCE, IB) — we send 3 (no FENCE).
 * Devcoredump (Session 23) showed the fault address is 8 pages above the
 * last MQD page, exactly inside the kernel's per-queue scratch / EOP /
 * HOQ allocation that backs the synthesized fallback fence.
 *
 * Diff captured by `LD_PRELOAD=build/libcsdump.so ./cl_probe` reveals
 * three additional Mesa-vs-spike deltas:
 *   1. BO_HANDLES `operation`/`list_handle` = 0xffffffff (inline magic),
 *      ours = 0 (CREATE op).
 *   2. IB `flags` = 0x00000008 (TC_WB_NOT_INVALIDATE), ours = 0.
 *   3. BO priorities non-zero, ours all 0. (Eviction hint; cosmetic.)
 *
 * This v3 spike adds the FENCE chunk, sets the BO_HANDLES inline magic,
 * sets the IB flag, and matches BO priorities — testing whether full
 * byte-shape parity makes the fault go away.
 *
 * Build:
 *   cc -O2 -Wall -o build/libdrm_store_spike_v3 deps/libdrm_store_spike_v3.c -ldrm_amdgpu
 * Run:
 *   ./build/libdrm_store_spike_v3
 *   CSDUMP=spike_v3.csdump LD_PRELOAD=$PWD/build/libcsdump.so ./build/libdrm_store_spike_v3
 * Expected:
 *   readback = 0xDEADBEEF (PASS)
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <fcntl.h>
#include <unistd.h>
#include <inttypes.h>
#include <time.h>
#include <errno.h>

#include <libdrm/amdgpu.h>
#include <libdrm/amdgpu_drm.h>

#define DIE(fmt, ...) do { fprintf(stderr, "FAIL: " fmt "\n", ##__VA_ARGS__); exit(1); } while (0)
#define CHECK(r, what) do { if ((r) != 0) DIE("%s: %d (%s)", what, r, strerror(-(r))); } while (0)

/* ---- PM4 helpers ---- */
#define PACKET3(opcode, count_minus_1) \
    (0xC0000000u | ((opcode) << 8) | (((count_minus_1) & 0x3FFFu) << 16))

#define IT_NOP                 0x10
#define IT_SET_UCONFIG_REG     0x79
#define IT_SET_SH_REG          0x76
#define IT_DISPATCH_DIRECT     0x15
#define IT_ACQUIRE_MEM         0x58

/* GFX9 register MMIO byte offsets (verified against kernel
 * drivers/gpu/drm/amd/include/asic_reg/gc/gc_9_0_offset.h × 4 + base). */
#define R_COMPUTE_NUM_THREAD_X            0xB81C
#define R_COMPUTE_PGM_LO                  0xB830
#define R_COMPUTE_PGM_HI                  0xB834
#define R_COMPUTE_PGM_RSRC1               0xB848
#define R_COMPUTE_PGM_RSRC2               0xB84C
#define R_COMPUTE_RESOURCE_LIMITS         0xB854
#define R_COMPUTE_STATIC_THREAD_MGMT_SE0  0xB858
#define R_COMPUTE_TMPRING_SIZE            0xB860
#define R_COMPUTE_STATIC_THREAD_MGMT_SE2  0xB864
#define R_COMPUTE_USER_DATA_0             0xB900
#define R_CP_COHER_START_DELAY            0x301EC
#define R_TA_CS_BC_BASE_ADDR              0x30E00

#define PM4_SH_REG_BASE        0xB000
#define PM4_UCONFIG_REG_BASE   0x30000

/* Mesa-verified compute-state magic numbers. */
#define GFX9_COMPUTE_PGM_RSRC1_MIN  0x002C0040u   /* see backend_native.cyr */
#define GFX9_DISPATCH_INITIATOR     0x45u         /* COMPUTE_SHADER_EN | FORCE_START_AT_000 */

/* PM4 emit helpers. `p` is a uint32_t* cursor; macros advance it.
 *
 * Session 14 fix: PACKET3 count_minus_1 = (body_dws - 1). Body_dws is the
 * number of DWs *after* the header. Pre-Session-14 this file passed
 * `body_dws` (one too high) for every helper, which the CP firmware partly
 * tolerated by silently consuming +1 DW per packet — masked the bug for
 * Sessions 7-13. Adding the WRITE_DATA + DMA_DATA packets at correct count
 * (they match Mesa byte-for-byte) desynced the mixed convention IB and
 * tripped "Illegal opcode" on Cezanne. Verified against Mesa AMD_DEBUG=ib
 * dump headers in build/strace/cl_probe_ib.log.
 */
static inline uint32_t *pm4_set_sh_reg_one(uint32_t *p, uint32_t reg_byte, uint32_t v) {
    *p++ = PACKET3(IT_SET_SH_REG, 1);                          /* body=2 (offset+value) */
    *p++ = (reg_byte - PM4_SH_REG_BASE) / 4;
    *p++ = v;
    return p;
}
static inline uint32_t *pm4_set_sh_reg_n(uint32_t *p, uint32_t reg_byte, unsigned n, const uint32_t *vals) {
    *p++ = PACKET3(IT_SET_SH_REG, n);                          /* body=n+1 (offset + n values) */
    *p++ = (reg_byte - PM4_SH_REG_BASE) / 4;
    for (unsigned i = 0; i < n; i++) *p++ = vals[i];
    return p;
}
static inline uint32_t *pm4_set_uconfig_one(uint32_t *p, uint32_t reg_byte, uint32_t v) {
    *p++ = PACKET3(IT_SET_UCONFIG_REG, 1);                     /* body=2 */
    *p++ = (reg_byte - PM4_UCONFIG_REG_BASE) / 4;
    *p++ = v;
    return p;
}
static inline uint32_t *pm4_set_uconfig_pair(uint32_t *p, uint32_t reg_byte, uint32_t lo, uint32_t hi) {
    *p++ = PACKET3(IT_SET_UCONFIG_REG, 2);                     /* body=3 (offset + lo + hi) */
    *p++ = (reg_byte - PM4_UCONFIG_REG_BASE) / 4;
    *p++ = lo;
    *p++ = hi;
    return p;
}
static inline uint32_t *pm4_acquire_mem_full(uint32_t *p) {
    /* PKT3 ACQUIRE_MEM with shader_type=2 in the predicate byte (Mesa pattern).
     * Header should match Mesa byte-exact: 0xC0055802. */
    *p++ = PACKET3(IT_ACQUIRE_MEM, 5) | 0x02;                  /* body=6 → 0xC0055802 */
    *p++ = 0xA8C40000u;                                        /* coher_cntl: full L2/I/K invalidate */
    *p++ = 0xFFFFFFFFu;                                        /* coher_size_lo */
    *p++ = 0x00FFFFFFu;                                        /* coher_size_hi (24-bit) */
    *p++ = 0;                                                  /* coher_base_lo */
    *p++ = 0;                                                  /* coher_base_hi */
    *p++ = 0x0A;                                               /* poll_interval */
    return p;
}
static inline uint32_t *pm4_dispatch_direct(uint32_t *p, uint32_t x, uint32_t y, uint32_t z) {
    /* Low byte of header = shader_type (2 = compute). Header matches Mesa byte-exact:
     * 0xC0031502 (count_minus_1 = 3 → body = 4 DWs: x, y, z, initiator). */
    *p++ = PACKET3(IT_DISPATCH_DIRECT, 3) | 0x02;              /* body=4 → 0xC0031502 */
    *p++ = x;
    *p++ = y;
    *p++ = z;
    *p++ = GFX9_DISPATCH_INITIATOR;
    return p;
}
/* Session 25: CPU-readable progress marker. WRITE_DATA(WR_CONFIRM=1, DST_SEL=5
 * MEMORY async, ENGINE_SEL=ME) writes one DW to a *mapped* user VA. WR_CONFIRM=1
 * means CP waits for the L2 ack — same packet shape that hung the spike at
 * IB_RPTR=23 in session 25, except now the target VA is real. If CP stalls
 * here too, the ack mechanism itself is broken; if it doesn't, each landed
 * marker tells us exactly how far the IB got. */
static inline uint32_t *pm4_write_data_marker(uint32_t *p, uint64_t dst_va, uint32_t value) {
    *p++ = PACKET3(0x37, 3);                                   /* WRITE_DATA, body=4 */
    *p++ = 0x00100500u;                                        /* DST_SEL=5, WR_CONFIRM=1, ENGINE=ME */
    *p++ = (uint32_t)(dst_va & 0xFFFFFFFFu);
    *p++ = (uint32_t)((dst_va >> 32) & 0xFFFFFFFFu);
    *p++ = value;
    return p;
}
static inline uint32_t *pm4_nop_pad(uint32_t *p, uint32_t *base, unsigned target_total_dw) {
    unsigned cur = (unsigned)(p - base);
    if (cur >= target_total_dw) return p;
    unsigned pad = target_total_dw - cur;
    *p++ = PACKET3(IT_NOP, pad);                               /* covers pad-1 body DWs after header */
    /* The pad-1 body DWs are already zero from memset. */
    p += (pad - 1);
    return p;
}

/* GFX9 store kernel — extracted from build/shader/spike.o (output of
 *    clang -target amdgcn--amdhsa -mcpu=gfx90c -O2 -c spike.cl
 * with kernel:
 *    __kernel void spike(__global uint *out) { *out = 0xDEADBEEF; }
 * 6 instructions, 9 DWs (36 bytes). USER_SGPR=2 reads {s0=va_lo, s1=va_hi}.
 */
/* Session 25 fix: prior code embedded bytes from build/shader/spike.o, which
 * is the *non-HSA* AMDPAL-style binary — those bytes start 0xBE8000FF = SOP1
 * `s_mov_b32 s0, 0x10000000`, i.e. scalar moves that *clobber* the USER_DATA
 * loaded into s0/s1 by the kernel-launch and substitute the shader's own
 * hardcoded buffer base. The comments labeled them as `v_mov_b32` but that
 * was a hex misread. Outcome: shader ran, stored 0xDEADBEEF, but to a wrong
 * address (somewhere derived from 0x10000000), so out_va kept its sentinel.
 *
 * The HSA-ABI binary (build/shader/spike_hsa.o) is what we actually want:
 * it uses VOP1 vector moves to copy USER_SGPR s0/s1 into v0/v1, then does a
 * FLAT-encoded global_store_dword v[0:1], v2 — exactly the .s source. Bytes
 * extracted via `xxd build/shader/spike_hsa.o` from offset 0x100 (.text).
 *
 * 8 DWs total (32 bytes). USER_SGPR=2 is the *minimum*; we set RSRC2=0x8 →
 * USER_SGPR=4, which still loads s0/s1 correctly (s2/s3 ignored by shader). */
static const uint32_t spike_shader_code[8] = {
    0x7E000200u,    /* v_mov_b32_e32 v0, s0                                          */
    0x7E020201u,    /* v_mov_b32_e32 v1, s1                                          */
    0x7E0402FFu,    /* v_mov_b32_e32 v2, lit                                         */
    0xDEADBEEFu,    /* literal value                                                 */
    0xDC708000u,    /* global_store_dword v[0:1], v2, off (FLAT/GLOBAL part 1)       */
    0x007F0200u,    /* part 2 (saddr=0x7F=NULL, vdata=v2, addr=v[0:1])               */
    0xBF8C0F70u,    /* s_waitcnt vmcnt(0) lgkmcnt(0)                                 */
    0xBF810000u,    /* s_endpgm                                                      */
};

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
    r = amdgpu_cs_ctx_create(dev, &ctx);
    CHECK(r, "amdgpu_cs_ctx_create");

    /* ---- Allocate 4 BOs: IB, shader, output, stub. ---- */
    /* The stub BO is per Session 5 finding: gfx9 derefs s[0:3] as a
     * scratch V# during dispatch setup even when SCRATCH_EN=0; pointing
     * USER_DATA_2/3 at any valid GTT VA satisfies the fetch. Output VA
     * goes in USER_DATA_0/1. RSRC2 USER_SGPR=4 → kernel loads s0..s3. */
    struct amdgpu_bo_alloc_request req = {
        .alloc_size = 4096, .phys_alignment = 4096,
        .preferred_heap = AMDGPU_GEM_DOMAIN_GTT, .flags = 0,
    };

    amdgpu_bo_handle ib_bo, shader_bo, out_bo, stub_bo, fence_bo;
    void *ib_cpu, *shader_cpu, *out_cpu, *stub_cpu, *fence_cpu;
    uint64_t ib_va, shader_va, out_va, stub_va, fence_va;
    amdgpu_va_handle ib_vah, shader_vah, out_vah, stub_vah, fence_vah;

#define ALLOC_BO(name, va_flags) do { \
    r = amdgpu_bo_alloc(dev, &req, &name##_bo); CHECK(r, "alloc " #name); \
    r = amdgpu_va_range_alloc(dev, amdgpu_gpu_va_range_general, 4096, 4096, 0, \
                              &name##_va, &name##_vah, (va_flags)); CHECK(r, "va_range_alloc " #name); \
    r = amdgpu_bo_va_op(name##_bo, 0, 4096, name##_va, 0, AMDGPU_VA_OP_MAP); CHECK(r, "va_op MAP " #name); \
    r = amdgpu_bo_cpu_map(name##_bo, &name##_cpu); CHECK(r, "cpu_map " #name); \
    memset(name##_cpu, 0, 4096); \
} while (0)

    /* Session 24 part 2: ALL BOs allocated in upper-canonical (HIGH VA range)
     * to match Mesa cl_probe byte-exactly. Mesa's IB/shader/out/stub/fence all
     * sit in 0xffff800x...; our previous v3 had only shader high — the IB and
     * data BOs were at 0x000000010000... user-low. Compute-queue UTCL2 may
     * require upper-canonical for store/IB-fetch routing on Cezanne. */
    ALLOC_BO(ib,     AMDGPU_VA_RANGE_HIGH);
    ALLOC_BO(fence,  AMDGPU_VA_RANGE_HIGH);
    /* Session 17 attempt 2: place shader BO at upper-canonical VA
     * (AMDGPU_VA_RANGE_HIGH = 0x2) to match Mesa's IB byte-exact for the
     * COMPUTE_PGM_LO/HI deltas (Mesa: PGM_HI=0x80 PGM_LO=0 → shader at
     * 0xFFFF800000000000+; ours was at user 0x100001000 → PGM_HI=0
     * PGM_LO=0x01000010). Hypothesis: CPC's internal CSA / wave-init
     * write path is gated on shader-VA-half (bit 47 = 1), and our
     * lower-canonical placement leaves CPC pointed at an uninitialized
     * default that derives the 0x66d000 fault address. */
    ALLOC_BO(shader, AMDGPU_VA_RANGE_HIGH);
    ALLOC_BO(out,    AMDGPU_VA_RANGE_HIGH);
    ALLOC_BO(stub,   AMDGPU_VA_RANGE_HIGH);

    fprintf(stderr, "ib_va=0x%016" PRIx64 " shader_va=0x%016" PRIx64
                    " out_va=0x%016" PRIx64 " stub_va=0x%016" PRIx64
                    " fence_va=0x%016" PRIx64 "\n",
            ib_va, shader_va, out_va, stub_va, fence_va);

    /* Seed output with a sentinel that is NOT 0xDEADBEEF to prove the shader wrote it. */
    *(uint32_t *)out_cpu = 0xBAADF00Du;

    /* Session 25: seed three CPU-readable IB-progress markers in fence_cpu.
     * Marker A at +0  (start-of-IB),
     * Marker B at +8  (after preamble, before DISPATCH_DIRECT),
     * Marker C at +16 (after DISPATCH_DIRECT, before terminator).
     * fence_cpu+32 is reserved for the kernel user-fence write (8 B), so we
     * stay clear of that window. WRITE_DATA(WR_CONFIRM=1) emits stall the
     * CP until each write is acked — if any landed value is the sentinel,
     * CP got stuck before that point. */
    *(uint32_t *)((uint8_t *)fence_cpu +  0) = 0xBAADF00Du;
    *(uint32_t *)((uint8_t *)fence_cpu +  8) = 0xBAADF00Du;
    *(uint32_t *)((uint8_t *)fence_cpu + 16) = 0xBAADF00Du;

    /* ---- Write shader code into shader BO. ---- */
    memcpy(shader_cpu, spike_shader_code, sizeof spike_shader_code);

    /* ---- Build PM4: full Mesa-rusticl byte-exact preamble + dispatch. ---- */
    uint32_t *ib = (uint32_t *)ib_cpu;
    uint32_t *p = ib;

    /* GFX9 PGM_LO/HI encoding (from radv / amdgpu kernel):
     *   PGM_LO = (va >> 8) & 0xFFFFFFFF   — bits [39:8] of VA in 32-bit register
     *   PGM_HI = (va >> 40) & 0xFF        — bits [47:40] of VA in low byte
     * Hardware reconstructs:  shader_addr = (PGM_HI << 40) | (PGM_LO << 8).
     * Session 13 fix: prior code stuffed VA[31:0] into PGM_LO and VA[39:8] into
     * PGM_HI — CP saw shader at (raw_va << 8), fetched garbage, hung every IB. */
    uint32_t pgm_lo = (uint32_t)((shader_va >> 8) & 0xFFFFFFFFu);
    uint32_t pgm_hi = (uint32_t)((shader_va >> 40) & 0xFFu);

    /* Session 25 marker A — fires before any preamble work. If fence+0 stays
     * 0xBAADF00D after submit, CP never started fetching; if it lands as
     * 0xCAFEBABE we know MEC at least entered our IB and cleared one
     * WR_CONFIRM round-trip. */
    p = pm4_write_data_marker(p, fence_va + 0, 0xCAFEBABEu);
    /* 1. PGM_HI (early). */
    p = pm4_set_sh_reg_one(p, R_COMPUTE_PGM_HI, pgm_hi);
    /* 2. STATIC_THREAD_MGMT_SE0 = 0xFFFFFFFF (enable all CUs in SE0), SE1 = 0 (Cezanne has 1 SE). */
    {
        uint32_t v[2] = { 0xFFFFFFFFu, 0u };
        p = pm4_set_sh_reg_n(p, R_COMPUTE_STATIC_THREAD_MGMT_SE0, 2, v);
    }
    /* 3. STATIC_THREAD_MGMT_SE2/SE3 = 0, 0. */
    {
        uint32_t v[2] = { 0u, 0u };
        p = pm4_set_sh_reg_n(p, R_COMPUTE_STATIC_THREAD_MGMT_SE2, 2, v);
    }
    /* 4. CP_COHER_START_DELAY = 0. */
    p = pm4_set_uconfig_one(p, R_CP_COHER_START_DELAY, 0);
    /* 5. TA_CS_BC_BASE_ADDR + _HI — border-color base.
     *
     * Session 25: previously set to (LO=0x01004400, HI=0x80) which the comment
     * called "Mesa kernel-reserved magic VA the firmware accepts as a no-op."
     * That assumption was wrong. The reconstructed VA 0xFFFF800100440000 is
     * Mesa's *own* allocated BC stub BO in *Mesa's* address space — we have
     * nothing mapped there. Set both halves to 0 instead. Our shader is bare
     * `global_store_dword`, no samplers, so CPC has no reason to walk the BC
     * descriptor. The session-16 hypothesis ("CPC walks BC base regardless")
     * was derived from an interpretation of the 0x66d000 fault that we now
     * know was actually a kernel reset-replay artifact of the WRITE_DATA hang
     * removed below — not a CPC-walk derivation. If CPC really does walk it
     * we'll learn that from the next devcd. */
    p = pm4_set_uconfig_pair(p, R_TA_CS_BC_BASE_ADDR,
                             0x00000000u,   /* LO = 0 */
                             0x00000000u);  /* HI = 0 */
    /* 5a. (REMOVED) WRITE_DATA zero-fence to 0xFFFF800100600300.
     * That packet was the proximate hang point in session 25: WR_CONFIRM=1
     * targeting an unmapped VA → CPC issues the write, UTCL1 walks the PTE,
     * faults, and CPC waits forever for an ack. IP State dump showed
     * IB_RPTR=23 = exactly past this packet, IB_BASE=0 (cleared on reset),
     * CP_CPC_STALLED_STAT1=0x01000000, CPC_UTCL1_STATUS=0x000f0002 (pending
     * walk). Removing this packet eliminates the unmapped-VA stall. */
    /* 6. PGM_LO. */
    p = pm4_set_sh_reg_one(p, R_COMPUTE_PGM_LO, pgm_lo);
    /* 7. PGM_RSRC1 + RSRC2 as a pair. RSRC2 = 0x8 → USER_SGPR=4 (loads s0..s3). */
    {
        uint32_t v[2] = { GFX9_COMPUTE_PGM_RSRC1_MIN, 0x00000008u };
        p = pm4_set_sh_reg_n(p, R_COMPUTE_PGM_RSRC1, 2, v);
    }
    /* 8. TMPRING_SIZE = 0x100. Mesa always sets this even when scratch is unused. */
    p = pm4_set_sh_reg_one(p, R_COMPUTE_TMPRING_SIZE, 0x100);
    /* 9-10. Session 18 final state: real values, Mesa-style packet shapes.
     *
     * Hypothesis E was: the count=2 SET_SH_REG for USER_DATA_0/1 triggered
     * the 0x66d000 CPC fault. Attempts 3 and 4 ran without that fault and
     * SEEMED to confirm it. Attempt 5 falsified it: replacing the store
     * kernel with bare s_endpgm brought the fault back even with these
     * Mesa-style packet shapes. So the fault is in CPC's *post-dispatch
     * cleanup* (CSA save / wave fini), and attempts 3-4 only hid it
     * because the global_store hung in s_waitcnt — wave never completed.
     *
     * Keeping these Mesa-style packet shapes for Session 19 — they are
     * Mesa-byte-exact in IB structure, which is the right baseline.
     * Investigation pivots to KERNEL queue setup: amdgpu_cs_ctx_create
     * vs whatever Mesa's rusticl uses, CSA region setup, queue init.
     */
    {
        uint32_t v[2] = { (uint32_t)(stub_va & 0xFFFFFFFFu),
                          (uint32_t)((stub_va >> 32) & 0xFFFFFFFFu) };
        p = pm4_set_sh_reg_n(p, R_COMPUTE_USER_DATA_0 + 8 /* USER_DATA_2 */, 2, v);
    }
    /* Session 21: pack USER_DATA_0/1 as a single count=2 SET_SH_REG packet
     * (matches Mesa IB1 byte-exact for the USER_DATA_0/1 emit; was previously
     * two count=1 packets — the only remaining packet-shape delta). */
    {
        uint32_t v[2] = { (uint32_t)(out_va & 0xFFFFFFFFu),
                          (uint32_t)((out_va >> 32) & 0xFFFFFFFFu) };
        p = pm4_set_sh_reg_n(p, R_COMPUTE_USER_DATA_0, 2, v);
    }
    /* 11. ACQUIRE_MEM (mandatory cache invalidate before dispatch on GFX9). */
    p = pm4_acquire_mem_full(p);
    /* 11a. (REMOVED) DMA_DATA NOWHERE source 0xFFFF800000000000.
     * Same Mesa-magic-VA mistake as 5a — that source VA is in Mesa's address
     * space, not ours. DMA_DATA reads 96 B via TC L2 from an unmapped VA
     * would page-fault. We never reached this packet in session 25 (hang at
     * idx 23 well before here), but removing it pre-empts a second fault. */
    /* 12. RESOURCE_LIMITS = 0x140 (WAVES_PER_SH=320). Zero stalls forever. */
    p = pm4_set_sh_reg_one(p, R_COMPUTE_RESOURCE_LIMITS, 0x140);
    /* 13. NUM_THREAD_X/Y/Z = 1, 1, 1. */
    {
        uint32_t v[3] = { 1, 1, 1 };
        p = pm4_set_sh_reg_n(p, R_COMPUTE_NUM_THREAD_X, 3, v);
    }
    /* Session 25 marker B — fires after preamble register-set + ACQUIRE_MEM,
     * before DISPATCH_DIRECT. 0xFEEDFACE landing means the entire compute-
     * state setup completed cleanly. */
    p = pm4_write_data_marker(p, fence_va + 8, 0xFEEDFACEu);
    /* 14. DISPATCH_DIRECT (1, 1, 1). */
    p = pm4_dispatch_direct(p, 1, 1, 1);
    /* Session 25 marker C — fires after DISPATCH_DIRECT returns control to
     * CP. 0xC0FFEE12 landing means CP got the dispatch back without faulting.
     * (DISPATCH_DIRECT is fire-and-forget for waves; CP doesn't wait on
     * shader completion here, only on dispatch acceptance.) */
    p = pm4_write_data_marker(p, fence_va + 16, 0xC0FFEE12u);
    /* 15. Trailing DMA_DATA CP_SYNC=1 BYTE_COUNT=0 terminator (Mesa cl_probe IB
     * byte-exact, dump line 165-186). 7-DW DMA_DATA(CP_SYNC=1, ENGINE=ME,
     * DST_SEL=DST_ADDR_TC_L2, SRC_SEL=SRC_ADDR_TC_L2, BYTE_COUNT=0) — a no-op
     * transfer that forces the CP to wait for outstanding work before
     * signaling the user fence. Session 17 attempt 1: closes a Mesa delta;
     * unlikely to fix the 0x66d000 CPC setup fault on its own (fault is
     * pre-dispatch), but eliminates one variable. */
    *p++ = PACKET3(0x50, 5);                                /* DMA_DATA, body=6 */
    *p++ = 0xE0300000u;                                     /* word0: CP_SYNC=1 */
    *p++ = 0x00000000u;                                     /* SRC_LO */
    *p++ = 0x00000000u;                                     /* SRC_HI */
    *p++ = 0x00000000u;                                     /* DST_LO */
    *p++ = 0x00000000u;                                     /* DST_HI */
    *p++ = 0x00000000u;                                     /* COMMAND: BYTE_COUNT=0 */
    /* Session 25: pad to 80 DWs total (multiple of 8 = 32 B) via a single NOP
     * packet emitted by hand — pm4_nop_pad mis-computes count_minus_1 (passes
     * `pad` instead of `pad-2`) which leaves the NOP packet's claimed body
     * extending 2 DWs past the cursor. Direct emission keeps the math
     * obvious: cur DWs used so far, target = 80, NOP packet covers
     * (80 - cur) DWs total = header + (80 - cur - 1) body DWs, so
     * count_minus_1 = (80 - cur - 2). Body bytes are already zero from
     * memset(ib_cpu, 0, 4096). */
    {
        unsigned cur = (unsigned)(p - ib);
        if (cur < 80) {
            unsigned pad_total = 80 - cur;
            if (pad_total >= 2) {
                *p++ = PACKET3(IT_NOP, (uint32_t)(pad_total - 2));
                p += (pad_total - 1);
            }
        }
    }
    unsigned ib_dws_used = (unsigned)(p - ib);

    fprintf(stderr, "pm4 IB built: %u DWs\n", ib_dws_used);

    /* Session 17 diagnostic: dump IB hex so we can byte-exact diff against
     * Mesa AMD_DEBUG=ib output. 8 DWs per line, hex offset on the left.
     * Lossless: no IB content change. Always emit; cheap. */
    fprintf(stderr, "---- IB DUMP (%u DWs) ----\n", ib_dws_used);
    for (unsigned i = 0; i < ib_dws_used; i += 8) {
        fprintf(stderr, "%04x:", i);
        for (unsigned j = i; j < i + 8 && j < ib_dws_used; j++) {
            fprintf(stderr, " %08x", ib[j]);
        }
        fprintf(stderr, "\n");
    }
    fprintf(stderr, "---- IB DUMP END ----\n");

    /* ---- Session 24: 4-chunk Mesa-shape submit.
     *
     * cl_probe csdump (docs/issues/session24/cl_probe.csdump) shows Mesa
     * sends 4 chunks in this order: BO_HANDLES, SYNCOBJ_OUT, FENCE, IB.
     * The FENCE chunk gives the kernel an explicit user-fence target —
     * without it the kernel falls back to a per-context allocation that
     * appears to land in the kernel's MQD-adjacent scratch region (Session
     * 23 devcoredump showed the 0x66d000 fault is exactly 8 pages above
     * the last MQD page). Adding the FENCE chunk should make the fallback
     * unnecessary.
     */

    /* Resolve KMS handles for BO_HANDLES chunk. */
    uint32_t ib_kh, shader_kh, out_kh, stub_kh, fence_kh;
    r = amdgpu_bo_export(ib_bo,     amdgpu_bo_handle_type_kms, &ib_kh);     CHECK(r, "bo_export ib");
    r = amdgpu_bo_export(shader_bo, amdgpu_bo_handle_type_kms, &shader_kh); CHECK(r, "bo_export shader");
    r = amdgpu_bo_export(out_bo,    amdgpu_bo_handle_type_kms, &out_kh);    CHECK(r, "bo_export out");
    r = amdgpu_bo_export(stub_bo,   amdgpu_bo_handle_type_kms, &stub_kh);   CHECK(r, "bo_export stub");
    r = amdgpu_bo_export(fence_bo,  amdgpu_bo_handle_type_kms, &fence_kh);  CHECK(r, "bo_export fence");

    /* BO priorities mirror Mesa's pattern (eviction hint; values inferred
     * from cl_probe csdump: ib≈10, shader≈4, out≈3, stub≈3, fence≈1). */
    struct drm_amdgpu_bo_list_entry bo_entries[5] = {
        { .bo_handle = fence_kh,  .bo_priority = 1 },
        { .bo_handle = stub_kh,   .bo_priority = 3 },
        { .bo_handle = out_kh,    .bo_priority = 3 },
        { .bo_handle = shader_kh, .bo_priority = 4 },
        { .bo_handle = ib_kh,     .bo_priority = 10 },
    };

    /* BO_HANDLES inline-magic: operation=0xffffffff, list_handle=0xffffffff
     * tells the kernel "no persistent BO list, parse entries from
     * bo_info_ptr". Mesa cl_probe uses these values verbatim. Our previous
     * spike used operation=0 (CREATE), which causes the kernel to allocate
     * and track a persistent BO list — different code path. */
    struct drm_amdgpu_bo_list_in bo_handles_data = {
        .operation    = 0xffffffffu,
        .list_handle  = 0xffffffffu,
        .bo_number    = 5,
        .bo_info_size = (uint32_t)sizeof(struct drm_amdgpu_bo_list_entry),
        .bo_info_ptr  = (uint64_t)(uintptr_t)bo_entries,
    };

    /* Output syncobj — kernel signals it when the IB completes. */
    uint32_t signal_syncobj = 0;
    r = amdgpu_cs_create_syncobj(dev, &signal_syncobj);
    CHECK(r, "amdgpu_cs_create_syncobj");

    /* User-fence chunk — the kernel writes the IB completion sequence
     * number to fence_va + offset when the IB completes. Mesa offsets to
     * 32 within a 64-byte fence BO; we match. */
    *(uint64_t *)((uint8_t *)fence_cpu + 32) = 0;
    struct drm_amdgpu_cs_chunk_fence fence_chunk_data = {
        .handle = fence_kh,
        .offset = 32,
    };

    const char *env_ring     = getenv("SPIKE_RING");
    const char *env_instance = getenv("SPIKE_IP_INSTANCE");
    uint32_t spike_ring     = env_ring     ? (uint32_t)atoi(env_ring)     : 0;
    uint32_t spike_instance = env_instance ? (uint32_t)atoi(env_instance) : 0;
    fprintf(stderr, "submit: ip_type=COMPUTE ip_instance=%u ring=%u (Mesa-shape v3)\n",
            spike_instance, spike_ring);

    struct drm_amdgpu_cs_chunk_ib ib_chunk_data = {
        ._pad        = 0,
        .flags       = AMDGPU_IB_FLAG_TC_WB_NOT_INVALIDATE, /* 0x08, matches Mesa */
        .va_start    = ib_va,
        .ib_bytes    = ib_dws_used * 4u,
        .ip_type     = AMDGPU_HW_IP_COMPUTE,
        .ip_instance = spike_instance,
        .ring        = spike_ring,
    };
    struct drm_amdgpu_cs_chunk_sem sem_chunk_data = { .handle = signal_syncobj };

    /* Chunk order matches Mesa cl_probe byte-exactly:
     * [0] BO_HANDLES, [1] SYNCOBJ_OUT, [2] FENCE, [3] IB. */
    struct drm_amdgpu_cs_chunk chunks[4] = {
        { .chunk_id   = AMDGPU_CHUNK_ID_BO_HANDLES,
          .length_dw  = (uint32_t)(sizeof(bo_handles_data) / 4),
          .chunk_data = (uint64_t)(uintptr_t)&bo_handles_data },
        { .chunk_id   = AMDGPU_CHUNK_ID_SYNCOBJ_OUT,
          .length_dw  = (uint32_t)(sizeof(sem_chunk_data) / 4),
          .chunk_data = (uint64_t)(uintptr_t)&sem_chunk_data },
        { .chunk_id   = AMDGPU_CHUNK_ID_FENCE,
          .length_dw  = (uint32_t)(sizeof(fence_chunk_data) / 4),
          .chunk_data = (uint64_t)(uintptr_t)&fence_chunk_data },
        { .chunk_id   = AMDGPU_CHUNK_ID_IB,
          .length_dw  = (uint32_t)(sizeof(ib_chunk_data) / 4),
          .chunk_data = (uint64_t)(uintptr_t)&ib_chunk_data },
    };

    uint64_t seq_no = 0;
    uint64_t t0 = mono_ms();
    r = amdgpu_cs_submit_raw2(dev, ctx, 0 /* no bo_list_handle */, 4, chunks, &seq_no);
    CHECK(r, "amdgpu_cs_submit_raw2");

    /* DRM_IOCTL_SYNCOBJ_WAIT takes an *absolute* CLOCK_MONOTONIC nsec
     * deadline, not a relative duration. */
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    int64_t deadline_ns = (int64_t)now.tv_sec * 1000000000ll
                        + (int64_t)now.tv_nsec
                        + 5000000000ll /* 5 s */;

    uint32_t first_signaled = 0;
    r = amdgpu_cs_syncobj_wait(dev, &signal_syncobj, 1,
                               deadline_ns, 0 /* flags */, &first_signaled);
    if (r == 0) {
        fprintf(stderr, "submit-to-syncobj-signal: %" PRIu64 " ms\n", mono_ms() - t0);
    } else {
        /* Per Session 18: even on timeout the wave may have completed
         * before the post-dispatch CPC fault — read out[0] to find out. */
        fprintf(stderr, "syncobj_wait: %d (%s) after %" PRIu64 " ms — checking out[0] anyway\n",
                r, strerror(-r), mono_ms() - t0);
    }

    uint32_t got = *(uint32_t *)out_cpu;
    /* Session 25 progress markers — print before the PASS/FAIL line so
     * partial-progress is visible even if the dispatch itself failed. */
    uint32_t mA = *(uint32_t *)((uint8_t *)fence_cpu +  0);
    uint32_t mB = *(uint32_t *)((uint8_t *)fence_cpu +  8);
    uint32_t mC = *(uint32_t *)((uint8_t *)fence_cpu + 16);
    fprintf(stderr,
            "marker A (start)   = 0x%08X %s\n"
            "marker B (pre-dsp) = 0x%08X %s\n"
            "marker C (post-dsp)= 0x%08X %s\n",
            mA, mA == 0xCAFEBABEu ? "OK"  : "STALE",
            mB, mB == 0xFEEDFACEu ? "OK"  : "STALE",
            mC, mC == 0xC0FFEE12u ? "OK"  : "STALE");
    printf("out[0] = 0x%08X (want 0xDEADBEEF) — %s\n",
           got, got == 0xDEADBEEFu ? "PASS" : "FAIL");

    /* Best-effort teardown. */
    amdgpu_cs_destroy_syncobj(dev, signal_syncobj);
#define FREE_BO(name) do { \
    amdgpu_bo_cpu_unmap(name##_bo); \
    amdgpu_bo_va_op(name##_bo, 0, 4096, name##_va, 0, AMDGPU_VA_OP_UNMAP); \
    amdgpu_va_range_free(name##_vah); \
    amdgpu_bo_free(name##_bo); \
} while (0)
    FREE_BO(ib); FREE_BO(shader); FREE_BO(out); FREE_BO(stub); FREE_BO(fence);
    amdgpu_cs_ctx_free(ctx);
    amdgpu_device_deinitialize(dev);
    close(fd);

    return got == 0xDEADBEEFu ? 0 : 1;
}
