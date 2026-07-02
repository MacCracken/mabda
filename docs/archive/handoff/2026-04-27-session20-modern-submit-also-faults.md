---
title: Session 20 — modern submit_raw2 path also triggers 0x66d000; fault is in kernel VMID 0, not user VM
date: 2026-04-27
session: 20
branch: v3
hardware: AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c, Renoir family), Linux 6.18.24-lts
toolchain: cyrius 5.7.12
status: Session 19 hypothesis FALSIFIED; new evidence narrows the search
---

# Summary

Converted `deps/libdrm_store_spike.c` from the legacy submit path
(`amdgpu_bo_list_create` + `amdgpu_cs_submit` + `amdgpu_cs_query_fence_status`)
to the modern path that Mesa cl_probe uses (`amdgpu_cs_submit_raw2` with
chunks `[IB, BO_HANDLES, SYNCOBJ_OUT]` + `amdgpu_cs_syncobj_wait`).

The modern submit produces the **same 0x66d000 CPC fault** as the
legacy submit. **Session 19's submit-path hypothesis is falsified.**

Two new pieces of evidence surfaced:

1. **dmesg fault tag is `vmid:0 pasid:0`** — the fault is in **kernel
   VMID 0**, not in our process VM. This means the faulting access is
   issued under the kernel's address space, by firmware that has been
   pre-configured by the kernel before our user IB runs.
2. **`out[0]` is unchanged after timeout** — sentinel `0xBAADF00D`
   stays. The wave did NOT write `0xDEADBEEF` to the output buffer.
   This **challenges Session 18's reframe** ("fault is post-dispatch
   cleanup, wave reaches s_endpgm first") — either the wave hangs in
   `s_waitcnt` and never reaches the store, or kernel TDR rolls the
   GTT BO content back to its pre-submit state during recovery.

# What was attempted

Three attempts in one session, all on the same v3 branch + spike binary:

| # | Change | Outcome |
|---|--------|---------|
| 1 | `submit_raw2` with chunks `[IB, BO_HANDLES, SYNCOBJ_OUT]` and `bo_handles_data = drm_amdgpu_bo_list_entry[4]` direct | CS ioctl → ENOENT — kernel parser reads BO_HANDLES chunk_data as `drm_amdgpu_bo_list_in` struct (`{operation, list_handle, bo_number, bo_info_size, bo_info_ptr}`), not as raw entry array |
| 2 | Wrap entries in `drm_amdgpu_bo_list_in`, point `bo_info_ptr` at the entries | CS submit OK; `syncobj_wait` returned ETIME after 0 ms because `timeout_nsec` is **absolute CLOCK_MONOTONIC**, not relative |
| 3 | Compute absolute deadline; print `out[0]` even on timeout | CS OK, syncobj never signaled (5089 ms ETIME), `out[0] = 0xBAADF00D` unchanged, dmesg shows the same `0x66d000 / CPC / vmid:0 pasid:0` fault, GPU recovered (cl_probe canary still passes) |

# What was learned

## The submit path is NOT what differs

Both paths feed CPC the same kernel-allocated state. Whatever causes
the post-dispatch fault is set up by the kernel during MEC/queue init
or by a register write triggered by something in our IB — and it is
**independent of `bo_list_handle` vs `BO_HANDLES` chunk, and of
seq_no fence vs syncobj fence.**

## The fault VA lives in kernel VM, not user VM

`vmid:0 pasid:0` is the kernel's own address space. CSA per-context
saves are normally tagged with the user VMID, so this isn't the
canonical CSA save path. Candidates for what gets accessed at
0x66d000 in kernel VMID 0:

- MEC firmware-internal state (HQD/CSA/EOP for a kernel-managed queue)
- A kernel ring's user-fence VA (kernel-allocated)
- A kernel-reserved BO mapped in VMID 0 that didn't get set up

The amdgpu kernel's `AMDGPU_VA_RESERVED_CSA_START` resolves to a high
canonical VA on gfx9; 0x66d000 is too low to be CSA. So the fault is
not the CSA save.

## IB diff vs Mesa IB1 (74 DWs)

After the fix to extract IB1 only (Mesa submits two IBs: 74 DW
dispatch + 41 DW cleanup/sync — we only mirror IB1), the deltas are:

- DW 26: PGM_LO `00000000` (Mesa) vs `01000000` (us) — known shader
  VA delta, our libdrm allocator gave shader at `0xFFFF800100000000`
  not `0xFFFF800000000000`.
