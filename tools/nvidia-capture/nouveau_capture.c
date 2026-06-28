// nouveau_capture.c — LD_PRELOAD ioctl/mmap interposer for capturing a
// known-good NVK compute dispatch (mabda N0.7(c)). Dep-free (libc + dlfcn).
//
// Decodes every nouveau DRM ioctl by nr, tracks GEM_NEW handles, their
// mmap'd CPU addresses, and VM_BIND GPU-VA ranges, then on EXEC dumps:
//   - the decoded exec struct + push list (text log)
//   - each pushbuffer's bytes and every VM-bound BO's bytes (raw .bin)
// The 256-byte QMD is one of the bound BOs (the one SEND_PCAS points at).
// CHANNEL_ALLOC + NVIF (0xC5C0 class-object create) structs are hex-logged.
//
// Build:  gcc -shared -fPIC -O2 nouveau_capture.c -o nouveau_capture.so -ldl
// Use:    NV_CAP_DIR=./cap LD_PRELOAD=./nouveau_capture.so <nvk_compute_app>
//
// Env: NV_CAP_DIR = output dir (default "./nvcap").
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <stdarg.h>
#include <string.h>
#include <dlfcn.h>
#include <unistd.h>
#include <sys/mman.h>

// ---- _IOC decode (asm-generic) ----
#define IOC_NR(r)   ((r) & 0xff)
#define IOC_TYPE(r) (((r) >> 8) & 0xff)
#define IOC_SIZE(r) (((r) >> 16) & 0x3fff)
#define IOC_DIR(r)  (((r) >> 30) & 0x3)
#define DRM_TYPE 0x64

// nouveau abs nr = DRM_COMMAND_BASE(0x40) + op
enum { NR_GETPARAM=0x40, NR_CHANNEL_ALLOC=0x42, NR_CHANNEL_FREE=0x43,
       NR_GROBJ_ALLOC=0x44, NR_NVIF=0x47, NR_VM_INIT=0x50, NR_VM_BIND=0x51,
       NR_EXEC=0x52, NR_GEM_NEW=0x80, NR_GEM_PUSHBUF=0x81, NR_GEM_INFO=0x84 };

static const char *nr_name(unsigned nr){
    switch(nr){
    case NR_GETPARAM: return "GETPARAM";
    case NR_CHANNEL_ALLOC: return "CHANNEL_ALLOC";
    case NR_CHANNEL_FREE: return "CHANNEL_FREE";
    case NR_GROBJ_ALLOC: return "GROBJ_ALLOC";
    case NR_NVIF: return "NVIF";
    case NR_VM_INIT: return "VM_INIT";
    case NR_VM_BIND: return "VM_BIND";
    case NR_EXEC: return "EXEC";
    case NR_GEM_NEW: return "GEM_NEW";
    case NR_GEM_PUSHBUF: return "GEM_PUSHBUF";
    case NR_GEM_INFO: return "GEM_INFO";
    default: return "drm?";
    }
}

#define MAXBO 1024
struct bo { uint32_t handle; uint32_t dom; uint64_t size; uint64_t map_handle; void *cpu; };
static struct bo g_bo[MAXBO]; static int g_nbo;
struct vm { uint64_t va, range; uint32_t handle; };
static struct vm g_vm[MAXBO]; static int g_nvm;

static FILE *g_log;
static char  g_dir[512];
static int   g_seq;
static int   g_fd = -1;   // the render-node fd (for the exit-time dump)

static int  (*real_ioctl)(int, unsigned long, ...);
static void*(*real_mmap)(void*, size_t, int, int, int, off_t);
static int  (*real_munmap)(void*, size_t);

static void init_once(void){
    if (g_log) return;
    real_ioctl  = dlsym(RTLD_NEXT, "ioctl");
    real_mmap   = dlsym(RTLD_NEXT, "mmap");
    real_munmap = dlsym(RTLD_NEXT, "munmap");
    const char *d = getenv("NV_CAP_DIR"); if(!d) d="./nvcap";
    snprintf(g_dir, sizeof g_dir, "%s", d);
    char p[600]; snprintf(p, sizeof p, "mkdir -p '%s'", g_dir); system(p);
    char lf[600]; snprintf(lf, sizeof lf, "%s/capture.log", g_dir);
    g_log = fopen(lf, "w"); setvbuf(g_log, NULL, _IONBF, 0);
    fprintf(g_log, "# nouveau capture (mabda N0.7c)\n");
}

