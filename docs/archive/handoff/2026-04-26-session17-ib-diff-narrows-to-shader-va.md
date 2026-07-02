---
title: v3 Phase B — Session 17 (attempt 1): byte-exact IB diff narrows fault to shader-VA region
date: 2026-04-26
branch: v3
---

# v3 Phase B — Session 17: byte-exact IB diff narrows fault to shader-VA region

**Date:** 2026-04-26 (evening, post-reboot 2)
**Branch:** `v3`
**Working tree at session end:** uncommitted edit to
`deps/libdrm_store_spike.c` staging Session 17 attempt 2
(AMDGPU_VA_RANGE_HIGH on shader BO).
**Status:** B.3.e — Session 17 attempt 1 (trailing DMA_DATA terminator
+ IB hex dump) burned this boot's first reboot. Same
`0x000000000066d000` CPC RW=1 fault. **But** the IB hex dump finally
made byte-exact diffing possible, and it cuts the candidate set down
to a single hypothesis: shader-VA placement.

## QUICK START — Session 17 attempt 2 (post-reboot)

```bash
# 0. Canary — must print 0xDEADBEEF or hard power-cycle.
./build/shader/cl_probe

# 1. Build the staged change (already in working tree, uncommitted):
make build/libdrm_store_spike

# 2. Submit. Expect EITHER:
#    (a) out[0] = 0xDEADBEEF + readback PASS — shader at upper-canonical
#        VA cleared the CPC fault → root cause confirmed → commit.
#    (b) NEW fault address (anything other than 0x66d000) — partial
#        progress, the VA-half mattered but not entirely.
#    (c) Same 0x66d000 — Hypothesis D falsified, fall through to
#        USER_DATA / shader-ABI rewrite.
./build/libdrm_store_spike

# 3. Always check canary after a failure:
./build/shader/cl_probe       # If 0x00000000 → reboot before retry.
```

## Headline

The Session 17 attempt 1 IB hex dump diffed against
`build/strace/cl_probe_ib.log` byte-exact. **75 of 75 DWs are
identical to Mesa past the USER_DATA region** (offset 41+). The only
deltas are seven DWs in DW2 / DW25 / DW35-40, and they collapse to
two related root causes:

1. **Shader VA placement** (DW2 PGM_HI, DW25 PGM_LO): Mesa puts
   shader at upper-canonical `0xFFFF800000000000` (PGM_HI=0x80,
   PGM_LO=0). Ours is at user-canonical `0x100001000` (PGM_HI=0,
   PGM_LO=0x01000010).
2. **USER_DATA / shader ABI** (DW35-40): Mesa's shader reads a buffer
   descriptor in s[0..3] — sets USER_DATA_0 only, leaves s1
   carrying prior state. Ours uses raw 64-bit pointer in s[0..1] +
   stub V# in s[2..3] — sets all four USER_DATA registers.

The CPC fault is `RW=1` (write) at `0x66d000` with `PERMISSION_FAULTS=0x5`
(RANGE + WRITE_PROTECT). USER_DATA_* values are shader-read inputs —
they cannot directly cause a CPC write fault. **The CPC's own writes
(CSA / wave-init / scratch save) are derived from shader-VA-region
registers**, so Hypothesis D (upper-canonical shader VA) is now the
highest-probability remaining cause, not the lowest as Session 16
playbook had it.

## Byte-exact IB diff

Captured IB hex dump (75 DWs):

```
0000: c0017600 0000020d 00000000 c0027600 00000216 ffffffff 00000000 c0027600
0008: 00000219 00000000 00000000 c0017900 0000007b 00000000 c0027900 00000380
0010: 01004400 00000080 c0033700 00100500 00600300 ffff8001 00000000 c0017600
0018: 0000020c 01000010 c0027600 00000212 002c0040 00000008 c0017600 00000218
0020: 00000100 c0027600 00000242 00003000 00000001 c0027600 00000240 00002000
0028: 00000001 c0055802 a8c40000 ffffffff 00ffffff 00000000 00000000 0000000a
0030: c0055000 60200000 00000000 ffff8000 00000000 ffff8000 80000060 c0017600
0038: 00000215 00000140 c0037600 00000207 00000001 00000001 00000001 c0031502
0040: 00000001 00000001 00000001 00000045 c0055000 e0300000 00000000 00000000
0048: 00000000 00000000 00000000
```

