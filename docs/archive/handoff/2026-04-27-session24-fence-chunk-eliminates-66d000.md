# Session 24 — FENCE chunk eliminates the 0x66d000 fault; IB still doesn't execute

**Date:** 2026-04-27
**Branch:** v3
**Hardware:** AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c, Renoir family)
**Kernel:** 6.18.24-1-lts, MEC fw 0x1e2

## Headline

**The 0x66d000 / CPC / UTCL2 fault that has dominated Sessions 14-23 is gone.**
Two spike attempts this session, both with the new 4-chunk Mesa-shape submit. Neither produced the page fault. dmesg shows only `ring comp_1.1.0 timeout` followed by clean MODE2 reset; post-spike `cl_probe` canary stays green both times.

The IB still doesn't execute (`out[0] = 0xBAADF00D` poison preserved), but the failure mode has fundamentally changed.

## What ran

| # | Build | Result | dmesg | Canary |
|---|-------|--------|-------|--------|
| Pre | `cl_probe` | `0xDEADBEEF` ✓ | clean | n/a |
| 1 | `libdrm_store_spike_v3` (4-chunk Mesa-shape, shader HIGH only) | timeout, `0xBAADF00D` | ring timeout + MODE2 reset (no 0x66d000) | green |
| 2 | `libdrm_store_spike_v3` (4-chunk Mesa-shape, ALL BOs HIGH) | timeout, `0xBAADF00D` | ring timeout + MODE2 reset (no 0x66d000) | green |

Stopped before attempt 3 — Session 23 documented "3 MODE2/boot before permanent wedge" rule; we used 2 this session.

## How the FENCE chunk was identified

Built `deps/libcsdump.c` — an LD_PRELOAD shim that intercepts `ioctl()` and dumps the payload of every `DRM_IOCTL_AMDGPU_CS`, `DRM_IOCTL_AMDGPU_BO_LIST`, and `DRM_IOCTL_AMDGPU_GEM_VA` call. Ran it under cl_probe (no GPU cost — succeeds normally) and got byte-exact ioctl payloads:

```
==== CS #1 ====
  ctx_id=5 bo_list_handle=0 num_chunks=4 flags=0x00000000
  chunk[0] id=6(BO_HANDLES)   length_dw=6   bo_number=6  op=0xffffffff list_handle=0xffffffff
  chunk[1] id=5(SYNCOBJ_OUT)  length_dw=1
  chunk[2] id=2(FENCE)        length_dw=2   handle=9 offset=32
  chunk[3] id=1(IB)           length_dw=8   flags=0x00000008 va_start=0xffff800100220000 ib_bytes=320
```

vs the spike's previous 3-chunk submit (IB + BO_HANDLES + SYNCOBJ_OUT, no FENCE, BO_HANDLES op=0, IB flags=0).

Four byte-level differences, all fixed in `deps/libdrm_store_spike_v3.c`:
1. Add the `AMDGPU_CHUNK_ID_FENCE` chunk pointing at a user-allocated 4 KB BO at offset 32
2. `BO_HANDLES.operation = BO_HANDLES.list_handle = 0xffffffff` (Mesa-inline magic)
3. IB flags `|= AMDGPU_IB_FLAG_TC_WB_NOT_INVALIDATE` (0x08)
4. BO priorities (1, 3, 3, 4, 10) — cosmetic eviction hint

## Why this works (theory)

Without the FENCE chunk, the kernel synthesizes a fallback user-fence target VA. On Cezanne kernel 6.18.24 that fallback target lands in the per-queue scratch / EOP / HOQ region adjacent to the kernel's MQD allocations — exactly where Session 23's devcoredump pinned 0x66d000 (8 pages above the last MQD page). The FENCE chunk replaces that fallback with a userspace BO the kernel and CPC firmware can both write cleanly.

The `0xffffffff` magic on BO_HANDLES is the kernel's signal that this CS uses inline residency only — don't allocate a persistent BO_LIST.

## What stays falsified, what's newly falsified

| Hypothesis | Status |
|---|---|
| IB content / packet shape (Session 17) | Falsified |
| Ring-specific MQD shadowing (Session 22) | Falsified Session 23 |
| USER_DATA packet shape (Session 21) | Falsified |
| All-HIGH-VA placement is sufficient | **Newly falsified Session 24** — attempt 2 had ib/shader/out/stub/fence all in upper-canonical and still timed out |

