# v3 Phase B — Session 11 Finding: libdrm_amdgpu reference also hangs CP

**Date:** 2026-04-25 (afternoon, after Session 10)
**Branch:** `v3`
**Working tree:** modified (libdrm spike + spike padding fix uncommitted)
**Status:** B.3.d **still blocked**, but the suspect surface has been
*massively* narrowed. Built the libdrm_amdgpu reference (Session 9
runbook Step 4); it hangs the GFX9 CP firmware in exactly the same
way our direct-ioctl spike does. **Bug is NOT in our direct-ioctl
path.** Captured a kernel coredump that shows ME stalled on our
WRITE_DATA packet after PFP fully consumed the IB.

## TL;DR

- Built `deps/libdrm_spike.c` — canonical libdrm_amdgpu API, Mesa-style
  submission, BO_LIST handle, `amdgpu_cs_submit_raw2`. Submits a
  WRITE_DATA + NOP IB, reads back.
- Result: identical hang. `amdgpu_cs_query_fence_status` returns
  **`-ECANCELED` (-125)** at ~10s. Kernel does **MODE2 GPU reset** to
  recover.
- `journalctl` confirms our process is the cause:
  `ring comp_1.1.0 timeout, signaled seq=45, emitted seq=46`,
  `Process libdrm_spike pid 53941`, `fail to wait on hqd deactive`,
  `Ring comp_1.1.0 reset failed`, `MODE2 reset`,
  `GPU reset succeeded`. Mesa's seq 45 ran fine, our seq 46 wedged
  the firmware so badly only a full GPU reset recovered it.
- Captured `/sys/class/drm/card0/device/devcoredump/data` (root) into
  `/tmp/amdgpu_coredump.txt`. Analysis below.
- Even a **NOP-only IB** (no WRITE_DATA at all) hangs identically.
- **Mesa cl_probe still works** because Mesa's OpenCL path uses real
  shader-dispatch (the shader stores via memory subsystem); it does
  NOT exercise CP-side WRITE_DATA. Our diagnostic was probing a path
  Mesa never validates.

## Coredump highlights (parsed from /tmp/amdgpu_coredump.txt)

`Ring timed out details: IP Type: 0 Ring Name: gfx`. Process: `libdrm_spike`.

CP register state at hang:

| Register | Value | Meaning |
|---|---|---|
| `mmCP_IB1_BASE_LO` | `0x00001000` | Our IB VA low — matches |
| `mmCP_IB1_BASE_HI` | `0x00000001` | Our IB VA high — matches |
| `mmCP_IB1_CMD_BUFSZ` | `0x00000100` | 256 DWs commanded — matches our 1 KiB IB |
| `mmCP_IB1_BUFSZ` | `0x00000000` | **CP fully consumed our IB** |
| `mmCP_STALLED_STAT1` | `0x00000c00` | ME stalled on something |
| `mmCP_BUSY_STAT` | `0x00400080` | Some sub-unit busy/waiting |
| `mmVM_L2_PROTECTION_FAULT_STATUS` | `0x00000000` | **No VM page fault recorded** |

The standalone `[gfxhub] Page fault observed / Faulty page address:
0x0000000000000000` line at the top of the dump is the kernel's
default placeholder when no protection-fault status is present —
ignore it.

PFP / ME packet header histories (8-deep ring buffer of recent headers):

```
PFP last 8 (newest first):
  0xcafebabe   ← our WRITE_DATA's data word, being read AS A HEADER
  0xc0fa1000   ← our IT_NOP, count=250 (fills rest of IB)
  0xc0033700   ← our WRITE_DATA, count=3
  0xc0023f00   ← kernel's INDIRECT_BUFFER → our IB
  ...

ME last 8:
  0xc0033700   ← our WRITE_DATA — ME's most recent
  0xc0023f00   ← INDIRECT_BUFFER
  ...
```

PFP got past our IB (consuming all 256 DWs), then somehow read
`0xCAFEBABE` (the data word from inside our IB) as a packet header.
ME's most-recent header is our WRITE_DATA — meaning ME is either
stuck mid-execution of WRITE_DATA, or the dump captured ME at the
last completed packet.

