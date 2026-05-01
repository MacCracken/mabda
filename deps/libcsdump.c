/*
 * deps/libcsdump.c — LD_PRELOAD shim that dumps drm_amdgpu_cs chunks
 * and BO_LIST entries at the DRM_IOCTL_AMDGPU_CS boundary.
 *
 * Session 24 priority 1: capture byte-exact ioctl payloads from both
 * Mesa's cl_probe (success) and our spike (fault) for diff analysis.
 *
 * Build:
 *   cc -O2 -Wall -fPIC -shared -o build/libcsdump.so deps/libcsdump.c -ldl
 * Use:
 *   CSDUMP=/tmp/cl_probe.csdump LD_PRELOAD=./build/libcsdump.so ./build/shader/cl_probe
 *   CSDUMP=/tmp/spike.csdump   LD_PRELOAD=./build/libcsdump.so ./build/libdrm_store_spike
 *
 * Output (one record per CS / BO_LIST ioctl) is line-buffered text with hex
 * dumps of every chunk struct and chunk_data payload. Captures:
 *   - drm_amdgpu_cs_in (ctx_id, bo_list_handle, num_chunks, flags)
 *   - each chunk header (id, length_dw)
 *   - each chunk_data payload, decoded for known chunk_ids and hex-dumped
 *   - drm_amdgpu_bo_list_in + per-entry handle + priority
 *   - drm_amdgpu_gem_va (op, handle, va_address, map_size, flags)
 *
 * Always passes the ioctl through to the real implementation; never
 * mutates payloads.
 */

#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdarg.h>
#include <string.h>
#include <stdarg.h>
#include <dlfcn.h>
#include <sys/ioctl.h>
#include <fcntl.h>
#include <errno.h>
#include <unistd.h>

#include <libdrm/amdgpu_drm.h>

static int (*real_ioctl)(int, unsigned long, ...) = NULL;
static FILE *out = NULL;
static int initialized = 0;
static int cs_seq = 0;
static int bo_list_seq = 0;
static int va_seq = 0;

static void csd_init(void) {
    if (initialized) return;
    initialized = 1;
    real_ioctl = dlsym(RTLD_NEXT, "ioctl");
    const char *path = getenv("CSDUMP");
    if (path && *path) {
        out = fopen(path, "w");
        if (!out) out = stderr;
    } else {
        out = stderr;
    }
    setvbuf(out, NULL, _IOLBF, 0);
}

static void hex_dump(const char *label, const void *p, size_t n) {
    const unsigned char *b = (const unsigned char *)p;
    fprintf(out, "%s (%zu bytes)\n", label, n);
    for (size_t i = 0; i < n; i += 16) {
        fprintf(out, "  %04zx:", i);
        for (size_t j = 0; j < 16 && i + j < n; j++) fprintf(out, " %02x", b[i + j]);
        fprintf(out, "\n");
    }
}

static const char *chunk_name(uint32_t id) {
    switch (id) {
        case AMDGPU_CHUNK_ID_IB: return "IB";
        case AMDGPU_CHUNK_ID_FENCE: return "FENCE";
        case AMDGPU_CHUNK_ID_DEPENDENCIES: return "DEPENDENCIES";
        case AMDGPU_CHUNK_ID_SYNCOBJ_IN: return "SYNCOBJ_IN";
        case AMDGPU_CHUNK_ID_SYNCOBJ_OUT: return "SYNCOBJ_OUT";
        case AMDGPU_CHUNK_ID_BO_HANDLES: return "BO_HANDLES";
        case AMDGPU_CHUNK_ID_SCHEDULED_DEPENDENCIES: return "SCHED_DEPENDENCIES";
        case AMDGPU_CHUNK_ID_SYNCOBJ_TIMELINE_WAIT: return "SYNCOBJ_TIMELINE_WAIT";
        case AMDGPU_CHUNK_ID_SYNCOBJ_TIMELINE_SIGNAL: return "SYNCOBJ_TIMELINE_SIGNAL";
        case AMDGPU_CHUNK_ID_CP_GFX_SHADOW: return "CP_GFX_SHADOW";
        default: return "UNKNOWN";
    }
}

