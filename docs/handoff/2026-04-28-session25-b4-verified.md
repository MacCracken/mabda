# Session 25 — Phase B.4 store shader live-verified

**Date:** 2026-04-28 (closes work that started 2026-04-27 evening as Session 25)
**Branch:** `v3`
**Hardware:** AMD Cezanne APU (Vega7 iGPU, GFX9 / `gfx90c`, Renoir family)
**Kernel:** 6.18.24-1-lts, MEC fw `0x1e2`
**Reverses:** `2026-04-23-session9-tdr-false-positive.md` retraction
**Closes Session 24 open blocker:** `2026-04-27-session24-fence-chunk-eliminates-66d000.md`

## Headline

`programs/native_compute_store.cyr` now lands `0xDEADBEEF` in
`out[0]` from a pure-Cyrius compute dispatch on Cezanne. Submit-to-
syncobj signal is 0 ms (not the 10 s TDR shape). The post-dispatch
`0xC0FFEE12` `WRITE_DATA` marker proves the CP returned cleanly from
`DISPATCH_DIRECT` and continued through the rest of the IB. cl_probe
canary stays green afterward.

```
post-dispatch marker = 0xC0FFEE12 OK (CP cleared dispatch)
output[0] = 0xDEADBEEF (want 0xDEADBEEF)
sentinel intact at [1]
OK — GPU wrote 0xDEADBEEF via pure-Cyrius dispatch
```

All three B.4 calibration gates from `docs/handoff/2026-04-27-session24-...md`
are now met:

| Gate | Status |
|------|--------|
| no `0x66d000` UTCL2 fault in dmesg | ✓ (Session 24 fence-chunk fix held) |
| `out[0] == 0xDEADBEEF` from real CPU readback | ✓ |
| cl_probe canary still passes after the spike run | ✓ |

## What ran

| Session | Path | Result | Note |
|---------|------|--------|------|
| 25  (C spike) | `libdrm_store_spike_v3` with `BC_BASE = 0x01004400 / 0x80` | timeout 5 s, `out[0] = 0xBAADF00D` | Session-16 BC-magic hypothesis |
| 25b (C spike) | `libdrm_store_spike_v3` with `BC_BASE = 0 / 0` + 3 progress markers | **PASS**, `out[0] = 0xDEADBEEF`, 0 ms | falsifies the BC-magic hypothesis |
| 25b (Cyrius)  | `native_compute_store` mirroring 25b C spike | **PASS**, `out[0] = 0xDEADBEEF`, 0 ms | first pure-Cyrius dispatch |
| 25c | devcoredump capture during exploratory C spike | binary artifact only | `docs/issues/session25c/devcd.bin` |

Logs: `docs/issues/session25/{spike,canary,dmesg}.log` (failed BC-magic),
`docs/issues/session25b/{spike,canary,dmesg}.log` (passed),
`docs/issues/session25c/devcd.bin` (binary devcoredump capture).

## What changed in the Cyrius path between Session 24 and now

The Session 24 spike was already 4-chunk Mesa-shape with a FENCE chunk
and produced "submits cleanly, IB never executes" — the open blocker.
Session 25b found six independent bugs that all had to be fixed for
the Cyrius native compute path to match the working C spike byte for
byte. The diff is `1437fa2..273bb5c` on `programs/native_compute_store.cyr`
and `src/backend_native.cyr`.

### 1. `native_pm4_nop` count_minus_1 off-by-one (`src/backend_native.cyr:580`)

`pad_dwords` was being passed directly to `native_pm4_pkt3_header`, which
internally subtracts one to encode the `count_minus_1` field. The header
on the wire therefore claimed `pad_dwords` body dwords (one too many);
CP read one dword past the IB end and either hung waiting for the
missing dword or processed garbage. This silently bricked every Cyrius
native compute submit — the C spike was unaffected because it doesn't
emit NOP padding.

```
-    if (pad_dwords < 1) { return pos; }
-    store32(buf + pos, native_pm4_pkt3_header(IT_NOP, pad_dwords, 0));
+    if (pad_dwords < 2) { return pos; }
+    store32(buf + pos, native_pm4_pkt3_header(IT_NOP, pad_dwords - 1, 0));
```

This is a fresh instance of the same `count_minus_1` convention bug
that the `feedback_pm4_count_minus_1_naming` memory was filed against
(direct-PM4 packet counts must match `AMD_DEBUG=ib` byte-for-byte —
off-by-one masked itself for 7 sessions until conventions mixed).
Adding it again here, in `native_pm4_nop`, means the rule still hasn't
fully landed in the codebase.

### 2. PGM_LO/HI encoding (`programs/native_compute_store.cyr:213`)

GFX9 reconstructs the shader address as `addr = (PGM_HI << 40) | (PGM_LO << 8)`,
i.e. the registers carry bits `[39:8]` and `[47:40]` of the VA. Old
code put bits `[31:0]` in PGM_LO and `[47:40]` in PGM_HI:

```
-    var pgm_lo = shader_va & 0xFFFFFFFF;
-    var pgm_hi = (shader_va / 0x10000000000) & 0xFF;
+    var pgm_lo = (shader_va >> 8) & 0xFFFFFFFF;
+    var pgm_hi = (shader_va >> 40) & 0xFF;
```

For `shader_va = 0xFFFF800100000000` the old code yielded `PGM_LO = 0`,
`PGM_HI = 0x80`, i.e. HW fetched from `(0x80 << 40)` = `0x0000800000000000` —
totally wrong, wave fails launch, queue eventually TDRs. This is the
same bug the C spike fixed in Session 13. The `feedback_cyrius_signed_div_high_va`
memory ("never use `va / 2^N` to extract bits from a canonical-high VA in
Cyrius — round-toward-zero gives wrong high-half encoding") covers the
class; this commit converts the remaining offenders to `>> N` shifts.

### 3. Stub VA must be canonical-high (`programs/native_compute_store.cyr:112`)

USER_DATA_2/3 carries Mesa's scratch V# stub VA. Old code mapped the
stub at `0x200000` (user-low aperture) on the untested hypothesis that
compute queues accept both halves. Session 25b's post-dispatch
`WRITE_DATA` marker test showed user-low writes from the compute queue
silently fail — the marker never landed. Fix: map stub at
`0xFFFF800100004000`, mirror the C spike layout, encode USER_DATA_2/3
as a 64-bit pointer split low/high:

```
-    var stub_va = 0x200000;
+    var stub_va = 0xFFFF800100004000;
...
-    store32(&user23 + 0, 0x00200000);
-    store32(&user23 + 4, 0);
+    store32(&user23 + 0, stub_va & 0xFFFFFFFF);
+    store32(&user23 + 4, (stub_va >> 32) & 0xFFFFFFFF);
```

### 4. `TA_CS_BC_BASE_ADDR = 0` (`programs/native_compute_store.cyr:236`)

Session 16 had hypothesised that `BC_BASE = shader_va` was load-bearing
because the CPC walks BC_BASE during dispatch setup. Session 25b's
C-side test pair (BC_BASE = `0x01004400 / 0x80` from Mesa's IB dump
vs. BC_BASE = `0 / 0`) showed the latter passes — falsifying both
the Mesa-magic-VA value (Session 16) and the older shader-VA value.
The 25 / 25b A/B is the single cleanest evidence we have on this.

```
-    var bc_va = shader_va;
+    var bc_va = 0;
     var bc_pair[8];
-    store32(&bc_pair + 0, (bc_va / 256) & 0xFFFFFFFF);
-    store32(&bc_pair + 4, (bc_va / 0x10000000000) & 0xFF);
+    store32(&bc_pair + 0, (bc_va >> 8) & 0xFFFFFFFF);
+    store32(&bc_pair + 4, (bc_va >> 40) & 0xFF);
```

### 5. New 4-chunk submit helper (`src/backend_native.cyr:929`)

`native_cs_submit_4chunk` adds the `AMDGPU_CHUNK_ID_FENCE` chunk
(`fence_kh`, `fence_offset = 32`) to the BO_HANDLES + SYNCOBJ_OUT + IB
shape that `native_cs_submit_inline_bos` already had. The 3-chunk
variant accepts the submit ioctl but the IB never reaches MEC for
compute on this kernel — TDR fires after 10 s and the syncobj signals
as if the IB completed (the original Session 9 false-positive shape).
Per Session 24's analysis: without an explicit FENCE chunk, the kernel
synthesises a fallback target VA in the per-queue scratch / EOP / HOQ
region, which on this kernel is adjacent to the MQD allocations — same
root cause of Session 23's `0x66d000` fault.

The new path also passes `ib_flags = 0x08`
(`AMDGPU_IB_FLAG_TC_WB_NOT_INVALIDATE`), which Mesa rusticl uses for
compute submits and which we'd previously left unset.

### 6. Inline BO list with `0xFFFFFFFF` sentinels (`programs/native_compute_store.cyr:380`)

Replaced `BO_LIST_OP_CREATE` (which allocates a persistent BO_LIST in
the kernel) with the inline BO_HANDLES chunk Mesa rusticl uses:

```
+    store32(&bo_list_in +  0, 0xFFFFFFFF);  # operation sentinel
+    store32(&bo_list_in +  4, 0xFFFFFFFF);  # list_handle sentinel
+    store32(&bo_list_in +  8, 5);            # bo_number
```

The `0xFFFFFFFF` values signal the kernel to use inline residency only,
matching the byte-exact `cl_probe` ioctl payloads captured under
`docs/issues/session24/cl_probe.csdump`.

### 7. Page-permission widening on all BOs

`AMDGPU_VM_PAGE_READABLE | AMDGPU_VM_PAGE_WRITEABLE | AMDGPU_VM_PAGE_EXECUTABLE`
applied uniformly to shader, stub, output, IB, fence. Previous code
varied (RX on shader, RW on out/stub, R on IB). Session 25b found that
narrower perms produced silent dispatch failures on the compute path.

This is overly permissive and should tighten back to per-BO minimums
once we have a regression test that catches narrowing — flagged as
follow-up.

### 8. Diagnostics retained for future sessions

- `mono_ns()` clock + `submit-to-syncobj-signal: <ms>` print —
  differentiates real completion (0 ms) from the TDR false-positive
  (≥ 9000 ms).
- `0xC0FFEE12` post-dispatch `WRITE_DATA(WR_CONFIRM=1)` marker into
  `stub_va + 4` — proves CP returned from `DISPATCH_DIRECT`.
- IB hex dump (8 dws/line) — diff target against `AMD_DEBUG=ib` and
  the C spike's `spike.log`.

These are useful enough during bring-up that they're worth keeping in
the program until the native backend has a wider regression suite to
catch the same failure modes.

## Falsified hypotheses

| Hypothesis | Falsified by |
|------------|-------------|
| `BC_BASE = shader_va` is load-bearing (Session 16) | 25b: works with `BC_BASE = 0` |
| `BC_BASE = 0x01004400 / 0x80` (Mesa magic VA) is load-bearing | 25 vs 25b: 25 used the magic VA and timed out, 25b used 0 and passed |
| Compute queues accept both VA halves | 25b: user-low USER_DATA + low-VA stub fail silently |
| The Session-23 `0x66d000` fault implied a CPC walks BC base regardless | 25b: BC = 0 still works |

## Verified-necessary fixes (locked in for v3 going forward)

Cumulative list — each cited by the session that proved it.

| Fix | Session | Why |
|-----|---------|-----|
| `PACKET3 count_minus_1 = body_dws - 1` | 14 | off-by-one desyncs IB |
| Trailing `DMA_DATA CP_SYNC=1 BYTE_COUNT=0` | 17 | Mesa-byte-exact end of IB |
| Shader BO at `AMDGPU_VA_RANGE_HIGH` | 17 | matches PGM_HI = 0x80 placement |
| `amdgpu_cs_submit_raw2` + syncobj | 19/20 | modern submit path |
| 4-chunk Mesa-shape submit (with FENCE) | 24 | removes the `0x66d000` fault |
| `BO_HANDLES.operation/list_handle = 0xFFFFFFFF` | 24 | inline-only path |
| `IB flags |= 0x08` (TC_WB_NOT_INVALIDATE) | 24 | Mesa byte-exact |
| `native_pm4_nop` count_minus_1 off-by-one | **25b** | NOP padding overran IB end |
| PGM_LO/HI = `(va >> 8 / >> 40)`, not `va & ... / va / ...` | **25b** | wrong VA → wave fails launch |
| All BOs at canonical-high VA | **25b** | user-low compute writes fail silently |
| `BC_BASE = 0` | **25b** | Session-16 magic-VA hypothesis falsified |
| All BOs `R | W | X` | **25b** | narrower perms cause silent failures (re-tighten later) |
| `WRITE_DATA(WR_CONFIRM=1)` post-dispatch marker | **25b** | proves CP returned from `DISPATCH_DIRECT` |

## What B.4 closing means

`native_compute_store` proves the *dispatch path*: device → context →
buffer → IB → submit → wait → readback. It does not yet prove:

- **Compute pipeline abstraction.** `native_compute_store` is one
  monolithic 470-line program, not a `Backend.compute_dispatch(…)` call.
- **Multiple dispatches per submit.** Single-shot only.
- **Timing / profiler integration.** The `submit-to-syncobj-signal`
  print is hand-rolled, not threaded through `src/profiler.cyr`.
- **The other three integration shapes**: `phase0` (buffer/texture
  smoke), `render_e2e` (render pass clear + pixel verify),
  `render_graph_e2e` (3-node DAG). All four are required by the v3.0
  exit criteria.

So B.4 is verified, but the v3.0 work is just opening up — the
backend abstraction layer (ADR 006) is now the natural next step.

## Codebase state at closeout

- `programs/native_compute_store.cyr` — 470 lines, lint-clean,
  passing on `gfx90c`.
- `src/backend_native.cyr` — `native_pm4_nop` corrected,
  `native_cs_submit_4chunk` added.
- `cyrius.cyml` — toolchain pin `5.7.23` (was `5.6.13` at Session 24,
  bumped during Session 25 work).
- `cyrius lint` — 0 warnings on touched files.
- 387 CPU assertions still pass; backend_native tests untouched.
- `dist/mabda.cyr` not yet regenerated — pending the next
  closeout pass since `src/` changed.

## Follow-ups

1. **Tighten BO page perms** — re-narrow shader to RX, IB to R, etc.
   once there's a regression test that catches the narrowing failure
   mode.
2. **Regenerate `dist/mabda.cyr`** before any consumer-facing release.
3. **Bench harness** — capture submit-to-completion timing for
   `native_compute_store` so future regressions are visible.
4. **Backend abstraction (ADR 006)** — start the design doc; the
   `native_compute_store` flow is the reference implementation for
   the `Backend.compute_dispatch` entry point.
