/* ibsnoop.c — LD_PRELOAD read-only snoop of amdgpu CS submissions.
 *
 * Why: `RADV_DEBUG=dumpibs` (the `make dump` target) prints radv's preamble IB
 * but NOT the IB it jumps to — radv's gfx_init state (CONTEXT_CONTROL +
 * CLEAR_STATE + radv_emit_graphics register writes, e.g. PA_CL_VTE_CNTL).
 * The 4.1.3 render-state audit needed that nested IB: `make snoop` captures
 * every IB radv submits, including nested INDIRECT_BUFFER targets.
 *
 * Records GEM_VA mappings (fd, handle, va, offset, size); on every
 * DRM_IOCTL_AMDGPU_CS it dumps each IB chunk's bytes (by re-mmapping the
 * backing BO through GEM_MMAP on the same fd) and recursively follows
 * PM4 INDIRECT_BUFFER packets so nested IBs (radv's gfx_init state IB,
 * referenced from its preamble IB) are captured too. Nothing is modified:
 * the real ioctl is invoked with the caller's arguments unchanged.
 *
 * Output: $IBSNOOP_DIR/cs<seq>_<k>_ip<ip>_fl<flags>_<va>.bin (+ index.txt)
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/mman.h>
#include <pthread.h>

struct vamap { int fd; uint32_t handle; uint64_t va, off, size; int live; };
static struct vamap maps[8192];
static int nmaps;
static int cs_seq;
static pthread_mutex_t mu = PTHREAD_MUTEX_INITIALIZER;
static int (*real_ioctl)(int, unsigned long, ...);

static const char *outdir(void) {
    const char *d = getenv("IBSNOOP_DIR");
    return d ? d : ".";
}

static void logline(const char *fmt, ...) {
    char p[512];
    snprintf(p, sizeof p, "%s/index.txt", outdir());
    FILE *f = fopen(p, "a");
    if (!f) return;
    va_list ap; va_start(ap, fmt); vfprintf(f, fmt, ap); va_end(ap);
    fclose(f);
}

static struct vamap *find_map(int fd, uint64_t va) {
    for (int i = nmaps - 1; i >= 0; i--) {
        struct vamap *m = &maps[i];
        if (!m->live || m->fd != fd) continue;
        if (va >= m->va && va < m->va + m->size) return m;
    }
    return NULL;
}

/* Read `nbytes` at GPU VA `va` into `out` via GEM_MMAP of the backing BO. */
static int read_va(int fd, uint64_t va, uint32_t nbytes, uint8_t *out) {
    struct vamap *m = find_map(fd, va);
    if (!m) return -1;
    uint64_t rel = va - m->va + m->off;
    if (va + nbytes > m->va + m->size) return -2;
    union { struct { uint32_t handle, pad; } in; struct { uint64_t addr_ptr; } out; } mm;
    memset(&mm, 0, sizeof mm);
    mm.in.handle = m->handle;
    unsigned long req = ((unsigned long)3 << 30) | (8UL << 16) | ('d' << 8) | 0x41;
    if (real_ioctl(fd, req, &mm) != 0) return -3;
    uint64_t maplen = m->off + m->size;
    uint8_t *p = mmap(NULL, maplen, PROT_READ, MAP_SHARED, fd, (off_t)mm.out.addr_ptr);
    if (p == MAP_FAILED) return -4;
    memcpy(out, p + rel, nbytes);
    munmap(p, maplen);
    return 0;
}

