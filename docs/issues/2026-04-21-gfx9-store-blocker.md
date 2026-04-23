# v3 Phase B.4 — GFX9 Compute Store Blocker

**Status:** B.3.d resolved 2026-04-23 (Session 7 PM4 fixes verified live); B.4 store verify still pending
**Date:** 2026-04-21 (opened) — 2026-04-23 (B.3.d retest passed)
**Affects:** Phase B.4 (verifiable compute dispatch on AMDGPU direct-ioctl path)
**Hardware:** AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c), Linux 6.18.22-lts

## Symptom

A minimum "write constant to memory" compute shader consistently triggers
`gfx_v9_0_bad_op_irq` ("Illegal opcode in command stream") and a
`MODE2` GPU reset. The non-store variant of the same shader (v_mov_b32
only, no memory access) dispatches cleanly and signals the sync-obj —
so the CS submission path, PM4 register setup, sync-obj handling, and
everything else in Phase B.1–B.3.d is working.

## What was tried

Every memory-store instruction variant fails identically:

- `global_store_dword v[0:1], v2, off` — with and without `glc slc`
- `flat_store_dword v[0:1], v2` — with and without `glc slc`
- `s_store_dword s3, s[0:1], 0x0` — scalar memory store
- All of the above with both **USER_DATA-loaded** addresses (RSRC2 USER_SGPR=2)
  and **hardcoded-in-shader** addresses (s_mov_b32 to build the address inline)

Every VA tried for the output buffer fails identically:

- `0x00600000` (6 MiB) — initial attempt
- `0x10000000` (256 MiB) — moved above low aperture
- `0x100000000` (4 GiB) — above 32-bit window
- `0x1000000000` (64 GiB) — well clear of 32-bit window

Shader binaries assembled by `clang -target amdgcn--amdhsa -mcpu=gfx90c`;
byte encodings cross-checked against AMD GCN5/Vega ISA documentation.

## Working PM4 register setup (B.3.d, s_endpgm dispatch)

- `COMPUTE_PGM_LO/HI` = shader_va split (VA >> 8 / VA >> 40)
- `COMPUTE_PGM_RSRC1` = 0x00AC0000 (FLOAT_MODE=0xC0 | DX10_CLAMP | IEEE_MODE)
- `COMPUTE_PGM_RSRC2` = 0 (no user SGPRs) or 0x04 (USER_SGPR=2)
- `COMPUTE_RESOURCE_LIMITS` = 0
- `COMPUTE_STATIC_THREAD_MGMT_SE0/SE1` = 0xFFFFFFFF
- `COMPUTE_TMPRING_SIZE` = 0
- `COMPUTE_USER_DATA_0/1` = output VA split (when USER_SGPR=2)
- `COMPUTE_NUM_THREAD_X/Y/Z` = 1
- `DISPATCH_DIRECT 1,1,1, initiator=0x5 (SHADER_EN | FORCE_START_AT_000)`

Submitted via `AMDGPU_HW_IP_COMPUTE`, IB + SYNCOBJ_OUT chunks, 5-second
sync-obj wait.

## Research hypotheses examined

1. **Wrong shader ISA encoding.** Ruled out — clang-assembled bytes
   match AMD GCN5 ISA doc layouts; non-store instructions from the same
   assembly run fine.

2. **`SYNCOBJ_OUT` chunk layout.** Solved separately (B.3.d) and
   captured as a vidya gotcha.

3. **`COMPUTE_STATIC_THREAD_MGMT_SE0..3` defaulting to 0.** Solved in
   B.3.d — CUs now enabled.

4. **GFX9 aperture classification.** Research identified
   `SH_MEM_BASES = 0x60006000` on Cezanne — private and shared
   apertures at VA `0x0006_xxxx_xxxx_xxxx`. Advice was to move the
   output buffer above the 32-bit window. **Empirically this didn't
   resolve it** — moving VA from 256 MiB to 64 GiB still triggers the
   same fault.