## Open blocker — Session 25 starting point

After v3 submits cleanly, comp_1.1.0 times out (`signaled seq=8, emitted seq=9`). No page fault. MODE2 reset succeeds. The IB queues but never executes — out[0] stays `0xBAADF00D`.

Most-likely-cause ladder, ordered by Session 25 priority:

### 1. Missing preamble IB (high-likelihood)

Mesa submits **two** IBs per cl_probe:
- CS #1: `ib_bytes=320` (80 DWs)
- CS #2: `ib_bytes=192` (48 DWs)

The first IB is likely a **preamble** that sets up persistent compute-queue state (CSA region, scratch base, persistent SH regs). Our spike sends one combined 300-byte IB. If Mesa's CS #1 establishes context that CS #2 then assumes, our single-shot IB is missing the setup half.

**Test:** capture Mesa's CS #1 IB byte stream (`build/strace/cl_probe_ib.log` already has it via `AMD_DEBUG=ib`), identify the packets in CS #1 that are NOT in our spike, replicate them in a preamble dispatch, and submit twice (preamble + actual).

### 2. Get devcoredump for the new failure (information gap)

Session 23's devcoredump told us exactly where 0x66d000 came from. We need the same X-ray for the new failure mode. Devcoredump capture requires sudo; we ran without sudo this session. Next session: cache sudo creds first, run a single spike attempt, snapshot `/sys/class/drm/card0/device/devcoredump/data` immediately. Look for `CP_HQD_IB_BASE_ADDR` — if non-zero, MEC loaded the IB and the wave hung in shader; if zero, MEC scheduler still rejecting submission.

### 3. Try AMDGPU_HW_IP_GFX (sidestep, planned in Session 24 originally)

Session 21 evidence: GFX path produced `ring gfx timeout` *without* the UTCL2 fault. Now that COMPUTE also produces a fault-free timeout, the GFX path may behave the same. But GFX queues run a different scheduler and may surface a more diagnosable failure. Worth trying after (1) and (2).

## Codebase changes this session

```
new:    deps/libcsdump.c                                — LD_PRELOAD ioctl dumper (~210 LoC)
new:    deps/libdrm_store_spike_v3.c                    — Mesa-shape 4-chunk submit (511 LoC, copy of v1 + delta)
new:    docs/issues/session24/cl_probe.csdump           — byte-exact Mesa ioctl payloads
new:    docs/issues/session24/spike_v3.csdump           — v3 with shader-only HIGH (timeout)
new:    docs/issues/session24/spike_v3_highva.csdump    — v3 with all-BOs HIGH (timeout)
unchanged: deps/libdrm_store_spike.c                    — preserved as control (the v1 that always faulted)
```

## Verified necessary fixes (locked in for Session 25+)

| Fix | Session | Why |
|-----|---------|-----|
| PACKET3 count_minus_1 = body_dws - 1 | 14 | Off-by-one desyncs IB |
| TA_CS_BC_BASE_ADDR = 0x01004400 / 0x80 | 16 | Mesa magic VA, not shader_va |
| Trailing DMA_DATA CP_SYNC=1 BYTE_COUNT=0 | 17 | Mesa-byte-exact end of IB |
| Shader BO at AMDGPU_VA_RANGE_HIGH | 17 | Matches PGM_HI=0x80 placement |
| amdgpu_cs_submit_raw2 + syncobj | 19/20 | Modern submit path |
| **4-chunk Mesa-shape submit (with FENCE)** | **24** | **Removes the 0x66d000 fault** |
| **BO_HANDLES.operation/list_handle = 0xffffffff** | **24** | **Inline-only path** |
| **IB flags |= 0x08 (TC_WB_NOT_INVALIDATE)** | **24** | **Mesa byte-exact** |

## Calibration — what counts as "B.4 unblocked"

Per the existing rule (don't repeat Session 21's mistake):
1. `journalctl -k --since "Xmin ago" | grep -E "0066d000|66d000"` → no hits ✓ (achieved)
2. `out[0] == 0xDEADBEEF` (real readback, not just RC=0) ✗ (still 0xBAADF00D)
3. cl_probe canary still passes after the spike run ✓ (achieved both attempts)

Two of three. (2) is the remaining gate.