Mesa reference (74 DWs, reconstructed from `build/strace/cl_probe_ib.log`
lines 1-188):

```
DW 0..73 — same first 40 DWs UP TO USER_DATA region differ as below;
DW 40..73 of Mesa = DW 41..74 of ours, byte-exact identical.
```

| Mesa DW | Ours DW | Field                              | Mesa value     | Ours value     | Notes |
|---------|---------|------------------------------------|----------------|----------------|-------|
| 0       | 0       | SET_SH_REG header (count=1)        | `c0017600`     | `c0017600`     | ✓ |
| 1       | 1       | reg-base = 0x20d (PGM_HI)          | `0000020d`     | `0000020d`     | ✓ |
| **2**   | **2**   | **COMPUTE_PGM_HI**                 | **`00000080`** | **`00000000`** | shader-VA-half delta |
| 3-22    | 3-22    | static-thread-mgmt, BC, WRITE_DATA | identical      | identical      | ✓ |
| 23      | 23      | SET_SH_REG header                  | `c0017600`     | `c0017600`     | ✓ |
| 24      | 24      | reg-base = 0x20c (PGM_LO)          | `0000020c`     | `0000020c`     | ✓ |
| **25**  | **25**  | **COMPUTE_PGM_LO**                 | **`00000000`** | **`01000010`** | shader-VA-half delta |
| 26-34   | 26-34   | RSRC1/RSRC2, TMPRING, USER_DATA_2 hdr | identical    | identical      | ✓ |
| **35**  | **35**  | **USER_DATA_2**                    | **`00200000`** | **`00003000`** | ABI delta |
| **36**  | **36**  | **USER_DATA_3**                    | **`00000000`** | **`00000001`** | ABI delta |
| **37**  | **37**  | **SET_SH_REG header for 0x240**    | **`c0017600`** count=1 | **`c0027600`** count=2 | ABI delta |
| 38      | 38      | reg-base = 0x240 (USER_DATA_0)     | `00000240`     | `00000240`     | ✓ |
| **39**  | **39**  | **USER_DATA_0**                    | **`00200040`** | **`00002000`** | ABI delta |
| —       | **40**  | **(extra DW: USER_DATA_1)**        | absent         | **`00000001`** | ABI delta — extra DW shifts our IB +1 from here on |
| 40-73   | 41-74   | ACQUIRE_MEM through trailing DMA_DATA | identical (with +1 shift) | identical (with +1 shift) | ✓ |

**Past offset 40, every DW lines up byte-exact** — including the
trailing DMA_DATA terminator we just added in attempt 1. So the
remaining gap is only the 7 DWs above.

## Why shader-VA placement is now the prime suspect

CPC's role at dispatch time:

- **Reads** shader bytes via `(PGM_HI<<40)|(PGM_LO<<8)`. A read fault
  here would manifest as `RW=0`, not the `RW=1` we're seeing.
- **Writes** the wave's CSA (Context Save Area), scratch ring base,
  and the wave's TGID/TIDIG initialization. CSA is the only one that
  goes to GPU VA (the others are on-chip SGPR file).

Per amdgpu kernel sources (`amdgpu_csa.c`), the CSA for gfx9 lives at
a per-process GPU VA inside `[AMDGPU_VA_RESERVED_BOTTOM,
AMDGPU_VA_RESERVED_TOP]` — roughly the low MB region. **Address
`0x66d000` fits squarely in that reserved range.** If the CSA
mapping is RO (kernel maps it write-protected for some submissions
and RW for others), CPC's CSA write would land exactly the
`PERMISSION_FAULTS=0x5` (RANGE + WRITE_PROTECT) signature we observe.