static void dump_ib(int fd, int seq, const char *tag, uint32_t ip, uint32_t flags,
                    uint64_t va, uint32_t nbytes, int depth) {
    if (depth > 4 || nbytes == 0 || nbytes > (1u << 24)) return;
    uint8_t *buf = calloc(1, nbytes);
    if (!buf) return;
    int rc = read_va(fd, va, nbytes, buf);
    char path[600];
    snprintf(path, sizeof path, "%s/cs%03d_%s_ip%u_fl%x_%016llx.bin", outdir(), seq, tag, ip,
             flags, (unsigned long long)va);
    logline("cs=%d tag=%s ip=%u flags=0x%x va=0x%016llx bytes=%u depth=%d read_rc=%d file=%s\n",
            seq, tag, ip, flags, (unsigned long long)va, nbytes, depth, rc, path);
    if (rc == 0) {
        FILE *f = fopen(path, "wb");
        if (f) { fwrite(buf, 1, nbytes, f); fclose(f); }
        /* follow INDIRECT_BUFFER (opcode 0x3F) packets */
        uint32_t ndw = nbytes / 4, i = 0, sub = 0;
        while (i < ndw) {
            uint32_t h; memcpy(&h, buf + i * 4, 4);
            if ((h >> 30) != 3) { i++; continue; }
            uint32_t cnt = ((h >> 16) & 0x3FFF) + 1;
            uint32_t op = (h >> 8) & 0xFF;
            if (op == 0x3F && cnt >= 3 && i + 3 < ndw) {
                uint32_t lo, hi, ctl;
                memcpy(&lo, buf + (i + 1) * 4, 4);
                memcpy(&hi, buf + (i + 2) * 4, 4);
                memcpy(&ctl, buf + (i + 3) * 4, 4);
                uint64_t sva = ((uint64_t)hi << 32) | lo;
                char st[64];
                snprintf(st, sizeof st, "%s-ib%u", tag, sub++);
                dump_ib(fd, seq, st, ip, flags, sva, (ctl & 0xFFFFF) * 4, depth + 1);
            }
            i += 1 + cnt;
        }
    }
    free(buf);
}

int ioctl(int fd, unsigned long req, ...) {
    va_list ap; va_start(ap, req);
    void *arg = va_arg(ap, void *);
    va_end(ap);
    if (!real_ioctl) real_ioctl = dlsym(RTLD_NEXT, "ioctl");
    uint32_t type = (req >> 8) & 0xFF, nr = req & 0xFF;
    if (type == 'd' && nr == 0x48 && arg) {           /* GEM_VA */
        int r = real_ioctl(fd, req, arg);
        struct { uint32_t handle, pad, op, flags; uint64_t va, off, size; } *v = arg;
        pthread_mutex_lock(&mu);
        if (r == 0 && (v->op == 1 || v->op == 4) && nmaps < 8192) {
            maps[nmaps++] = (struct vamap){fd, v->handle, v->va, v->off, v->size, 1};
        } else if (r == 0 && (v->op == 2 || v->op == 3)) {
            for (int i = 0; i < nmaps; i++)
                if (maps[i].fd == fd && maps[i].va == v->va) maps[i].live = 0;
        }
        pthread_mutex_unlock(&mu);
        return r;
    }
    if (type == 'd' && nr == 0x44 && arg) {           /* CS */
        struct { uint32_t ctx, bo_list, nchunks, flags; uint64_t chunks; } *cs = arg;
        pthread_mutex_lock(&mu);
        int seq = cs_seq++;
        uint64_t *cp = (uint64_t *)(uintptr_t)cs->chunks;
        for (uint32_t k = 0; cp && k < cs->nchunks; k++) {
            struct { uint32_t id, len; uint64_t data; } *ch = (void *)(uintptr_t)cp[k];
            if (!ch) continue;
            logline("cs=%d ctx=%u chunk=%u id=%u len_dw=%u\n", seq, cs->ctx, k, ch->id, ch->len);
            if (ch->id != 1) continue;
            struct { uint32_t pad, flags; uint64_t va; uint32_t bytes, ip, inst, ring; } *ib =
                (void *)(uintptr_t)ch->data;
            char tag[32];
            snprintf(tag, sizeof tag, "c%u", k);
            dump_ib(fd, seq, tag, ib->ip, ib->flags, ib->va, ib->bytes, 0);
        }
        pthread_mutex_unlock(&mu);
        return real_ioctl(fd, req, arg);
    }
    return real_ioctl(fd, req, arg);
}
