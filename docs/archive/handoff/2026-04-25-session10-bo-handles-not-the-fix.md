# v3 Phase B — Session 10 Finding: BO_HANDLES Path Implemented But Not the Fix

**Date:** 2026-04-25 (post-reboot)
**Branch:** `v3`
**Working tree:** clean (Session 10 changes committed)
**Status:** B.3.d **still not actually passing**. Implemented Session 9's
runbook through Step 3 (BO_HANDLES chunk path). Submission is now
byte-equivalent to Mesa's pattern at the chunks-array level, kernel
accepts it, but CP still does not execute the IB. Bug is somewhere
else.

## TL;DR

Post-reboot ran the Session 9 runbook:

1. **Step 0 (Mesa health-check)** ✅ — `cl_probe` returns `0xDEADBEEF`
   in 74ms. GPU is healthy.
2. **Step 1 (timing baseline)** — first run was 0.756s (TDR-from-clean-state
   exception); next 3 runs all hit ~10.0s exactly. **Outcome B confirmed**:
   submission-path bug, not accumulated state.
3. **Step 2 (WRITE_DATA proof-of-execution)** — already in the stale
   binary; `stub[0] = 0x00000000` post-submit across all runs. CP never
   executes our IB.
4. **Step 3 (BO_HANDLES chunk)** ✅ implemented — `native_cs_submit_inline_bos`
   in `src/backend_native.cyr` matches Mesa's `amdgpu_cs_submit_ib_kernelq`
   byte-for-byte at the chunks-array level. Kernel accepts (no `ENOENT`).
   **`stub[0]` still 0**; still ~10s TDR. **BO_HANDLES was a real
   correctness fix in spirit, but it was not the blocker.**

We also disproved several other hypotheses with cheap experiments
(see "Eliminated suspects" below). The bug is somewhere we haven't
looked yet.

## Important correction to Session 9 handoff

`/sys/class/drm/card0/device/compute_reset_mask` returns
`soft queue full`. Session 9 read this as evidence of TDR resets
piling up. **It is not.** It is a static capability bitmask printed
by `amdgpu_show_reset_mask` (`drivers/gpu/drm/amd/amdgpu/amdgpu_device.c`):

```c
if (supported_reset & AMDGPU_RESET_TYPE_SOFT_RESET)  size += "soft ";
if (supported_reset & AMDGPU_RESET_TYPE_PER_QUEUE)   size += "queue ";
if (supported_reset & AMDGPU_RESET_TYPE_PER_PIPE)    size += "pipe ";
if (supported_reset & AMDGPU_RESET_TYPE_FULL)        size += "full ";
```

The string `"soft queue full"` is a constant on this hardware (Cezanne
gfx90c), printed every boot. It tells us the device supports those
three reset types — it does NOT indicate any reset has occurred. We
have no kernel-visible evidence of any TDR firing during our submissions.
The 10s wall time is the only signal that something went wrong.

## What's now committed and correct

- **`src/backend_native.cyr`**:
  - `AMDGPU_CHUNK_ID_BO_HANDLES = 0x06` constant.
  - `native_cs_submit_inline_bos(fd, ctx_id, bo_list_in_ptr, ib_va, ib_bytes, ip_type, syncobj_handle)`
    — implements the Mesa rusticl CS pattern. Chunk order: BO_HANDLES,
    SYNCOBJ_OUT, IB. `cs.in.bo_list_handle = 0`. The chunk_data for
    BO_HANDLES is `struct drm_amdgpu_bo_list_in` (24 bytes, op=`~0`,
    list_handle=`~0`) whose `bo_info_ptr` points to the
    `{handle u32, priority u32}` entries array.
- **`programs/native_compute_spike.cyr`**:
  - Builds `bo_list_in` inline; uses `native_cs_submit_inline_bos`
    instead of `native_bo_list_create` + `native_cs_submit`.
  - Emits PKT3 WRITE_DATA at PM4 offset 0 writing `0xCAFEBABE` to
    `stub_va`. Post-submit reads back `stub[0]` and exits 9 if not
    `0xCAFEBABE` (TDR false positive).
  - Prints submit-to-completion elapsed time in milliseconds.
  - Queries `AMDGPU_INFO_HW_IP_INFO` for both GFX and COMPUTE
    pre-submit; prints `available_rings` mask. Confirmed: GFX=0x1
    (ring 0 only), COMPUTE=0xf (rings 0-3).

