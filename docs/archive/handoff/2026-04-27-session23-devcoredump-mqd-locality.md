# Session 23 — Devcoredump analysis: 0x66d000 lives in the kernel MQD/EOP region

**Date:** 2026-04-27
**Branch:** v3
**Hardware:** AMD Cezanne APU (Vega7 iGPU, GFX9/gfx90c, Renoir family)
**Kernel:** 6.18.24-1-lts, MEC fw 0x1e2

## What ran

- Cold-boot canary `cl_probe` → PASS (`out[0]=0xDEADBEEF`)
- `scripts/spike-ring-matrix.sh` SPIKE_RING=0 SPIKE_IP_INSTANCE=0
  - Submit returned RC=0; syncobj timed out at 5226 ms
  - `out[0] = 0xBAADF00D` (poison preserved — shader never wrote)
- Devcoredump captured: `docs/issues/2026-04-27-session23-devcd-r0-i0.bin` (6.1 MB)
- Post-fail canary `cl_probe` → **also timed out** on comp_1.2.0; GPU wedged after one MODE2 reset → forced reboot

## Original fault (dmesg, 11:47:37)

```
[gfxhub0] no-retry page fault (src_id:0 ring:40 vmid:0 pasid:0)
in page starting at address 0x000000000066d000 from IH client 0x1b (UTCL2)
VM_L2_PROTECTION_FAULT_STATUS:0x00040A50
Faulty UTCL2 client ID: CPC (0x5)
PERMISSION_FAULTS: 0x5  RW: 0x1  MAPPING_ERROR: 0x0  WALKER_ERROR: 0x0
```

Byte-identical to every prior session.

## What the devcoredump reveals

### 1. Kernel routed our submission to `comp_1.1.0`

Spike submitted `ip_type=COMPUTE, ip_instance=0, ring=0`. Kernel reported `ring comp_1.1.0 timeout, signaled seq=4, emitted seq=5`. The userspace ring number is **not** the physical MEC ring — the kernel scheduler picks among `comp_1.0.0..comp_1.3.1`.

### 2. MEC firmware never loaded our IB into the HQD

For `mec 1, pipe 1, queue 0` (= `comp_1.1.0`):

| Register | Value | Meaning |
|---|---|---|
| `CP_HQD_ACTIVE` | `0x00000001` | Queue still active |
| `CP_HQD_PQ_RPTR` | `0x00001800` | RPTR fully consumed PQ |
| `CP_HQD_PQ_WPTR_LO` | `0x00001800` | WPTR matches RPTR |
| `CP_HQD_IB_BASE_ADDR` | **`0x00000000`** | **MEC never picked up an IB** |
| `CP_HQD_IB_BASE_ADDR_HI` | `0x00000000` | |
| `CP_HQD_IB_RPTR` | `0x00000000` | Never started reading any IB |
| `CP_HQD_DEQUEUE_REQUEST` | `0x00000000` | No preempt requested |
| `CP_HQD_ERROR` | `0x00000000` | No HQD error logged |
| `CP_MEC_ME2_HEADER_DUMP` | 8× `0xc0000b00` | Only MEC scheduler-internal packets |

So the MEC scheduler walked over the PQ ring but never transferred control into a user IB. The fault happens **inside the MEC scheduler / dispatch path**, before user code runs.

### 3. `comp_1.1.0` PQ ring contents (the kernel's wrapper)

```
0x00 0xc0017900   PACKET3 type=3 count=2 opcode=0x79 (non-standard / scheduler-internal)
0x04 0x00000040
0x08 0xdeadbeef   ← literal 0xDEADBEEF written by the kernel as a fence/marker
0x0c 0xc0fb1000   PACKET3 NOP count=252 (ring padding)
0x10..0x3fc       0xffff1000 (ring init filler)
```

There is **no PACKET3_INDIRECT_BUFFER** (opcode 0x32 / 0x3F) referencing our IB. Either:
- the kernel didn't write the wrapper packet (our submit was rejected after the chunk-level handshake but before ring write), or
- the wrapper *is* the `0xc0017900` packet and it's a Cezanne-specific kernel-side scheduler op we don't recognize.

The literal `0xdeadbeef` at PQ[0x8] is suspicious — that's a sentinel the kernel writes when something goes wrong, not a value our code controls.

