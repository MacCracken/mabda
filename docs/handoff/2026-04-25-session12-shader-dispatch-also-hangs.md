# v3 Phase B — Session 12 Finding: shader-dispatch via libdrm also hangs

**Date:** 2026-04-25 (late afternoon, after Session 11)
**Branch:** `v3`
**Working tree:** clean (Session 12 work committed)
**Status:** B.3.d **still blocked.**

## QUICK START — post-reboot Session 13 entry

Run from repo root:

```bash
# 1. Verify Cyrius toolchain (manifest pin: 5.6.13; tested under 5.6.43).
cyrius --version

# 2. Rebuild diagnostic binaries from committed sources.
make build/libdrm_spike build/libdrm_store_spike build/native_compute_spike

# 3. Baseline. Single command runs Mesa cl_probe (PASS expected) then
# both libdrm spikes (HANG expected) and prints recent kernel events.
make gpu-baseline

# 4. If the post-reboot results match what's documented below, proceed
# to the Step 2 / Step 3 experiments. If anything has changed (e.g.,
# Mesa cl_probe also fails, or our spikes start working), STOP and
# document — the situation is no longer the same.
```

**Cycle budget:** every spike run that hangs the GPU triggers a MODE2
reset and degrades the kernel's recovery state. Allow at most **3
direct-submission attempts per session** before rebooting again.
Mesa cl_probe is the canary: if it stops working, the GPU is
properly poisoned and we must reboot before further work.

## Headline summary

Built `deps/libdrm_store_spike.c` — canonical libdrm_amdgpu
submission with the **real GFX9 store
shader** (clang-compiled, byte-extracted from `build/shader/spike.o`)
and the full Mesa-rusticl byte-exact compute preamble (Session 7
pattern: PGM_HI/STATIC_THREAD_MGMT/CP_COHER/TA_CS_BC/PGM_LO/
PGM_RSRC1/RSRC2/TMPRING_SIZE/USER_DATA_2/USER_DATA_0/ACQUIRE_MEM/
RESOURCE_LIMITS/NUM_THREAD/DISPATCH_DIRECT, padded to 256 DW). It
**also hangs the GPU**. journalctl confirmed GPU resets 24 and 25
were caused by `Process libdrm_store_sp`. Mesa cl_probe continues
to work at 76 ms.

## TL;DR

| Probe | What it tries | Result |
|---|---|---|
| `deps/libdrm_spike.c` (Session 11) | bare WRITE_DATA + NOPs, libdrm | 10 s TDR, ECANCELED, MODE2 reset |
| `programs/native_compute_spike.cyr` (Session 10) | bare WRITE_DATA + NOPs, direct ioctl | 10 s TDR, syncobj signaled (cancellation invisible) |
| `deps/libdrm_store_spike.c` (Session 12) | full preamble + real shader, libdrm | fast hang (~0.7 s post-reset), ECANCELED, MODE2 reset |
| `build/shader/cl_probe` (Mesa OpenCL) | rusticl shader-dispatch | 76 ms, correct readback |

So **every direct submission we've made on this hardware hangs the
GPU**, regardless of submission API (direct ioctl vs libdrm), regardless
of IB content (NOP-only / WRITE_DATA / full real shader-dispatch),
regardless of ring (GFX vs COMPUTE). Only Mesa's full OpenCL stack
runs successfully on this hardware.

## What we have left to try

The "Mesa is doing something we're not" surface is now narrow but
important. Specific candidates, ordered by likelihood:

1. **`AMDGPU_GEM_CREATE_VM_ALWAYS_VALID` flag** on IB / shader BOs.
   We tried this once mid-session under post-reset state where every
   submission was being canceled fast — couldn't tell if it changed
   anything. **Try after a reboot.**
2. **Different VA range.** Our libdrm va_range_alloc gives consecutive
   VAs at `0x100000000+`. Mesa allocates VAs differently (separate
   ranges per pool, possibly higher). Could be a chip-MMU bug at the
   4 GiB boundary specifically.
3. **`ltrace -l libdrm_amdgpu.so.1` cl_probe** to capture Mesa's
   exact libdrm call sequence. ltrace is lighter than full strace,
   may not perturb Mesa's submission. If it does, build a tiny C
   program that uses the same libdrm calls Mesa makes (queries before
   first CS) — there may be a "warm-up" query Mesa does that primes
   GPU state.
4. **Fall back to wgpu-native for v3 GA** if (1)–(3) don't pan out.
   Document v3 native backend as "Cezanne-blocked, gfx10+ may work,
   revisit on RDNA hardware."

## Important context: GPU state degrades with each hang

Every spike run that triggers a TDR puts the kernel into a recovery
window where subsequent submissions are quickly canceled (`-ECANCELED`)
even before the CP sees them. Mesa cl_probe still works during this
window — confirming the GPU is alive, just our specific contexts
or BO mappings are being flagged as guilty.

**Reboot before further testing** to start from a clean baseline.

## What's committed in Session 12

- `deps/libdrm_store_spike.c` (~250 LOC) — full Mesa-byte-exact compute
  preamble + 9-DW GFX9 store shader (extracted from
  `build/shader/spike.o` .text section). Build:
  `cc -O2 -Wall -o build/libdrm_store_spike deps/libdrm_store_spike.c -ldrm_amdgpu`.
  Currently hangs the GPU on this kernel/firmware.
- `docs/handoff/2026-04-25-session12-shader-dispatch-also-hangs.md`
  (this file).

## Repo state at handoff