static struct bo* bo_by_handle(uint32_t h){
    for(int i=0;i<g_nbo;i++) if(g_bo[i].handle==h) return &g_bo[i];
    return NULL;
}
static struct bo* bo_by_maph(uint64_t mh){
    for(int i=0;i<g_nbo;i++) if(g_bo[i].map_handle && g_bo[i].map_handle==mh) return &g_bo[i];
    return NULL;
}
// resolve a GPU VA to a CPU pointer via VM_BIND range + the BO's mmap.
static void* va_to_cpu(uint64_t va, uint64_t *bo_base){
    for(int i=0;i<g_nvm;i++){
        if(va>=g_vm[i].va && va < g_vm[i].va+g_vm[i].range){
            struct bo *b = bo_by_handle(g_vm[i].handle);
            if(b && b->cpu){ if(bo_base)*bo_base=g_vm[i].va; return (char*)b->cpu + (va - g_vm[i].va); }
        }
    }
    return NULL;
}

static void hexlog(const char *tag, const unsigned char *p, int n){
    fprintf(g_log, "  %s [%d bytes]:", tag, n);
    for(int i=0;i<n;i++){ if(i%16==0)fprintf(g_log,"\n    "); fprintf(g_log,"%02x ", p[i]); }
    fprintf(g_log, "\n");
}
static void dump_bin(const char *name, const void *p, uint64_t n){
    char f[700]; snprintf(f, sizeof f, "%s/%s", g_dir, name);
    FILE *o=fopen(f,"wb"); if(o){ fwrite(p,1,n,o); fclose(o); fprintf(g_log,"  -> wrote %s (%lu B)\n", name, (unsigned long)n); }
}
// Self-mmap a BO via its map_handle (works for VRAM-over-BAR1 too, since
// NVK doesn't keep these CPU-mapped) and dump [off, off+n).
static void dump_bo_region(int fd, struct bo *b, uint64_t off, uint64_t n, const char *name){
    if(!b || !b->map_handle){ return; }
    void *m = real_mmap(0, b->size, PROT_READ, MAP_SHARED, fd, (off_t)b->map_handle);
    if(m==MAP_FAILED){ fprintf(g_log,"  (self-mmap fail h=%u off=0x%lx)\n", b->handle,(unsigned long)b->map_handle); return; }
    if(off > b->size) off=b->size;
    if(off+n > b->size) n = b->size-off;
    dump_bin(name, (char*)m+off, n);
    real_munmap(m, b->size);
}
static struct bo* bo_for_va(uint64_t va, uint64_t *base){
    for(int j=0;j<g_nvm;j++) if(va>=g_vm[j].va && va<g_vm[j].va+g_vm[j].range){
        if(base)*base=g_vm[j].va; return bo_by_handle(g_vm[j].handle); }
    return NULL;
}