621/621 CPU tests pass under cyrius 5.6.43 (manifest pin still 5.6.13).

## Eliminated suspects (post-reboot, this session)

| Hypothesis | Test | Result |
|---|---|---|
| BO_LIST ioctl vs BO_HANDLES chunk | Implemented inline-bos path | Same TDR |
| COMPUTE ring vs GFX ring | Switched ip_type | Same TDR on both |
| Full preamble vs minimal IB (WRITE_DATA + NOP only) | Stripped dispatch state | Same TDR |
| Canonical-high VAs vs low VAs | Set shader_va=0x100000, ib_va=0x300000 | Same TDR |
| Async VM page-table commit race | Added 100ms nanosleep between GEM_VA and CS | Same TDR |
| Ring 0 not user-submittable | Queried HW_IP_INFO | GFX:0x1 COMPUTE:0xf — ring 0 valid |
| Wrong DRM device | Verified `/dev/dri/renderD128` opened | Correct render node |
| `compute_reset_mask` runtime state | Read kernel source | Static cap bitmask, irrelevant |

The blocker is upstream of PM4, ring routing, and BO list management,
and it survives across reboots. Mesa's full OpenCL stack works fine
on the same hardware — so it's something about our specific submission
shape that the kernel either silently rejects post-acceptance or
schedules onto a queue that never picks it up.

## Remaining candidates (ranked, untested)

1. **Context creation flags / priority.** We pass `priority=0, flags=0`
   to `AMDGPU_CTX_OP_ALLOC_CTX`. Mesa goes through libdrm's
   `amdgpu_cs_ctx_create2` which passes the same defaults — should
   match. But Mesa also uses `amdgpu_query_gpu_info` and other queries
   that may set up driver-internal state we're missing. Worth: read
   `amdgpu_ctx_alloc` in the kernel, check for any state setup we're
   bypassing.

2. **IB chunk `flags` field.** We set `ib_chunk.flags = 0`. Mesa
   sometimes sets `AMDGPU_IB_FLAG_EMIT_MEM_SYNC` (1<<6). Cheap
   experiment: try with that flag.

3. **`scheduled_dependencies` chunk required for first submission?**
   Mesa includes a SCHEDULED_DEPENDENCIES chunk on some submissions.
   Worth checking the kernel parser for any "first submission must
   have X chunk" semantics.

4. **Build a minimal libdrm_amdgpu reference.** Write a 100-line C
   program that uses libdrm's `amdgpu_cs_submit_raw2` to submit the
   same minimal WRITE_DATA IB. If that works, the bug is **purely**
   in our direct ioctls. If it ALSO fails, there's a deeper issue
   (kernel state, missing setup ioctls, etc.). This is Step 4 from
   Session 9's runbook and is now the right next move.

5. **CS `flags` field on the request.** We set `cs.in.flags = 0`. The
   kernel checks for `AMDGPU_CS_USE_VM` and other flags but they
   default to OK. Worth a quick read of `amdgpu_cs_ioctl`.

## Next-session runbook

### Step A — try IB flag combinations (cheap, in-place)

Edit `native_cs_submit_inline_bos` to take an `ib_flags` parameter or
just hardcode `0x40` (`AMDGPU_IB_FLAG_EMIT_MEM_SYNC`) and rerun. One
data point each.

### Step B — build a libdrm_amdgpu reference (medium effort)

