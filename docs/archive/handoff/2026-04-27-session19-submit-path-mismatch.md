---
title: Session 19 — submit-path mismatch: spike uses legacy BO_LIST + WAIT_CS, Mesa uses BO_HANDLES + SYNCOBJ
date: 2026-04-27
session: 19
branch: v3
hardware: AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c, Renoir family), Linux 6.18.24-lts
toolchain: cyrius 5.7.12
status: hypothesis identified, not yet tested
---

# Summary

Per Session 18's pivot, ran fresh side-by-side strace of `build/shader/cl_probe` (the working canary, Mesa rusticl + radeonsi winsys) and `build/libdrm_store_spike` (our Mesa-byte-exact PM4 spike that hangs with the post-dispatch CPC fault). The IB content is byte-exact identical. The **submit path is structurally different**.

# Logs

- `/tmp/cl_probe_s19.log` — non-verbose, 91 ioctls — full picture
- `/tmp/cl_probe_v_s19.log` — verbose, 76 ioctls (some dropped by strace -X)
- `/tmp/spike_s19.log` — non-verbose, 22 lines, 15 AMDGPU ioctls
- `/tmp/spike_v_s19.log` — verbose, 22 lines

cl_probe PASSED in 277 ms (fresh boot); spike timed out at 5062 ms with the same 0x66d000 fault signature; cl_probe re-tested and PASSED again afterward (no MODE2 reset needed this session).

# The structural delta

| ioctl | spike | cl_probe | meaning |
|---|---|---|---|
| `DRM_IOCTL_AMDGPU_BO_LIST` (0xc0186443) | **1** | 0 | spike uses persistent BO list handle |
| `DRM_IOCTL_AMDGPU_CS` (0xc0186444) | 1 | 2 | cl_probe submits two CSes (build + dispatch) |
| `DRM_IOCTL_AMDGPU_WAIT_CS` (0xc0206449) | **1** | 0 | spike uses legacy seq_no fence wait |
| `DRM_IOCTL_SYNCOBJ_CREATE` (0xc00864bf) | 0 | **3** | cl_probe creates per-submission syncobjs |
| `DRM_IOCTL_SYNCOBJ_WAIT` | 0 | **2** | cl_probe waits on syncobjs |
| `DRM_IOCTL_SYNCOBJ_DESTROY` | 0 | **1** | cl_probe cleans up syncobjs |

What this means at the libdrm layer:

- **Spike** uses the **legacy submit path**:
  `amdgpu_bo_list_create` (DRM_IOCTL_AMDGPU_BO_LIST) → `amdgpu_cs_submit` (CS with `bo_list_handle` populated, single IB chunk) → `amdgpu_cs_query_fence_status` (DRM_IOCTL_AMDGPU_WAIT_CS, seq_no-keyed kernel-internal fence).

- **cl_probe** uses the **modern syncobj path**:
  `amdgpu_cs_submit_raw2` with `bo_list_handle = 0` and chunks that include `AMDGPU_CHUNK_ID_BO_HANDLES` (per-submission BO list inline) and `AMDGPU_CHUNK_ID_SYNCOBJ_OUT` (signal a userspace-managed syncobj on completion). Wait via `amdgpu_cs_syncobj_wait` (DRM_IOCTL_SYNCOBJ_WAIT) on the syncobj handle.

# What is identical

CTX allocation is byte-exact:
- cl_probe `DRM_IOCTL_AMDGPU_CTX` first call: op=ALLOC_CTX(1), flags=0, ctx_id=0, priority=0
- spike same: op=ALLOC_CTX(1), flags=0, ctx_id=0, priority=0 → ctx_id=1

So priority/flags are *not* the delta. Both use NORMAL priority, no special flags.

GEM_CREATE shapes differ (cl_probe creates many BOs of varying sizes/domains/flags; spike creates 4 × 4 KB GTT BOs flags=0) but this is just the workload difference. The flags cl_probe sets (e.g., `0x6 = NO_CPU_ACCESS | CPU_GTT_USWC`) are only on internal Mesa BOs — none of cl_probe's BOs use `VM_ALWAYS_VALID` (0x20), `EXPLICIT_SYNC` (0x40), `PREEMPTIBLE` (0x200), or anything exotic.