The kernel's INDIRECT_BUFFER on the ring at byte offset 0x1570:
```
0x1570  0xc0023f00   PKT3 INDIRECT_BUFFER, count=2
0x1574  0x00001000   IB addr_lo
0x1578  0x00000001   IB addr_hi
0x157c  0x07000100   control: vmid=7, length=256
```

Note `INDIRECT_BUFFER_VALID` (bit 23) is NOT set. **This is correct
behavior on GFX rings** — `gfx_v9_0_ring_emit_ib_gfx` initializes
`control = 0` and OR's only `length_dw | (vmid << 24)`. Compute rings
DO set VALID via `gfx_v9_0_ring_emit_ib_compute`, but on the GFX ring
VALID=0 is normal. (Earlier theory disproved.)

## Negative results from this session

| Hypothesis | Test | Result |
|---|---|---|
| Bug is in our direct ioctls | Built libdrm reference | Same hang. |
| WR_CONFIRM bit causing ME hang | Cleared bit 20 in control word | Same hang. |
| WR_ONE_ADDR causing issue | Cleared bit 16 | Same hang. |
| GFX vs COMPUTE ring | Tested both on libdrm | Both hang. |
| `cs_ctx_create` vs `cs_ctx_create2(prio=0)` | Tested both | Both hang. |
| 256-DW IB padding requirement | Confirmed kernel auto-pads via `amdgpu_ring_generic_pad_ib`; updated spike to pre-pad anyway | Cosmetic, didn't fix bug. |
| CONTEXT_CONTROL preamble missing | Read `amdgpu_ib_schedule` — kernel emits it | N/A, kernel handles it. |
| `INDIRECT_BUFFER_VALID` bit | Decoded control word; bit 23=0 is normal on GFX ring | Not a bug. |
| Writing WRITE_DATA to a NOP-only IB | Stripped IB to a single PKT3 NOP filling 256 DWs | Same hang. |

So: the bug isn't in chunk shape, isn't in IB content (NOP-only fails
too), isn't in our context, isn't in libdrm vs raw ioctls. It survives
across reboots. Mesa exclusively works because it dispatches a real
shader, never tests pure CP-side WRITE_DATA.

## Working hypothesis

On AMD Cezanne (gfx90c, Renoir family) under kernel 6.18.22-lts +
firmware MEC fw=0x1e2, ME fw=0xa7, PFP fw=0xc5: **WRITE_DATA inside a
user-submitted IB does not work** (or works only with a specific
preamble Mesa+gallium sets up via shader compilation that we haven't
reverse-engineered). NOP-only IBs also wedge the CP hard enough to
require MODE2 reset, which suggests **any** user IB lacking a real
compute dispatch packet wedges the firmware on this chip.

This is consistent with the empirical fact that no AMDGPU production
caller uses bare WRITE_DATA IBs. Mesa, libdrm tests, igt-gpu-tools
WRITE_DATA test all pair WRITE_DATA with shader-dispatch contexts —
the test suite running on actual production hardware always has
state surrounding it.

## Implications for v3

- **Switch B.4's proof-of-execution from WRITE_DATA to shader-store.**
  The store program at `programs/native_compute_store.cyr` already
  uses the real shader path (a 6-instruction `s_store_dword`), and it's
  what we should have been validating against from the start. WRITE_DATA
  was a "simplest possible diagnostic" that turned out to be broken
  on this firmware in this submission shape.
- **B.4 store path is the next candidate** — same kernel, same
  hardware, same submission API, but with a real DISPATCH_DIRECT
  setting up compute state. If it works, the WRITE_DATA failure is a
  curiosity we can document. If it also hangs, we've narrowed to
  context-state setup specifically.
- **Phase B status remains: ⚠ blocked, not closed.** The
  Session-9-onwards story is not "ours is broken, fix it" — it's
  "WRITE_DATA-on-Cezanne-from-direct-CS is a broken path in general,
  switch to shader-dispatch and retest."

## Next-session runbook (Session 12)

### Step A — confirm WRITE_DATA works for Mesa cl_probe peers