int ioctl(int fd, unsigned long req, ...){
    init_once();
    va_list ap; va_start(ap, req); void *arg = va_arg(ap, void*); va_end(ap);
    int rc = real_ioctl(fd, req, arg);

    if (IOC_TYPE(req) != DRM_TYPE) return rc;
    unsigned nr = IOC_NR(req);
    // only log nouveau-range nrs we care about (+ NVIF/GROBJ)
    int interesting = (nr>=0x40 && nr<=0x53) || (nr>=0x80 && nr<=0x84);
    if (!interesting) return rc;

    unsigned char *a = arg;
    fprintf(g_log, "[%d] ioctl %s (nr 0x%02x, size %u, rc %d)\n",
            g_seq, nr_name(nr), nr, IOC_SIZE(req), rc);

    if (g_fd<0) g_fd=fd;
    if (rc==0 && nr==NR_GEM_NEW && g_nbo<MAXBO){
        struct bo *b=&g_bo[g_nbo++];
        b->handle=*(uint32_t*)(a+0); b->dom=*(uint32_t*)(a+4); b->size=*(uint64_t*)(a+8); b->map_handle=*(uint64_t*)(a+24); b->cpu=NULL;
        fprintf(g_log,"  GEM_NEW handle=%u size=%lu map_handle=0x%lx domain=0x%x tile_mode=0x%x tile_flags=0x%x align=0x%x\n",
                b->handle,(unsigned long)b->size,(unsigned long)b->map_handle,
                *(uint32_t*)(a+4),*(uint32_t*)(a+32),*(uint32_t*)(a+36),*(uint32_t*)(a+44));
    } else if (rc==0 && nr==NR_GEM_INFO){
        struct bo *b=bo_by_handle(*(uint32_t*)(a+0));
        uint64_t mh=*(uint64_t*)(a+24); if(b&&!b->map_handle)b->map_handle=mh;
        fprintf(g_log,"  GEM_INFO handle=%u map_handle=0x%lx\n",*(uint32_t*)(a+0),(unsigned long)mh);
    } else if (nr==NR_CHANNEL_ALLOC){
        fprintf(g_log,"  CHANNEL_ALLOC fb=0x%x tt=0x%x chid=%d pushdom=0x%x notifier=0x%x nr_subchan=%u\n",
                *(uint32_t*)(a+0),*(uint32_t*)(a+4),*(int32_t*)(a+8),*(uint32_t*)(a+12),*(uint32_t*)(a+16),*(uint32_t*)(a+84));
        hexlog("channel_alloc", a, 88);
    } else if (nr==NR_NVIF){
        // variable-size NVIF payload — dump the header region (class-object create)
        hexlog("nvif", a, IOC_SIZE(req)? (int)IOC_SIZE(req):64);
    } else if (nr==NR_GROBJ_ALLOC){
        hexlog("grobj_alloc", a, 12);
    } else if (rc==0 && nr==NR_VM_BIND){
        uint32_t opc=*(uint32_t*)(a+0); uint64_t opp=*(uint64_t*)(a+32);
        unsigned char *op=(unsigned char*)opp;
        uint32_t bflags=*(uint32_t*)(a+4);
        for(uint32_t i=0;i<opc && op;i++,op+=40){
            uint32_t o=*(uint32_t*)(op+0), of=*(uint32_t*)(op+4), h=*(uint32_t*)(op+8);
            uint64_t va=*(uint64_t*)(op+16), rg=*(uint64_t*)(op+32);
            fprintf(g_log,"  VM_BIND op=%u op_flags=0x%x bind_flags=0x%x handle=%u va=0x%lx range=0x%lx\n",
                    o,of,bflags,h,(unsigned long)va,(unsigned long)rg);
            if(o==0 && g_nvm<MAXBO){ g_vm[g_nvm].va=va; g_vm[g_nvm].range=rg; g_vm[g_nvm].handle=h; g_nvm++; }
        }
    } else if (nr==NR_EXEC){
        uint32_t chid=*(uint32_t*)(a+0), pc=*(uint32_t*)(a+4);
        uint64_t pp=*(uint64_t*)(a+32);
        fprintf(g_log,"  EXEC chid=%u push_count=%u rc=%d\n",chid,pc,rc);
        unsigned char *pe=(unsigned char*)pp;
        for(uint32_t i=0;i<pc && pe;i++,pe+=16){
            uint64_t va=*(uint64_t*)(pe+0); uint32_t len=*(uint32_t*)(pe+8);
            uint64_t base=0; struct bo *b=bo_for_va(va,&base);
            fprintf(g_log,"  push[%u] va=0x%lx len=%u (h=%u off=0x%lx)\n",
                    i,(unsigned long)va,len,b?b->handle:0,b?(unsigned long)(va-base):0);
            if(b){ char nm[176]; snprintf(nm,sizeof nm,"%03d_exec_chid%u_push%u_h%u_off%lx_len%u.bin",
                g_seq,chid,i,b->handle,(unsigned long)(va-base),len);
                dump_bo_region(fd,b,va-base,len,nm); }
        }
        // self-mmap + dump every VM-bound BO (QMD/shader/const/output among these)
        for(int i=0;i<g_nvm;i++){
            struct bo *b=bo_by_handle(g_vm[i].handle); if(!b||!b->map_handle) continue;
            uint64_t dn = b->size<65536? b->size:65536;
            char nm[200]; snprintf(nm,sizeof nm,"%03d_chid%u_bo_va%lx_h%u_dom%x_sz%lu.bin",
                g_seq,chid,(unsigned long)g_vm[i].va,b->handle,b->dom,(unsigned long)b->size);
            dump_bo_region(fd,b,0,dn,nm);
        }
    }
    g_seq++;
    return rc;
}

void *mmap(void *addr, size_t len, int prot, int flags, int fd, off_t off){
    init_once();
    void *r = real_mmap(addr,len,prot,flags,fd,off);
    if (r!=MAP_FAILED && off){
        struct bo *b=bo_by_maph((uint64_t)off);
        if(b){ b->cpu=r; fprintf(g_log,"  mmap handle=%u off=0x%lx -> %p (len %zu)\n",b->handle,(unsigned long)off,r,len); }
    }
    return r;
}

// At process exit (after all dispatches have run), self-mmap every bound BO
// and dump it — captures the POST-dispatch state, so the BO holding the
// shader's 0xDEADBEEF result is visible and identifiable by its handle.
__attribute__((destructor))
static void final_dump(void){
    if(!g_log || g_fd<0) return;
    fprintf(g_log,"\n=== FINAL (post-dispatch) BO dump ===\n");
    for(int i=0;i<g_nvm;i++){
        struct bo *b=bo_by_handle(g_vm[i].handle);
        if(!b||!b->map_handle) continue;
        uint64_t dn = b->size<65536? b->size:65536;
        char nm[200]; snprintf(nm,sizeof nm,"final_va%lx_h%u_dom%x.bin",
            (unsigned long)g_vm[i].va,b->handle,b->dom);
        dump_bo_region(g_fd,b,0,dn,nm);
    }
}