- DWs 36-37: USER_DATA_2/3 — Mesa points at `0x0000000000200000`
  (its rusticl arg-block VA), we point at our stub_va `0x100002000`.
  This is the **OpenCL ABI** delta — Mesa's shader expects a
  scalar-register layout that loads its real argument indirectly
  through the kernel-arg block; ours expects USER_DATA_0/1 to be the
  output BO directly. Setting Mesa's exact values would just make our
  shader read garbage. This delta is correct for our ABI.
- DW 40 onwards: Mesa packs USER_DATA_0/1 as a single SET_SH_REG
  count=2 packet; we emit two separate count=1 packets. **Functionally
  equivalent** in PM4 terms, but the packet shape differs by 3 DWs.

These three deltas are what Session 18 ended on. None of them
explains a fault in *kernel* VMID 0.

# Fixes that landed in `deps/libdrm_store_spike.c`

- Switched submit to `amdgpu_cs_submit_raw2` with `IB`, `BO_HANDLES`
  (correctly wrapped in `drm_amdgpu_bo_list_in`), and `SYNCOBJ_OUT`.
- Replaced fence wait with `amdgpu_cs_syncobj_wait` using an
  **absolute** CLOCK_MONOTONIC deadline.
- Spike now reads `out[0]` even on syncobj timeout — this gave us the
  `0xBAADF00D` evidence that's a key data point for Session 21.
- Teardown drops `amdgpu_bo_list_destroy` (no list now) and adds
  `amdgpu_cs_destroy_syncobj`.

These changes are working but **uncommitted**.

# Where Session 21 should look

Order of cheapness, not certainty:

1. **Try the GFX ring instead of COMPUTE.** One-line change:
   `AMDGPU_HW_IP_COMPUTE` → `AMDGPU_HW_IP_GFX`. The fault is tagged
   `client CPC` — if it goes away on the GFX ring (which goes through
   ME, not MEC), that confirms the issue is MEC-specific. If it
   moves to a different client, it's a generic write-to-bad-VA bug
   in our IB. If it disappears entirely, Phase B.4 is unblocked via
   GFX ring (slower but works). Mesa rusticl probably uses COMPUTE,
   but radv compute on the same hardware uses GFX as its compute
   ring sometimes.

2. **Reproduce the fault with bare `s_endpgm` shader (Session 18
   attempt 5).** Now that we read `out[0]` post-timeout, we can
   distinguish:
   - Wave actually completed (out unchanged because the shader does
     nothing) — confirms post-dispatch fault model
   - Wave never started (out unchanged because dispatch was killed) —
     refutes post-dispatch model

3. **Byte-copy Mesa IB1 verbatim** (with Mesa's USER_DATA values
   `0x00200040 / 0x00200000`). The shader will read garbage and
   probably hang in `s_waitcnt`, but the *kernel-side* state will be
   set up identically to cl_probe. If the fault still triggers, it's
   not in our IB content. If it disappears, it's in the packet shape
   delta.

4. **Decode `ring:40` in dmesg.** The fault is on ring 40 of
   gfxhub0. Cross-reference against `journalctl -k --grep "ring "`
   from boot to identify which compute pipe/queue this is. The kernel
   logs ring assignments like `ring comp_1.X.Y uses VM inv eng N on
   hub 0` at boot — match ring 40 to one of those.

5. **Compare amdgpu_info AMDGPU_INFO_HW_IP_INFO output** between
   spike and a healthy program. Mesa cl_probe queries this 36 times
   total — many are firmware version probes, but one or two might be
   `INFO_HW_IP_INFO` for compute, and the kernel's response shapes
   our submission. We don't query this at all.

# Reboot status

Two ring timeouts this session (one ENOENT pre-submit didn't touch
the GPU; two reached the kernel and triggered TDR + reset). Cl_probe
canary survived all three. No MODE2 reset escalation needed. Reboot
budget remains lighter than expected — Cezanne kernel 6.18.24 is
recovering cleanly from compute-ring TDRs in this scenario.

# Codebase state at session end

- `deps/libdrm_store_spike.c` — modified to modern submit path,
  **uncommitted**. Need to commit before Session 21.
- All other source unchanged. Cyrius backend (`src/backend_native.cyr`,
  `programs/native_compute_*.cyr`) untouched.
- `dist/mabda.cyr` — no API change.
- Logs: `/tmp/cl_probe_s19.log` (canary), `/tmp/spike_s20b.log` (modern
  spike fault), `/tmp/mesa_ib1.txt`, `/tmp/spike_ib.txt` — preserve by
  copying into `build/strace/` if Session 21 needs to reference.

**Until Session 21 produces real-dispatch confirmation, do not treat
Phase B.3 as done. Do not present mabda as having a working native
backend. Do not quote dispatch latency.**