static const char *ip_type_name(uint32_t t) {
    switch (t) {
        case 0: return "GFX";
        case 1: return "COMPUTE";
        case 2: return "DMA";
        case 3: return "UVD";
        case 4: return "VCE";
        case 5: return "UVD_ENC";
        case 6: return "VCN_DEC";
        case 7: return "VCN_ENC";
        case 8: return "VCN_JPEG";
        default: return "?";
    }
}

static void decode_ib_chunk(const struct drm_amdgpu_cs_chunk_ib *ib) {
    fprintf(out, "    IB: _pad=0x%08x flags=0x%08x va_start=0x%016llx ib_bytes=%u ip_type=%u(%s) ip_instance=%u ring=%u\n",
            ib->_pad, ib->flags, (unsigned long long)ib->va_start,
            ib->ib_bytes, ib->ip_type, ip_type_name(ib->ip_type),
            ib->ip_instance, ib->ring);
}

static void decode_fence_chunk(const struct drm_amdgpu_cs_chunk_fence *f) {
    fprintf(out, "    FENCE: handle=%u offset=%u\n", f->handle, f->offset);
}

static void decode_syncobj_chunk(const void *p, uint32_t length_dw) {
    const struct drm_amdgpu_cs_chunk_syncobj *s = p;
    size_t n = (size_t)length_dw * 4 / sizeof(*s);
    fprintf(out, "    SYNCOBJ entries=%zu:\n", n);
    for (size_t i = 0; i < n; i++) {
        fprintf(out, "      [%zu] handle=%u flags=0x%x point=0x%016llx\n",
                i, s[i].handle, s[i].flags, (unsigned long long)s[i].point);
    }
}

static void decode_bo_handles_chunk(const void *p, uint32_t length_dw) {
    /* BO_HANDLES chunk_data is `struct drm_amdgpu_bo_list_in` followed by
     * an array of `struct drm_amdgpu_bo_list_entry` at bo_info_ptr. */
    const struct drm_amdgpu_bo_list_in *in = p;
    fprintf(out, "    BO_HANDLES: op=%u list_handle=%u bo_number=%u bo_info_size=%u bo_info_ptr=0x%016llx\n",
            in->operation, in->list_handle, in->bo_number, in->bo_info_size,
            (unsigned long long)in->bo_info_ptr);
    if (in->bo_info_ptr && in->bo_info_size >= sizeof(struct drm_amdgpu_bo_list_entry)) {
        const struct drm_amdgpu_bo_list_entry *e =
            (const struct drm_amdgpu_bo_list_entry *)(uintptr_t)in->bo_info_ptr;
        for (uint32_t i = 0; i < in->bo_number; i++) {
            fprintf(out, "      [%u] handle=%u priority=%u\n",
                    i, e[i].bo_handle, e[i].bo_priority);
        }
    }
    (void)length_dw;
}

static void dump_cs(const union drm_amdgpu_cs *cs) {
    int seq = ++cs_seq;
    fprintf(out, "==== CS #%d ====\n", seq);
    fprintf(out, "  ctx_id=%u bo_list_handle=%u num_chunks=%u flags=0x%08x chunks_ptr=0x%016llx\n",
            cs->in.ctx_id, cs->in.bo_list_handle, cs->in.num_chunks, cs->in.flags,
            (unsigned long long)cs->in.chunks);

    if (!cs->in.chunks || !cs->in.num_chunks) return;

    const uint64_t *chunk_ptrs = (const uint64_t *)(uintptr_t)cs->in.chunks;
    for (uint32_t i = 0; i < cs->in.num_chunks; i++) {
        const struct drm_amdgpu_cs_chunk *c =
            (const struct drm_amdgpu_cs_chunk *)(uintptr_t)chunk_ptrs[i];
        if (!c) {
            fprintf(out, "  chunk[%u] = NULL\n", i);
            continue;
        }
        fprintf(out, "  chunk[%u] id=%u(%s) length_dw=%u chunk_data=0x%016llx\n",
                i, c->chunk_id, chunk_name(c->chunk_id), c->length_dw,
                (unsigned long long)c->chunk_data);

        const void *data = (const void *)(uintptr_t)c->chunk_data;
        size_t bytes = (size_t)c->length_dw * 4;
        if (!data || !bytes) continue;

        switch (c->chunk_id) {
            case AMDGPU_CHUNK_ID_IB:
                if (bytes >= sizeof(struct drm_amdgpu_cs_chunk_ib))
                    decode_ib_chunk((const struct drm_amdgpu_cs_chunk_ib *)data);
                break;
            case AMDGPU_CHUNK_ID_FENCE:
                if (bytes >= sizeof(struct drm_amdgpu_cs_chunk_fence))
                    decode_fence_chunk((const struct drm_amdgpu_cs_chunk_fence *)data);
                break;
            case AMDGPU_CHUNK_ID_SYNCOBJ_IN:
            case AMDGPU_CHUNK_ID_SYNCOBJ_OUT:
                decode_syncobj_chunk(data, c->length_dw);
                break;
            case AMDGPU_CHUNK_ID_BO_HANDLES:
                decode_bo_handles_chunk(data, c->length_dw);
                break;
            default:
                break;
        }
        hex_dump("    raw chunk_data", data, bytes);
    }
    fflush(out);
}