```c
/* deps/libdrm_spike.c — link with -ldrm -ldrm_amdgpu */
amdgpu_device_handle dev;
amdgpu_device_initialize(fd, &maj, &min, &dev);

amdgpu_context_handle ctx;
amdgpu_cs_ctx_create(dev, &ctx);

amdgpu_bo_handle ib_bo;
/* alloc + map + CPU-write the same WRITE_DATA bytes we use */

uint32_t bo_list[] = { ib_bo_handle, stub_bo_handle, 0 };
amdgpu_bo_list_handle list;
amdgpu_bo_list_create(dev, 2, bos, NULL, &list);

struct drm_amdgpu_cs_chunk_ib ib_in = { .va_start = ib_va,
                                         .ib_bytes = 20 + padding,
                                         .ip_type = AMDGPU_HW_IP_GFX,
                                         .ring = 0 };
struct drm_amdgpu_cs_chunk chunks[1] = {{
    .chunk_id = AMDGPU_CHUNK_ID_IB,
    .length_dw = sizeof(ib_in)/4,
    .chunk_data = (uintptr_t)&ib_in,
}};

uint64_t seq_no;
amdgpu_cs_submit_raw2(dev, ctx, list_handle, 1, chunks, &seq_no);

/* wait via amdgpu_cs_query_fence_status */
/* readback stub[0] — expect 0xCAFEBABE */
```

If this works: bug is in our direct ioctls. Strace-diff the C version
vs spike to find the difference.

If this also fails: there's something fundamentally different about
this hardware/kernel/process that prevents non-Mesa direct submissions.
Consider running it from a clean shell or with `AMD_DEBUG=ib` to see
if Mesa's debug machinery reveals anything.

### Step C — fall back to libdrm permanently for CS submit (escape hatch)

If the libdrm reference works and we can't reverse-engineer the
delta in reasonable time, link `libdrm_amdgpu` in as a transient C
dependency for CS submission only. Keep BO creation / PM4 building
in pure Cyrius. This loses some sovereignty but unblocks B.4 and
the rest of v3. Document as a known-temporary measure for v3.0
GA. Revisit after the v4.0 native-driver milestone.

## Repo state at handoff

```
branch: v3
working tree: clean

uncommitted changes (Session 10): all reverted from diagnostic
experiments. Final state:
  - src/backend_native.cyr   +AMDGPU_CHUNK_ID_BO_HANDLES, +native_cs_submit_inline_bos
  - programs/native_compute_spike.cyr — refactored to BO_HANDLES + WRITE_DATA + readback + HW_IP_INFO
  - docs/handoff/2026-04-25-session10-bo-handles-not-the-fix.md (this file)

CPU tests: 621 passed, 0 failed (cyrius 5.6.43, manifest pin 5.6.13)
GPU integration (direct-DRM): BLOCKED at exactly the same place as Session 9:
  submissions accepted, syncobj signals after 10s, CP doesn't execute IB.
GPU integration (Mesa cl_probe, control): 74ms, correct readback.
```

## Memory updates from this session

- `feedback_compute_reset_mask_static.md` — new feedback memory:
  `compute_reset_mask` is a static capability bitmask, not runtime
  state. Don't read it as evidence of TDRs.
- `project_first_native_dispatch.md` — refresh: BO_HANDLES did not
  fix it; suspect surface narrowed; libdrm reference is the next step.

## Supporting material

- Mesa CS source: `https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/src/gallium/winsys/amdgpu/drm/amdgpu_cs.cpp`
  function `amdgpu_cs_submit_ib_kernelq` lines 1293–1430.
- libdrm CS source: `https://gitlab.freedesktop.org/mesa/drm/-/raw/main/amdgpu/amdgpu_cs.c`
  function `amdgpu_cs_submit_raw2` lines 927–951.
- Kernel CS parser: `https://git.kernel.org/.../drivers/gpu/drm/amd/amdgpu/amdgpu_cs.c`
  `amdgpu_cs_p1_bo_handles` lines 144–166, `amdgpu_cs_pass1` lines 168+.
- Kernel BO list reconstruction: `drivers/gpu/drm/amd/amdgpu/amdgpu_bo_list.c`
  `amdgpu_bo_create_list_entry_array` lines 183+.
- Prior handoff: `docs/handoff/2026-04-23-session9-tdr-false-positive.md`.