VA mappings (`DRM_IOCTL_AMDGPU_GEM_VA`, 0xc0406448) appear in both traces (5 in cl_probe, 4 in spike) — the wrappers `amdgpu_bo_va_op` + `amdgpu_va_range_alloc` work identically.

# Hypothesis

**The legacy `amdgpu_cs_submit` path may set up a kernel-internal user fence VA that CPC's post-dispatch cleanup tries to write to, and this VA is not consistently mapped on Cezanne's gfx90c kernel 6.18.24.** With the modern `submit_raw2` + SYNCOBJ_OUT path, the kernel doesn't need a user-fence VA at all — completion is signaled via the syncobj fd.

This would explain:
- The fault is at a fixed VA (0x66d000) every time → kernel-allocated, not consumer-controlled.
- The fault fires *after* wave completion (S18 finding) → it's the fence-write step at end-of-pipe.
- Why the IB content is byte-exact-Mesa but the spike still hangs: the IB is fine, but the kernel-side queue setup that surrounds dispatch differs.

This is testable with one focused change: swap the spike from `amdgpu_cs_submit` + `amdgpu_bo_list_create` → `amdgpu_cs_submit_raw2` + chunks `[IB, BO_HANDLES, SYNCOBJ_OUT]` + `amdgpu_cs_syncobj_wait`.

# What Session 20 should do

1. Convert `deps/libdrm_store_spike.c` to the modern submit path. Keep the IB content byte-exact (do NOT touch any PM4 emit logic). Three changes only:
   - Replace `amdgpu_bo_list_create` → `amdgpu_bo_list_create_raw` (uint32_t handle out, populated from `struct drm_amdgpu_bo_list_entry[]`).
   - Replace `amdgpu_cs_submit(ctx, 0, &submit_req, 1)` → `amdgpu_cs_submit_raw2(dev, ctx, 0 /*no bo_list_handle*/, num_chunks, chunks, &seq_no)` where chunks = `[IB, BO_HANDLES, SYNCOBJ_OUT]`.
   - Replace `amdgpu_cs_query_fence_status` → `amdgpu_cs_syncobj_wait(dev, &syncobj_handle, 1, 5*1000*1000*1000ll, ...)`.
2. Fresh strace, confirm the new path matches cl_probe's ioctl shape.
3. Run the spike. Three outcomes:
   - **PASS (`out[0] = 0xDEADBEEF`)** — hypothesis confirmed. Port the modern path back to Cyrius `backend_native.cyr`. Phase B.4 unblocked.
   - **Same 0x66d000 fault** — hypothesis falsified, fault is deeper than submit path. Next candidates: VM_ALWAYS_VALID flag on shader BO, second/third-CS preamble that cl_probe runs.
   - **Different fault address** — even more informative; tells us we changed something but not the right thing.

# Required prerequisite for Session 20

The user must reboot before testing if today's spike runs eat into the budget further. Today's spike already cost one ring timeout; cl_probe survived, so the GPU is recoverable without MODE2 — but Session 18 burned 7 MODE2 resets and policy is 3/boot. Stay conservative.

# Codebase state at session end

- `deps/libdrm_store_spike.c` — unchanged from commit `38bfa40` ("working ideas").
- Logs in `/tmp/cl_probe_s19.log` etc are not version-controlled; they get cleared on reboot.
  Persist any reference traces by copying to `build/strace/` before reboot if needed.
- No code changes were committed this session.

# Reading list

- `/usr/include/libdrm/amdgpu.h` lines 1952-1981 — `amdgpu_cs_submit_raw` / `submit_raw2` signatures.
- `/usr/include/libdrm/amdgpu.h` lines 826-844 — `amdgpu_bo_list_create_raw` / `destroy_raw`.
- `/usr/include/libdrm/amdgpu.h` lines 1648-1750 — syncobj API.
- `/usr/include/libdrm/amdgpu_drm.h` lines 887-896 — chunk ID enum.