### 4. `0x66d000` is in the kernel's MQD/EOP region

KIQ ring contents show 8 `MAP_QUEUES` packets (`0xc005a200`, opcode 0xa2). Their MQD addresses are:

```
0x65e000, 0x65f000, 0x660000, 0x661000,
0x662000, 0x663000, 0x664000, 0x665000
```

Eight contiguous 4 KB pages — one MQD per KCQ. `comp_1.1.0`'s EOP base is at `0x65d000` (decoded from `mmCP_HQD_EOP_BASE_ADDR=0x65d0`, which is `addr >> 8`).

The fault address **`0x66d000` is exactly 8 pages above the last MQD page** (`0x665000 + 8*0x1000`). That places it inside the kernel's per-queue scratch / EOP / HOQ allocation that is read-only or unmapped to CPC's UTCL2 path under vmid 0.

Whatever the MEC scheduler is computing for "scratch / fence / preempt save area" is producing an address one queue's worth past where it should — and the page the firmware tries to write is not write-mapped for vmid 0.

## What got falsified this session

| Hypothesis | Falsified by |
|---|---|
| Fault is ring-specific (Session 22 priority 2) | Spike hit `comp_1.1.0`; post-spike canary hit `comp_1.2.0` and **also timed out**. Same fault on a different physical ring. |
| MODE2 reset always recovers | This time MODE2 succeeded but the very next compute submission also wedged. Required cold reboot. |
| `SPIKE_RING` / `SPIKE_IP_INSTANCE` env can target a specific physical ring | Kernel-side scheduler picks the comp ring; the env values are not honored as we assumed. |

## What is now confirmed

1. The 0x66d000 fault is a **CPC-firmware-level** address-computation issue — not user-PM4, not user-shader, not user IB content (the IB never even loads).
2. The fault is **post-MAP_QUEUES** but **pre-IB-dispatch** in the MEC scheduler state machine.
3. The fault address consistently lands in the kernel's per-queue scratch region adjacent to the MQD allocations.
4. The wedge **survives MODE2 reset** for at least one subsequent submission — so the trigger is something the spike does that corrupts kernel-MEC handshake state durably.

## Open question

**Why does Mesa's `cl_probe` succeed when our spike fails?** Both go through `amdgpu_cs_submit_raw2` ultimately. The differences must live in:
- IB content (PM4 byte stream) — we already know shader/PGM bytes differ but Session 17 falsified that as the trigger
- BO list — Mesa includes more BOs (kernel-arg buffer, scratch, fence)
- chunk types — does Mesa add a SYNCFILE chunk we don't have? a USER_FENCE chunk?
- ctx flags / context priority

This is the next investigation thread.

## Session 23 plan — status

| # | Plan item | Status |
|---|---|---|
| 1 | Read devcoredump | ✅ Done — `docs/issues/2026-04-27-session23-devcd-r0-i0.bin` |
| 2 | Ring sweep via SPIKE_RING | ❌ Falsified — kernel ignores user ring index for COMPUTE; only got 1 attempt before wedge |
| 3 | GFX ip_type with graphics-context preamble | Not yet attempted |
| 4 | UMQ bypass | Not yet attempted |

## Session 24 priorities (ordered)

1. **Strace `cl_probe` and our spike at the `DRM_IOCTL_AMDGPU_CS` boundary** — capture exact chunk byte streams for diff. Need to see the exact `drm_amdgpu_cs_chunk_ib`, `bo_list`, syncobj chunk payloads.
2. **Compare BO list contents** — Mesa's submit includes [shader, kernel-arg, scratch, output, fence]. Ours includes [ib, shader, stub, out]. Is the missing scratch/fence BO the trigger?
3. **Try `ip_type=AMDGPU_HW_IP_GFX`** to sidestep CPC entirely (Session 21 showed GFX path produced a different failure mode — `ring gfx timeout` *without* the UTCL2 fault, which means no kernel-state corruption).
4. **Capture a clean `AMD_DEBUG=ib` from Mesa** of an end-to-end OpenCL dispatch, including the kernel-managed PQ-ring write, for byte-exact comparison.

Until then, do **not** quote dispatch latency, do **not** treat Phase B.3 as done.