The gating mechanism: when the shader is in lower-canonical VA, MEC
firmware may take a **CSA-based wave-save path** (write-then-launch).
When the shader is in upper-canonical VA (Mesa's choice), MEC may
take a **direct-launch path** that bypasses CSA write. This would
cleanly explain why every prior register-level fix failed to change
the fault address — CPC's write target is gated on a single bit (47)
of PGM_HI/LO.

## Session 17 attempt 2 — staged change

Single edit to `deps/libdrm_store_spike.c`:

```c
#define ALLOC_BO(name, va_flags) do {
    ...
    r = amdgpu_va_range_alloc(dev, amdgpu_gpu_va_range_general, 4096, 4096, 0,
                              &name##_va, &name##_vah, (va_flags));
    ...
} while (0)

ALLOC_BO(ib,     0);
ALLOC_BO(shader, AMDGPU_VA_RANGE_HIGH);   /* ← this line; everything else unchanged */
ALLOC_BO(out,    0);
ALLOC_BO(stub,   0);
```

`AMDGPU_VA_RANGE_HIGH = 0x2` is defined in `/usr/include/libdrm/amdgpu.h`
and is the standard libdrm flag to request VA from the upper-canonical
half. After this change:

- shader BO will be allocated at some VA in `[0xFFFF800000000000,
  0xFFFFFFFFFFFFFFFF]`
- our `pgm_hi = (shader_va >> 40) & 0xFF` will yield `0x80`+ (matching
  Mesa's PGM_HI)
- our `pgm_lo = (shader_va >> 8) & 0xFFFFFFFF` will be derived from
  the new high VA — won't be exactly 0 like Mesa, but will land
  shader bytes in the upper-canonical half

The other 3 BOs (IB, out, stub) keep low-canonical VA — that's
known-working (Session 11+ canary), and changing them all at once
would muddy the signal.

Build: clean (`make build/libdrm_store_spike` — single `cc` line, no
warnings beyond pre-existing clangd `errno.h` unused-include note).

## Cleanliness baseline

No Cyrius source touched this session — the spike is C-only. Smoke /
lint / fmt / 621 tests still match the 5.7.12 baseline from
Session 16.

## Don't lose

- **PACKET3 count_minus_1 fix is necessary and correct** — Session 14.
- **TA_CS_BC_BASE_ADDR Mesa-byte-exact value is correct** even though
  it didn't fix the fault — Session 16.
- **Trailing DMA_DATA terminator is Mesa-byte-exact and stays** —
  Session 17 attempt 1.
- **NUM_THREAD=1,1,1 + RESOURCE_LIMITS=0x140 are already emitted** —
  the Session 16 playbook claimed they were missing, that was wrong.
- **The IB diff past offset 40 is now byte-exact identical to Mesa**
  — confirmed in this session. No more PM4 register hunting.

## Predictions for attempt 2

- **PASS path:** `out[0] = 0xDEADBEEF`. Confirms CPC's CSA write
  target is gated on PGM_HI bit 47. We commit `AMDGPU_VA_RANGE_HIGH`
  for the shader BO and proceed to port the fix into
  `src/backend_native.cyr` for the pure-Cyrius dispatch path.
- **NEW-FAULT-ADDR path:** something other than `0x66d000` — partial
  progress. PGM_HI/LO matters, but USER_DATA / shader-ABI also feeds
  the derivation. Document the new address and either keep digging
  or take the USER_DATA-rewrite path next.
- **SAME-FAULT path:** `0x66d000` again. Hypothesis D falsified.
  Falls through to USER_DATA-Mesa-byte-exact: rewrite the spike's
  shader to use a buffer descriptor in s[0..3] instead of a raw
  64-bit pointer — much larger change (new shader bytes, new V#
  encoding) but the only remaining IB delta.

## Files changed this session

- `deps/libdrm_store_spike.c`:
  - `ALLOC_BO` macro takes a new `va_flags` parameter.
  - `shader` BO call site uses `AMDGPU_VA_RANGE_HIGH`; others use 0.
  - 11-line comment block on the shader call explains the hypothesis.

No other files touched. `dist/mabda.cyr` not regenerated (no Cyrius
API change).

## Files unchanged this session

- `src/backend_native.cyr`, `programs/native_compute_spike.cyr`,
  `programs/native_compute_store.cyr` — Cyrius port still pending C
  reference passing first.
- `cyrius.cyml` — toolchain pin still 5.7.12 from Session 16.
- `dist/mabda.cyr` — no API change.