static void dump_bo_list(const union drm_amdgpu_bo_list *bl) {
    int seq = ++bo_list_seq;
    fprintf(out, "==== BO_LIST #%d ====\n", seq);
    fprintf(out, "  op=%u list_handle=%u bo_number=%u bo_info_size=%u bo_info_ptr=0x%016llx\n",
            bl->in.operation, bl->in.list_handle, bl->in.bo_number, bl->in.bo_info_size,
            (unsigned long long)bl->in.bo_info_ptr);
    if (bl->in.bo_info_ptr && bl->in.bo_info_size >= sizeof(struct drm_amdgpu_bo_list_entry)) {
        const struct drm_amdgpu_bo_list_entry *e =
            (const struct drm_amdgpu_bo_list_entry *)(uintptr_t)bl->in.bo_info_ptr;
        for (uint32_t i = 0; i < bl->in.bo_number; i++) {
            fprintf(out, "    [%u] handle=%u priority=%u\n",
                    i, e[i].bo_handle, e[i].bo_priority);
        }
    }
    fflush(out);
}

static void dump_gem_va(const struct drm_amdgpu_gem_va *va) {
    int seq = ++va_seq;
    fprintf(out, "==== GEM_VA #%d ====\n", seq);
    fprintf(out, "  op=%u flags=0x%08x handle=%u _pad=%u va_address=0x%016llx offset=0x%016llx map_size=%llu\n",
            va->operation, va->flags, va->handle, va->_pad,
            (unsigned long long)va->va_address,
            (unsigned long long)va->offset_in_bo,
            (unsigned long long)va->map_size);
    fflush(out);
}

int ioctl(int fd, unsigned long request, ...) {
    csd_init();

    va_list ap;
    va_start(ap, request);
    void *arg = va_arg(ap, void *);
    va_end(ap);

    /* Inspect known requests before passing through. */
    switch (request) {
        case DRM_IOCTL_AMDGPU_CS:
            if (arg) dump_cs((const union drm_amdgpu_cs *)arg);
            break;
        case DRM_IOCTL_AMDGPU_BO_LIST:
            if (arg) dump_bo_list((const union drm_amdgpu_bo_list *)arg);
            break;
        case DRM_IOCTL_AMDGPU_GEM_VA:
            if (arg) dump_gem_va((const struct drm_amdgpu_gem_va *)arg);
            break;
        default:
            break;
    }

    int rc = real_ioctl(fd, request, arg);
    if (request == DRM_IOCTL_AMDGPU_CS) {
        const union drm_amdgpu_cs *cs = (const union drm_amdgpu_cs *)arg;
        fprintf(out, "  -> rc=%d errno=%d (%s) cs_handle=0x%016llx\n",
                rc, rc < 0 ? errno : 0, rc < 0 ? strerror(errno) : "ok",
                (unsigned long long)cs->out.handle);
        fflush(out);
    }
    return rc;
}