5. **Missing `SH_MEM_CONFIG` / `SH_MEM_BASES` register writes.**
   These are privileged (RLC-gated) and cannot be written from a PM4
   IB. They're kernel-programmed per-VMID at init. The agent
   hypothesis was that our VMID hadn't been through `init_compute_vmid`
   — but every other ioctl works, and `STATIC_THREAD_MGMT` wouldn't
   work either without proper VMID init. Unconfirmed.

## Session 2 additions (after three more research agents)

Additional fixes applied — **none of which have unblocked the store**:

- `TRAP_PRESENT=1` in PGM_RSRC2 (bit 6). Was missing; now `RSRC2 = 0x44` for the store-shader path.
- 16 dwords of `s_nop 0` trailing padding after `s_endpgm` (AMDGPU ABI requirement; SQ prefetches past shader end).
- `IT_ACQUIRE_MEM` packet with `coher_cntl = 0x38800000` (SH_ICACHE | SH_KCACHE | TC | TCL1 action_ena) emitted before `DISPATCH_DIRECT`.
- `SET_UCONFIG_REG CP_COHER_START_DELAY = 0` (GFX9-required queue preamble register that radv emits once per queue init).
- Per-SE CU masks now write all four: `COMPUTE_STATIC_THREAD_MGMT_SE0..SE3` at `0xB85C`, `0xB860`, `0xB864`, `0xB868` (previously had SE0 + SE1 + a WRONG register at 0xB864 labeled as `TMPRING_SIZE` but actually `SE2`; was silently disabling SE2 every dispatch).
- `RSRC1.SGPRS = 1` (16 SGPRs allocated, vs the previous 0 → 8 SGPRs, insufficient for USER_SGPR + VCC + flat_scratch per radv's accounting).

Failure mode evolution across all the attempts:
1. **Original**: `bad_op_irq` + GPU reset on any store.
2. **After VA aperture moves to 4 GiB / 64 GiB**: unchanged.
3. **After TRAP_PRESENT + NOP padding**: unchanged.
4. **After ACQUIRE_MEM + CP_COHER_START_DELAY + SE2/SE3 writes + SGPRS=1**: on some runs `bad_op_irq` still fires; on some runs dispatch "completes" but output buffer reads sentinel. It's possible the "completion" is actually a timeout-then-reset-signal cycle rather than a genuine dispatch success — dmesg still shows resets under the same timestamp window.

## Session 3 — GROUND TRUTH from Mesa AMD_DEBUG=ib (2026-04-21 evening)

Installed opencl-mesa + clinfo + strace. Built a minimal OpenCL "store 0xDEADBEEF to buf[0]" probe, ran it with `AMD_DEBUG=ib`. Mesa's rusticl dumped the full PM4 IB it emits for a successful compute dispatch on this exact Cezanne/gfx90c hardware.

**Biggest finding: my register offsets were wrong by a whole-register shift.** Every register from PGM_LO through TMPRING_SIZE was at the wrong byte offset. B.3.d's "dispatch completed" in earlier sessions was a false positive — the sync-obj was firing via the GPU-reset recovery path, not from actual shader execution. My "PGM_LO" writes were going to RSRC1; the real PGM_LO retained kernel-stale state, and the "dispatched shader" was whatever kernel-installed default lived at that address.

Corrected GFX9 register offsets (verified against Mesa IB dump):

| Register | Actual offset | What I had (wrong) |
|----------|--------------|--------------------|
| `COMPUTE_PGM_LO` | **0xB830** | 0xB848 (actually RSRC1) |
| `COMPUTE_PGM_HI` | **0xB834** | 0xB84C (actually RSRC2) |
| `COMPUTE_PGM_RSRC1` | **0xB848** | 0xB850 (reserved?) |
| `COMPUTE_PGM_RSRC2` | **0xB84C** | 0xB854 (actually RESOURCE_LIMITS) |
| `COMPUTE_RESOURCE_LIMITS` | **0xB854** | 0xB858 (actually SE0) |
| `COMPUTE_STATIC_THREAD_MGMT_SE0` | **0xB858** | 0xB85C (actually SE1) |
| `COMPUTE_STATIC_THREAD_MGMT_SE1` | **0xB85C** | 0xB860 (actually TMPRING_SIZE) |
| `COMPUTE_TMPRING_SIZE` | **0xB860** | not written |
| `COMPUTE_STATIC_THREAD_MGMT_SE2` | 0xB864 | 0xB864 (accidentally correct) |
| `COMPUTE_STATIC_THREAD_MGMT_SE3` | 0xB868 | 0xB868 (accidentally correct) |

Offsets corrected. Tests updated to match. `make test` = 602/602 pass.

**With offsets corrected, ALL hardware dispatches wedge the GPU** — even the `s_endpgm`-only `compute_spike`. This is actually the HONEST state. There's additional setup Mesa does that we don't, which this session identified in the IB dump but hasn't yet applied fully:

1. **SET_SH_REG `PGM_HI = 0x80`** unconditionally (Mesa's shaders live at VA `0x800000000000`, but user VA range on gfx90c caps below that — needs more investigation to find the valid high-VA shader slot).
2. **SET_UCONFIG_REG `TA_CS_BC_BASE_ADDR = 0x01004400` + `_HI = 0x80`** (border color base).
3. **`WRITE_DATA`** to VA `0xffff800100600300` (kernel-tracked, purpose unclear).
4. **`COMPUTE_PGM_RSRC2 = 0x8`** (USER_SGPR=4, not 2) — Mesa ABI expects USER_DATA_0..3 to hold something specific (likely scratch V# descriptor in s[0:3]).
5. **Scratch V# setup** — Mesa sets `USER_DATA_0 = 0x00200040` and `USER_DATA_2 = 0x00200000`. USER_DATA_0/1 is almost certainly the private_segment_buffer V# (scratch descriptor, 16-byte buffer resource). Even though SCRATCH_EN=0 in RSRC2, the hardware may validate s[0:3] as a V#.
6. **`COMPUTE_TMPRING_SIZE = 0x100`** (Mesa allocates 256 waves of scratch even for trivial kernels).
7. **`ACQUIRE_MEM CP_COHER_CNTL = 0xa8c40000`** (Mesa uses this exact value; we've been using 0x38800000).
8. **`DISPATCH_INITIATOR = 0x45`** (bits 0, 2, 6 — bit 6 is ORDER_MODE).

Next session's first move: match Mesa's complete PM4 stream byte-for-byte on a scratch-less GFX9 test, iterate until the dispatch actually runs our shader (verifiable by store succeeding).

## Session 4 — Mesa strace + more register semantics (2026-04-21 late evening)

Installed packages (strace, clinfo, opencl-mesa). Ran `strace -f -e trace=ioctl -v` on the working OpenCL compute probe. Full log preserved at `docs/issues/2026-04-21-mesa-cl-ioctl-trace.log`.

**New ground truth from the strace:**

1. **Mesa maps ALL its BOs in the canonical-high VA aperture**, specifically starting at `0xFFFF_8001_0000_0000` with 2 MiB strides: handle 1 → `0xFFFF_8001_0000_0000`, handle 3 → `0xFFFF_8001_0020_0000`, etc. My mabda code had been mapping at low VAs (2 MiB / 4 MiB) which the kernel accepts but the GPU's compute path doesn't actually run shaders from.

2. **The apparent "PGM_HI = 0x80 = 128 TiB shader VA" that made no physical sense turns out to be encoding bits[47:40] of a canonical-high VA `0xFFFF_8001_xxxxxxxx`.** The 0x80 + the implicit bits 39:32 = 0x01 (from the GFX9 shader aperture) combine to give the full 48-bit canonical VA — sign-extended upward by the hardware. PGM_LO holds the low 32 bits, PGM_HI holds bits 47:40, and bits 39:32 are an implicit 0x01 tied to the aperture. My PGM encoding had been assuming a much lower VA range.

3. **Mesa uses `DRM_IOWR` (dir=3) for `AMDGPU_GEM_VA`, not `DRM_IOW` (dir=1)** that the header declares. Kernel accepts both — not a blocker but worth noting.

**Applied this session:**
- Shader and IB VAs moved to canonical-high aperture.
- Both `compute_spike.cyr` and `compute_store.cyr` updated.
- PGM_LO/HI encoding confirmed: `PGM_LO = va & 0xFFFFFFFF`, `PGM_HI = (va >> 40) & 0xFF`.

**Result:** still wedges. So the VA aperture was *a* missing piece but not the final one. The next most likely remaining delta is Mesa's `WRITE_DATA` preamble packet (writing 4 bytes to `0xFFFF_8001_0060_0300`) which appears to be a kernel-tracked progress marker. Without it, the kernel may not know our submission actually advanced, and consider it stuck.

**Saved artifacts for next session:**
- Full Mesa ioctl trace: `docs/issues/2026-04-21-mesa-cl-ioctl-trace.log`
- Mesa PM4 IB dump (not saved, but reproducible via `AMD_DEBUG=ib ./build/shader/cl_probe`)
- OpenCL probe source: `build/shader/cl_probe.c` (used to reproduce the working-dispatch trace)

Concrete next moves (in order of probability):
1. Emit Mesa's `WRITE_DATA` packet (opcode 0x37) writing 0 to `0xFFFF_8001_0060_0300` in the preamble.
2. Set `TA_CS_BC_BASE_ADDR` to point at a canonical-high VA matching Mesa's `0x80_01004400`.
3. Capture Mesa's AMDGPU_CS ioctl payload byte-for-byte; build our CS ioctl to match chunk-by-chunk.
4. If still stuck, check whether Mesa uses `AMDGPU_VM_OP_RESERVE_VMID` via a separate VM ioctl before GEM_VA.

## Session 5 — Devcoredump pinpoints VA-0 fault (2026-04-22)

Captured `/sys/class/drm/card0/device/devcoredump/data` (6 MB, 290k lines) during a fresh wedge. Key findings:

```
[gfxhub] Page fault observed
Faulty page starting at address: 0x0000000000000000
mmVM_L2_PROTECTION_FAULT_STATUS = 0x00700881
  → CID=1 (CP or shader memory client)
  → VMID=8 (compute VMID)
```

**Fault is at VA 0x0**, not a high-VA mis-translation. Combined with the compute VMID (8) and the CP/shader client, this matches the hypothesis that **the hardware dereferences `s[0:3]` as a private_segment_buffer V# descriptor during dispatch setup**, even when the shader doesn't use scratch. Zero-valued SGPRs → zero V# base → access at VA 0 → fault.

**Mesa's approach that avoids this:**
- `COMPUTE_PGM_RSRC2 = 0x8` (USER_SGPR=4, TRAP_PRESENT=0)
- `COMPUTE_USER_DATA_0 = 0x00200040` (V# word 0: base low bits at 2 MiB + 64)
- `COMPUTE_USER_DATA_2 = 0x00200000` (kernel arg pointer, 2 MiB)
- USER_DATA_1/_3 left at 0 (V# word 1 = high bits + stride, word 3 = kernarg high/stride)

Mesa deliberately puts its data buffers at LOW VAs (`0x00200000`-range) while shader BOs live at canonical-high (`0xFFFF_8001_XXXXXXXX`). The V# base thus points to valid low-VA mapped memory, satisfying the HW's implicit scratch-setup check without actually using scratch.

**Saved artifacts:**
- `/tmp/gpu_dump.txt` — 6 MB devcoredump with register state at fault time
- `docs/issues/2026-04-21-mesa-cl-ioctl-trace.log` — Mesa OpenCL ioctl trace
- Reproduction: `./build/native_compute_spike` → wedges → `sudo cat /sys/class/drm/card0/device/devcoredump/data > /tmp/gpu_dump.txt`

**Concrete next steps (in order):**
1. Set `RSRC2 = 0x8` (USER_SGPR=4).
2. Emit `SET_SH_REG COMPUTE_USER_DATA_0..3` with Mesa's exact values: 0x00200040, 0, 0x00200000, 0. Even if we don't have a real scratch BO, these values are non-zero enough to satisfy the HW's V# base check, and they point to the LOW-VA area which we can map.
3. Map a dummy "scratch" BO at VA 0x00200000 with some reasonable size so the V# base resolves to valid memory. The GPU won't actually touch it (our shader doesn't use scratch), but the HW's implicit check passes.
4. On spike (s_endpgm) we expect clean execution (no wedge, no sync-obj-via-reset).
5. On store, proceed with the actual shader once the spike baseline is clean.

## Session 6 — The wedge is in kernel init, not our PM4 (2026-04-22)

Applied Session 5's proposed fix: stub BO at VA 0x200000, RSRC2=0x8 (USER_SGPR=4), USER_DATA_0..3 = Mesa's V# values (0x00200040, 0, 0x00200000, 0), stub BO added to BO list.

**Result: same fault, same VA, same everything.**

Captured fresh devcoredump (`/tmp/gpu_dump2.txt`). Critical new observation from `mmCP_HQD_IB_BASE_ADDR = 0x00b88c00`:

> The MEC is executing an IB at VA 0x00b88c00 — a LOW kernel-GART VA, NOT our IB (which we mapped at `0xFFFF_8001_0020_0000`). This is a **kernel-inserted initialization IB** that runs BEFORE user submissions as part of context/VMID setup. Our IB never runs because the kernel's own init PM4 is what's faulting at VA 0.

**Revised root cause theory:** the problem is not in our PM4 stream at all. Our context is landing on a VMID that the kernel hasn't fully initialized for compute. Its init IB tries to access something at VA 0 (probably the SH_MEM aperture state, or a kernel tracking BO with a zero-initialized base) and faults.

Mesa works because libdrm_amdgpu's `amdgpu_cs_ctx_create` wraps the ioctl with additional setup that triggers `init_compute_vmid` on the kernel side — which our raw `DRM_IOCTL_AMDGPU_CTX` apparently doesn't fully trigger.

**This means more PM4 fiddling won't help. The fix is at the context/VM layer:**

1. Try `DRM_IOCTL_AMDGPU_CTX` with different flags (`AMDGPU_CTX_PRIORITY_HIGH`, `AMDGPU_CTX_FLAG_RECOVERY`, etc.).
2. Add a `DRM_IOCTL_AMDGPU_VM` call with `AMDGPU_VM_OP_RESERVE_VMID` after context alloc — may force kernel to run the full compute-VMID init path.
3. Strace Mesa's `DRM_IOCTL_AMDGPU_CTX` args specifically — compare flags byte-for-byte.
4. Last resort: use libdrm_amdgpu's `amdgpu_cs_ctx_create` (import as a C dep) to validate the context-setup theory. If dispatch succeeds via libdrm's context but fails via raw ioctl with identical PM4, we've proven it's context-init and can then reverse-engineer libdrm's specific init calls.

**Saved state:**
- `/tmp/gpu_dump2.txt` — second coredump, same fault pattern.
- Stub BO + USER_SGPR=4 + USER_DATA V# values are in the code (didn't help but they're Mesa-matched and wanted eventually anyway).
- `DISPATCH_INITIATOR = 0x45` restored to match Mesa exactly.

The specific next test: **strace Mesa's DRM_IOCTL_AMDGPU_CTX call with full arg bytes, diff against what mabda sends.** If there's any flag bit difference, that's likely the answer.

## Likely next angles (for a future session)

1. **Is there a `COMPUTE_PGM_RSRC3` on GFX9?** GFX10+ has one. Vega
   docs suggest no, but worth double-checking in Mesa radv's
   `radv_emit_compute_shader` path.

2. **`ENABLE_SGPR_PRIVATE_SEGMENT_BUFFER` in PGM_RSRC2.** If this bit
   is implicitly expected to be set and we're not setting it, the SQ
   may refuse to issue any memory op. The "private segment buffer" is
   the scratch V# — without it, the segment check may fail.

3. **Shader wrapper / AMDHSA kernel descriptor.** ROCm's HSA runtime
   prepends a 64-byte `amd_kernel_code_t` descriptor before the ISA
   bytes. Our direct-PM4 path skips this entirely because PM4 writes
   PGM_LO/HI/RSRC1/RSRC2 directly, but maybe on GFX9 the kernel
   descriptor is load-bearing for some other reason (e.g., initializing
   hidden registers the CP reads).

4. **KMD ring selection.** We submit to `AMDGPU_HW_IP_COMPUTE` which
   lands us on `comp_1.*.*` ring (confirmed in dmesg). But maybe
   stores from the non-KFD compute ring require specific USER_DATA
   slots pre-programmed to kernel-provided values the way ROCm's KFD
   path does.

5. **`libdrm_amdgpu` comparison.** Strace a simple OpenCL program
   doing a single memory store to compare the exact CS chunks and
   register writes Mesa emits vs what we emit. The delta would be
   exactly what we're missing.

## Checkpoint state

- `src/backend_native.cyr`: `native_gfx9_shader_store_deadbeef` is
  stubbed to the v_mov+endpgm diagnostic variant. Does not wedge the
  GPU on run; does not write anything either, so
  `programs/native_compute_store.cyr` expectedly fails at the verify
  step (sentinel still in place).
- `programs/native_compute_store.cyr`: output VA = 0x1000000000 (64 GiB).
- All other hardware integration programs (`native_device_enum`,
  `native_gem_roundtrip`, `native_submit_setup`, `native_compute_spike`)
  continue to pass.
- 598 CPU tests + 3 fuzz harnesses green.

## What it means for Phase B

Phase B.3 stated exit ("compute dispatch completes without kernel
error") is MET. Phase B.4 stated exit ("byte-identical compute output
vs wgpu") is NOT met — we can't verify output because stores wedge.

The wgpu backend does compute round-trips in `programs/compute_e2e.cyr`
(shipping since v2.4.0), so consumer-level compute *does* work on this
hardware — just not via our direct-ioctl path with the PM4 state we're
programming. That gap is the open question.

## Session 7 (2026-04-22): PM4 builder bug — found via Mesa IB diff

Ran `AMD_DEBUG=ib ./build/shader/cl_probe` to dump the exact PM4
stream Mesa emits for a working compute dispatch, and diffed it
byte-by-byte against ours.

**Two genuine encoding bugs in the shared PM4 builder:**

### 1. ACQUIRE_MEM count field off-by-one

`native_pm4_acquire_mem_full_invalidate` was calling
`native_pm4_pkt3_header(IT_ACQUIRE_MEM, 7, 2)`. The `count` parameter
is documented as "number of data dwords following the header"; the
formula subtracts one: `(count - 1) & 0x3FFF`. ACQUIRE_MEM on GFX9 has
6 data dwords (coher_cntl, size_lo, size_hi, base_lo, base_hi,
poll_interval). Passing `count=7` made the header claim 7 data dwords.

Mesa emits `c0055802`. We emitted `c0065802`. The CP, following the
header's word-count, consumed an extra dword past our last payload —
namely the **next packet's header** (SET_SH_REG PGM_LO, 0xC0017600).
After this point every subsequent packet boundary was misaligned: CP
interpreted `native_sh_reg_offset(R_COMPUTE_PGM_LO)` (0x20C) as a new
PKT3 header (invalid — top bits are zero, so type=0), causing the
illegal-opcode trap and GPU reset.

This fully explains why every Session 1–6 experiment wedged in what
looked like different places: any post-ACQUIRE_MEM work we added was
being parsed as garbage data by the CP. The "fault at VA 0" in
coredumps was the CP dereferencing whatever dword happened to land in
the SRC_ADDR slot of a mis-parsed DMA_DATA opcode.

Fix: `native_pm4_pkt3_header(IT_ACQUIRE_MEM, 6, 2)` →
header `0xC0055802`, matching Mesa.

### 2. DISPATCH_DIRECT missing shader_type=2

The low byte of IT_DISPATCH_DIRECT's PKT3 header is `shader_type`
(0 = graphics, 2 = compute) — not a predicate bit. We were emitting
`0xC0031500`; Mesa emits `0xC0031502`. Whether this alone would wedge
a compute dispatch on GFX9 is unclear (some docs suggest the CP
ignores it for DISPATCH_DIRECT on the compute ring), but it's wrong
either way. Fixed by passing `2` as the predicate/type arg.

### Other functional gaps aligned to Mesa

Also in the Mesa dump but missing or wrong in our spike:

- `COMPUTE_RESOURCE_LIMITS = 0x140` (WAVES_PER_SH=320). We wrote 0,
  which caps the SH to zero waves — silent stall even if PM4 parses
  correctly.
- `COMPUTE_TMPRING_SIZE = 0x100`. Unset in our prior spike.
- `STATIC_THREAD_MGMT_SE1/SE2/SE3 = 0` (Cezanne has 1 SE). We wrote
  all 0xFFFFFFFF; Mesa zeros the absent-SE mask registers.
- Register ordering — Mesa programs STATIC_THREAD_MGMT before the
  UCONFIG preamble, not after.

Updated `programs/native_compute_spike.cyr` to emit the preamble in
Mesa's exact order with the correct register values. New PM4 size:
~56 dwords + NOP pad to 64. Added
`test_native_pm4_acquire_mem_layout` (byte-exact header + payload)
and fixed `test_native_pm4_dispatch_direct_layout` to expect
`0xC0031502`. Test count 602 → 610 (8 new assertions). All green.

**Next live test:** rerun `./build/native_compute_spike`. Expected
outcome: the `s_endpgm`-only dispatch (no memory ops) signals the
sync-obj cleanly without wedging the GPU. If confirmed, the
previously-observed wedges are explained by the off-by-one and the
path to Phase B.4 reopens — the shader store experiments can resume
with a correctly-built PM4 stream.

## Session 8 (2026-04-23): B.3.d retest — PASSED

Retest ran on the freshly-rebooted work machine immediately after the
active cyrius toolchain was switched from the in-dev 5.6.14 back to
the released 5.6.13. Spike binary rebuilt from source under 5.6.13
(224,984 bytes), 610/610 CPU assertions green, then:

```
$ ./build/native_compute_spike
mabda native compute spike (v3 Phase B.3.d)
-------------------------------------------
fd=3
shader bo=1 va=0x-140733193388032
stub bo=2 va=0x2097152
pm4 stream bytes=256
ib bo=3 va=0x-140733191290880
ctx_id=1
bo_list_handle=1
syncobj=1
submitted to COMPUTE ring
cs submitted
dispatch completed (sync-obj signaled)
OK
$ echo $?
0
$ sudo dmesg --since "1 minute ago" | grep -iE "amdgpu|drm|gfx|bad_op|reset|ring|hang|fence"
(empty)
```

- RC=0, no kernel log entries — no `gfx_v9_0_bad_op_irq`, no `MODE2`
  reset, GPU stayed healthy through the run.
- This is the first genuine pass of B.3.d (Session 3's "pass" was a
  false positive via reset-recovery sync-obj signaling; Session 8's
  pass has byte-correct PM4 and a silent dmesg).

**Vindicates the Session 7 analysis.** Two encoding bugs in the PM4
builder (`ACQUIRE_MEM` count off-by-one; `DISPATCH_DIRECT` missing
`shader_type=2`) were responsible for every wedge observed in Sessions
1–6. The VMID/scratch/SH_MEM/VA-aperture theories chased across those
sessions were all downstream of CP stream desync; once the CP parses
the stream correctly, dispatch just works.

**Cosmetic observation from the spike output (not a correctness
issue):** `shader bo=1 va=0x-140733193388032` and `ib bo=3
va=0x-140733191290880` print the canonical-high VAs as signed i64.
Worth a one-line `fmt` fix in the spike's logging when we're next in
that file.

### What this closes / what remains

- **Closed:** B.3.d retest. The PM4 wedge blocker that opened this
  issue on 2026-04-21 is resolved.
- **Still open:** the original issue title ("GFX9 Compute Store
  Blocker"). Stores themselves haven't been re-attempted since the
  PM4 fixes — `programs/native_compute_store.cyr` still uses the
  diagnostic `s_endpgm`-only stub from Session 6. B.4's stated exit
  ("byte-identical compute output vs wgpu") requires swapping the
  stub for the real store variant and verifying readback.

This issue stays open until the store variant runs green. When it
does, close with a final "Session N: store verified" section and
update the v3-native-api-principles.md Phase B status block.