Find any libdrm test (e.g., `igt-gpu-tools/lib/amdgpu/amd_command_submission.c::amdgpu_command_ce_write_fence`) that does WRITE_DATA-only. Build and run on this hardware. Same result expected. Confirms the failure is reproducible across all WRITE_DATA-only submissions on Cezanne.

### Step B — try shader-store via libdrm

Adapt `deps/libdrm_spike.c` into `deps/libdrm_store_spike.c`:
- Add a real GFX9 store shader (12 instructions: `s_store_dword` + `s_endpgm`, byte-exactly the Mesa rusticl pattern from Session 7)
- Build a full compute IB: PGM_LO/HI/RSRC1/RSRC2/USER_DATA/NUM_THREAD/DISPATCH_DIRECT
- Submit via libdrm

If readback shows the sentinel: confirms our submission machinery is
fine, the diagnostic just needs a shader. **Phase B.4 is unblocked.**
Port the libdrm fix back to direct-ioctl Cyrius spike.

If readback fails: there's something else still wrong. Capture a fresh
coredump and investigate the ME state during a real dispatch hang.

### Step C — fall back to libdrm for B.4 if needed

If shader-via-libdrm works but shader-via-direct-ioctl-Cyrius doesn't,
we have an isolated regression that's purely in our Cyrius code path.
Strace-diff is the way forward.

If neither works on this hardware/kernel/firmware combo, **fall back
to wgpu-native for v3 GA**, document the native backend as
"experimental, gfx10+ only" or similar, and revisit on hardware that
isn't end-of-life.

## What's committed in this session

Pending commit (current branch state):

- `deps/libdrm_spike.c` — minimal libdrm_amdgpu reference, ~210 LOC,
  builds with `cc -O2 -Wall -o build/libdrm_spike deps/libdrm_spike.c -ldrm_amdgpu`.
- `programs/native_compute_spike.cyr` — IB padding bumped from 64 DW
  to 256 DW. Cosmetic correctness, didn't move the needle.

## Repo state at handoff

```
branch: v3
working tree: 2 modified files, 1 new file
  modified: programs/native_compute_spike.cyr (256 DW padding)
  modified: src/backend_native.cyr (Session 10 changes already committed?)
  new:      deps/libdrm_spike.c
  new:      docs/handoff/2026-04-25-session11-libdrm-also-hangs.md (this file)

CPU tests: 621 passed, 0 failed (cyrius 5.6.43, manifest pin 5.6.13).
GPU integration: still BLOCKED at the same hang location, but the
suspect surface is now narrowed to "any non-shader user IB on Cezanne
firmware". Diagnostic switch from WRITE_DATA to shader-store is the
next move.
```

## Memory updates from this session

- `feedback_write_data_on_cezanne.md` — new feedback memory: WRITE_DATA-only / NOP-only user IBs hang the GFX9 CP firmware on Cezanne (kernel 6.18.22, MEC fw=0x1e2). Diagnostic must use shader-dispatch.
- `project_first_native_dispatch.md` — refresh: Session 11 ruled out direct-ioctl as the cause; bug is at firmware/submission-shape level. Phase B.4 path forward is shader-store (which we already have) under libdrm reference first.

## Supporting material

- Session 11 coredump: `/tmp/amdgpu_coredump.txt` (consumed-on-read sysfs;
  may need re-capture). Contains full register dump, ring contents
  for all rings, HQD state, IP firmware versions.
- Mesa cs.cpp ref:
  https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/src/gallium/winsys/amdgpu/drm/amdgpu_cs.cpp
- libdrm cs.c ref:
  https://gitlab.freedesktop.org/mesa/drm/-/raw/main/amdgpu/amdgpu_cs.c
- Kernel cs.c parser:
  https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/drivers/gpu/drm/amd/amdgpu/amdgpu_cs.c
- Kernel gfx_v9_0.c ring funcs:
  https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/plain/drivers/gpu/drm/amd/amdgpu/gfx_v9_0.c
- Mesa sid.h packet defs:
  https://gitlab.freedesktop.org/mesa/mesa/-/raw/main/src/amd/common/sid.h
- Prior handoffs:
  - `docs/handoff/2026-04-23-session9-tdr-false-positive.md`
  - `docs/handoff/2026-04-25-session10-bo-handles-not-the-fix.md`