```
branch: v3
working tree: clean

In tree but not committed yet (committed by user this session):
  - deps/libdrm_spike.c (Session 11)
  - deps/libdrm_store_spike.c (Session 12)
  - programs/native_compute_spike.cyr (256-DW padding fix from Session 11)
  - src/backend_native.cyr (BO_HANDLES path from Session 10)

CPU tests: 621 passed, 0 failed (cyrius 5.6.43, manifest pin 5.6.13)
GPU integration: every direct submission hangs the GPU, regardless of
shape. Mesa cl_probe works.
```

## Memory updates from this session

- `feedback_write_data_on_cezanne.md` (from Session 11) needs a
  small append: shader-dispatch IBs from libdrm also hang on Cezanne.
  Mesa-via-OpenCL is the only known-good path on this hardware.
- `project_first_native_dispatch.md` refresh: Session 12 ruled out
  shader-dispatch shape as the fix. Path forward is post-reboot
  experiments (1)–(3) above OR fallback to wgpu-native for v3 GA.

## Next-session runbook (Session 13)

### Step 0 — reboot

Required. The system did ~25 GPU resets today; the kernel's
guilt-tracking and reset-recovery state are likely poisoning fresh
submissions.

### Step 1 — re-baseline

```bash
make gpu-baseline
```

That runs Mesa cl_probe (must PASS, ~80 ms), then both libdrm spikes
(both expected to FAIL with ECANCELED), then dumps recent AMDGPU
journal entries. If Mesa fails, the post-reboot state is wrong —
debug that first. If both spikes pass, the bug self-resolved across
reboots — celebrate, validate, update memories.

### Step 2 — try VM_ALWAYS_VALID

Edit `deps/libdrm_store_spike.c`, change the `amdgpu_bo_alloc_request`
flags from `0` to `AMDGPU_GEM_CREATE_VM_ALWAYS_VALID` (= `(1 << 6)`).

```bash
make build/libdrm_store_spike
./build/libdrm_store_spike
```

If readback shows `0xDEADBEEF`: **bug found.** Mesa relies on
VM_ALWAYS_VALID, our submissions don't get the same VM-side
treatment without it. Port the flag back to direct-ioctl Cyrius
(`native_bo_create_gtt` in `src/backend_native.cyr`) and rebuild
the Cyrius spike. **STOP after one attempt** — if it still hangs,
revert and proceed to Step 3.

### Step 3 — ltrace Mesa to capture the working init sequence

```bash
ltrace -l 'libdrm_amdgpu*' -o /tmp/mesa.ltrace ./build/shader/cl_probe
ltrace -l 'libdrm_amdgpu*' -o /tmp/spike.ltrace ./build/libdrm_store_spike
diff /tmp/mesa.ltrace /tmp/spike.ltrace
```

ltrace intercepts only libdrm symbol calls (lighter than full strace);
shouldn't perturb Mesa's submission timing. If it does perturb, build
a tiny C program that *just* replicates Mesa's pre-CS query sequence
(amdgpu_query_gpu_info, amdgpu_query_hw_ip_info × N, amdgpu_query_firmware_version,
etc.) without any submit, observe its calls, then pre-pend those queries
to our store_spike.

The diff should reveal one or more libdrm calls Mesa makes during
init that we don't — most likely candidates: `amdgpu_query_gpu_info`,
`amdgpu_query_firmware_version`, `amdgpu_cs_ctx_stable_pstate`, or
multiple `amdgpu_query_hw_ip_info` queries that prime per-IP state.

### Step 4 — if Steps 2 and 3 fail, fall back

Document Cezanne as known-blocked, ship v3 GA via wgpu-native, file an
upstream issue (linking this handoff and the Session 11 coredump).
The mabda v3 native backend can revisit when we have gfx10+ hardware
available.

Specifically:
- Update `docs/proposals/v3-native-api-principles.md` to mark Phase
  B.3.d as ⚠ blocked-on-Cezanne; document the wgpu fallback path.
- Update CHANGELOG with a "Known issue: native backend non-functional
  on AMD APUs gfx9–gfx9c (Renoir/Cezanne); use wgpu backend".
- File an issue at `docs/issues/2026-04-25-cezanne-direct-cs-blocked.md`
  with full repro steps + coredump excerpt.

Alternative: get RDNA hardware (gfx10+) — any RX 5500 or newer dGPU,
or a Phoenix/Strix/Krackan APU — and retest. If the bug is
Cezanne-firmware-specific, the native backend may "just work" on
newer hardware. That's a relatively cheap acquisition compared to
weeks of Cezanne ISA archeology.

## Supporting material

- Coredump from Session 11: `/tmp/amdgpu_coredump.txt` (6.1 MB,
  consumed-on-read; user ran `sudo cat ... > /tmp/...` mid-session).
- Mesa rusticl reference:
  `https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/src/gallium/winsys/amdgpu/drm/amdgpu_cs.cpp`
- libdrm reference:
  `https://gitlab.freedesktop.org/mesa/drm/-/raw/main/amdgpu/amdgpu_cs.c`
- Kernel cs.c parser:
  `https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/drivers/gpu/drm/amd/amdgpu/amdgpu_cs.c`
- Prior handoffs (chronological):
  - `docs/handoff/2026-04-23-session9-tdr-false-positive.md`
  - `docs/handoff/2026-04-25-session10-bo-handles-not-the-fix.md`
  - `docs/handoff/2026-04-25-session11-libdrm-also-hangs.md`
  - `docs/handoff/2026-04-25-session12-shader-dispatch-also-hangs.md` (this file)
