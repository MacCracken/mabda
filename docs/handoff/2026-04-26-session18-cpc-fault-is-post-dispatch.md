---
title: Session 18 — 0x66d000 is post-dispatch CPC cleanup, not pre-dispatch
date: 2026-04-26
session: 18
branch: v3
hardware: AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c, Renoir family), Linux 6.18.24-lts
toolchain: cyrius 5.7.12
status: UNCOMMITTED — spike state changed, working tree dirty at session end
---

# Summary

Five attempts in one session, **two hypotheses falsified**, one major reframing of the root cause.

| # | Change | IB DWs | Outcome |
|---|--------|--------|---------|
| 1-2 | Session 17 attempt 2 — shader BO `AMDGPU_VA_RANGE_HIGH` (committed `2eae3cc`) | 75 | timeout + 0x66d000 CPC RW=1 — **Hypothesis D falsified** |
| 3 | Mesa-byte-exact USER_DATA values + count=1 packet shape for USER_DATA_0 | 74 | timeout, **NO 0x66d000 fault** |
| 4 | Real VAs + Mesa packet shapes (USER_DATA_2/3 count=2, USER_DATA_0 count=1, USER_DATA_1 count=1 separately) | 77 | timeout, **NO 0x66d000 fault** — looked like Hypothesis E confirmed |
| 5 | Same as #4 BUT shader = bare `s_endpgm` (2-DW) | 77 | timeout + **0x66d000 CPC RW=1 RETURNS** — **Hypothesis E falsified** |

The decisive flip was attempt 5: same IB as attempt 4, only the shader bytes changed (9-DW store kernel → 2-DW `s_endpgm`). The fault came back. Therefore the fault has nothing to do with USER_DATA packet shape — attempts 3 and 4 were red herrings that *hid* the fault.

# The reframe

**The 0x66d000 fault is in CPC's post-dispatch cleanup, not pre-dispatch.**

Sequence:

1. Wave dispatches — CP enters DISPATCH_DIRECT successfully.
2. Wave runs to completion (s_endpgm) **OR** hangs in s_waitcnt awaiting an unsatisfiable global_store.
3. *If the wave completes*, CPC starts post-dispatch cleanup → CSA save → write to `0x66d000` → UTCL2 fault (RW=1, PERMISSION_FAULTS=0x5, client CPC).
4. *If the wave hangs*, CPC never reaches cleanup → no fault, just 5-second ring timeout.

Attempts 3-4 had the global_store target a wrong VA (USER_DATA values were Mesa's literals or our address but the high half wasn't reaching the shader properly), so the wave hung in `s_waitcnt vmcnt(0)` waiting on a store that would never complete cleanly. The wave never finished, so the CPC cleanup write never fired, so we saw "no fault, just timeout" and incorrectly read that as "fault fixed."

Attempt 5 with bare `s_endpgm` removed the global_store entirely. The wave terminated immediately. CPC cleanup ran. Same `0x66d000` fault.

Sessions 7-17 hit the same trap. Every shader we tried either crashed in CP setup (preamble fault — different signature) or hung in body (no fault, no PASS). We never actually ran a wave to clean completion against this dispatch path until attempt 5 of Session 18.

# What's correct (don't re-derive)

- **PACKET3 count_minus_1** (Session 14) — necessary, correct.
- **TA_CS_BC_BASE_ADDR Mesa-byte-exact** (Session 16) — necessary, correct.
- **Trailing DMA_DATA terminator** (Session 17 attempt 1) — necessary, correct.
- **AMDGPU_VA_RANGE_HIGH for shader BO** (Session 17 attempt 2) — keep, matches Mesa.
- **USER_DATA Mesa-shape packets** (Session 18 attempt 4) — keep, matches Mesa byte-exact in IB structure.
- **Final IB**: 74 DWs (with 9-DW store shader; 77 with the count=1 USER_DATA_1 split as in attempt 4). Byte-exact identical to Mesa cl_probe IB save the 1-bit shader-VA delta in PGM_LO (`0x01000000` vs Mesa's `0x00000000` — libdrm allocator gave us shader at `0xFFFF800100000000` not `0xFFFF800000000000`).

# Where Session 19 should look

The IB content is solved. What's left is *kernel-side*: Mesa's `cl_probe` submits via the same amdgpu CS ioctl path but doesn't trigger the post-dispatch fault. The queue context setup must differ.

Top candidates:

1. **`amdgpu_cs_ctx_create2` with priority** — Mesa might use AMDGPU_CTX_PRIORITY_HIGH; we use `amdgpu_cs_ctx_create` which is normal-priority by default. Different priority paths may set up CSA differently.
2. **Per-queue CSA** — Mesa rusticl may explicitly allocate a CSA buffer and bind it via `amdgpu_va_range_alloc` + `amdgpu_bo_va_op` before submit. We do not.
3. **AMDGPU_GEM_CREATE_* flags** — Mesa may mark BOs with `CPU_ACCESS_REQUIRED`, `NO_CPU_ACCESS`, `EXPLICIT_SYNC`, etc. We use only `preferred_heap = GTT, flags = 0`.
4. **Lower-level submit path** — Mesa may use `amdgpu_cs_submit_raw2` with explicit chunks (`AMDGPU_CHUNK_ID_IB`, `AMDGPU_CHUNK_ID_BO_HANDLES`, possibly `AMDGPU_CHUNK_ID_FENCE`, `AMDGPU_CHUNK_ID_SYNCOBJ_*`). We use the high-level `amdgpu_cs_submit` wrapper.
5. **Context flags** — Mesa may pass specific flags in the AMDGPU_CTX_OP_ALLOC_CTX ioctl.

**Recommended first move**: strace both processes side-by-side and diff the AMDGPU ioctl sequences, especially `AMDGPU_CTX_OP_ALLOC_CTX`, `AMDGPU_GEM_CREATE`, `AMDGPU_CS`, `AMDGPU_VA_OP`. Reference logs already exist:

- `build/strace/cl_probe.log` — Mesa cl_probe ioctl trace (Apr 25 23:17)
- `build/strace/spike_vmav.log` — old spike trace (likely stale; re-capture against Session 18 binary)

```bash
strace -e trace=ioctl -f -o /tmp/spike_s18.log ./build/libdrm_store_spike
diff <(grep AMDGPU build/strace/cl_probe.log) <(grep AMDGPU /tmp/spike_s18.log)
```

# Reboot budget exhausted

7 MODE2 resets this session. The canary `cl_probe` survived all of them (verified before each spike attempt). But the official policy in `feedback_write_data_on_cezanne.md` is 3-per-boot, and we are well past it. **Reboot before Session 19.**

# Codebase state at session end

- `deps/libdrm_store_spike.c`: USER_DATA emit changed to Mesa-style packet shapes (USER_DATA_2/3 as count=2, USER_DATA_0 as count=1, USER_DATA_1 as a separate count=1 — 3 packets instead of 2). Shader bytes reverted to the 9-DW store kernel (attempt 5's bare s_endpgm rolled back). Comments rewritten to reflect Session 18's actual findings (Hypothesis E falsified, fault is post-dispatch cleanup). **UNCOMMITTED in working tree.**
- `cyrius.cyml`: pin still 5.7.12.
- `src/backend_native.cyr`, `programs/native_compute_*.cyr`: untouched.
- `dist/mabda.cyr`: no API change, no regen needed.

**Until Session 19 produces a real-dispatch confirmation post-reboot, do not treat Phase B.3 as done. Do not present mabda as having a working native backend. Do not quote dispatch latency.**
