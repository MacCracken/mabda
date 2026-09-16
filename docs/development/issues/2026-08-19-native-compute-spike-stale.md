# `native_compute_spike`: two defects since its first commit, and a marker that could not see a hang

*Filed as: "`native_compute_spike` is bit-rotted and superseded — retire it or repair it".
That title was the filing's hypothesis, and it was wrong. See
[What the filing got wrong](#what-the-filing-got-wrong).*

**Status:** ✅ **RESOLVED in 4.1.3** (2026-09-16). The spike is repaired and is no longer in
the Makefile's `NATIVE_KNOWN_FAIL`, so `make test-native-all` runs it like any other gate.
⚠ **Caveat:** every run of the repaired spike happened *after* that day's two MODE2 GPU
resets (12:26:32 and 12:32:19, both caused by reproducing defect 1 below). Those resets
changed how the GPU keeps context-register state (see the render/sampler note below). A
clean-boot confirmation run of `make test-native-compute-spike` is part of the 4.1.3
closeout.
**Placement:** `programs/native_compute_spike.cyr`, `Makefile:test-native-compute-spike`.
**Discovered:** 2026-08-19, by `make test-native-all` on its first run (v4.0.11 work, shipped
as 4.1.0).
**Resolved:** 2026-09-16, on Cezanne (gfx90c), kernel 7.2.3.
**Severity:** **Low** for consumers, because the program is not shipped code. Its effect on
*diagnosis* was larger: the MODE2 resets it caused hid a real render bug (see below).
**Affects:** the in-tree spike program. The library's compute path was not affected by these
two defects.

## Symptom (as filed, 2026-08-19)

```
$ make test-native-compute-spike
HW_IP_INFO GFX: rc=0 available_rings=0x1
HW_IP_INFO COMPUTE: rc=0 available_rings=0xf
submitted to COMPUTE ring
dispatch completed (sync-obj signaled) in 2206 ms
stub[0] = 0x0  (FAIL — want 0xCAFEBABE)
```

Reproducible, twice in a row, on Cezanne (gfx90c): 2015 ms and 2206 ms.

The program failed with two different kernel signatures on two dates. They are separate
observations, and only the second was reproduced.

**Observation A, the filed runs (2026-08-19, kernel 7.1.8).** The boot's kernel log
(boot `8f7869649f1741b58bcbdd08cdb2946d`, `journalctl -b <id> -k`) names the process
`native_compute_` in four GPU events between 17:30 and 17:50:

```
17:38:12  Illegal opcode in command stream; ring comp_1.3.0 timeout; ring reset failed;
          GPU reset begin … MODE2 reset … GPU reset(1) succeeded
17:39:58  ring comp_1.3.0 timeout; Starting comp_1.3.0 ring reset; ring reset succeeded
17:40:08  ring comp_1.3.0 timeout; Starting comp_1.3.0 ring reset; ring reset succeeded
17:41:05  ring comp_1.3.0 timeout; Starting comp_1.3.0 ring reset; ring reset succeeded
```

The filed runs (2015 ms and 2206 ms, `stub[0] = 0`) fit the three later events: ~2 s **ring
timeouts whose ring reset succeeded, with no "Illegal opcode" line**, all after the 17:38:12
MODE2 reset. That signature is **observed but not reproduced**: nothing on 2026-09-16 produced
"~2 s + `stub[0] = 0`". The likeliest explanation is the post-reset GPU state (see the
render note below: a MODE2 reset changed how this GPU keeps state). Of the two defects, only
defect 1 (a CP that never runs the IB) is consistent with `stub[0] = 0`; defect 2 alone lets
the marker land (ablation A5/B1). It was not re-run on hardware to confirm: provoking it
takes a hang and a reset.

**Observation B, the 2026-09-16 repro (kernel 7.2.3, program unchanged, no earlier reset on
that boot):**

```
dispatch completed (sync-obj signaled) in 198 ms
stub[0] = 0x0  (FAIL — want 0xCAFEBABE)          → exit 9
kernel: [drm:gfx_v9_0_bad_op_irq] *ERROR* Illegal opcode in command stream
kernel: ring comp_1.2.0 timeout … Starting comp_1.2.0 ring reset
kernel: fail to wait on hqd deactive … Ring comp_1.2.0 reset failed
kernel: GPU reset begin! … MODE2 reset … GPU reset(1) succeeded!
```

This is the same signature as the first 2026-08-19 event (17:38:12), and ablation A4 below
reproduces it by putting back only defect 1.

## Root cause: two defects, both from the program's first day

The READABLE-only IB mapping and the signed-`/` `PGM_HI` split are in `2ee06d3`
(2026-04-21 19:48), the commit that added the program. `PGM_LO` started out correct there,
as `(shader_va / 256) & 0xFFFFFFFF`, which yields `0x01000000` for this VA; `00ad544`
("more repair for wedge", 2026-04-21 22:55) replaced it with `shader_va & 0xFFFFFFFF`,
dropping the `>> 8`. None of the three lines changed again until 4.1.3
(`git log -p -- programs/native_compute_spike.cyr`). This was never a working program that
decayed. No run on record is an honest pass: the one Phase B.3.d "pass" was retracted in
Session 9 as a TDR false positive.

### 1. The IB was mapped READABLE-only

```
rc = native_gem_va_map(fd, ib_handle, ib_va, ib_size, AMDGPU_VM_PAGE_READABLE);
```

On Cezanne the CP's fetch of an IB is gated by the execute bit. Step 4f.ii's hardware bisect
(kernel 6.18.24) established this and gave the library `_NATIVE_PERM_IB` (R | X), but the
spike never adopted it. With a READABLE-only IB the CP never runs the stream. On 2026-09-16
(no earlier reset on that boot) `stub[0]` stayed 0, the kernel logged "Illegal opcode in
command stream", the ring reset failed, and the GPU took a **MODE2 reset** within ~200 ms:
observation B, and ablation A4. The filed 2026-08-19 runs (observation A) also read
`stub[0] = 0`, but as ~2 s ring timeouts that recovered with a ring reset.

### 2. `COMPUTE_PGM_HI` was split with signed `/`, and `COMPUTE_PGM_LO` without `>> 8`

```
var pgm_lo = shader_va & 0xFFFFFFFF;                  # should be (va >> 8)  & 0xFFFFFFFF
var pgm_hi = (shader_va / 0x10000000000) & 0xFF;      # should be (va >> 40) & 0xFF
```

GFX9 fetches the shader from `(PGM_HI << 40) | (PGM_LO << 8)`. The shader VA
`0xFFFF_8001_0000_0000` is negative as an i64, and Cyrius `/` is signed division that truncates
toward zero, so `va / 2^40` gives `PGM_HI = 0x81`, not `0x80`. `PGM_LO` is a separate
mistake: without the `>> 8` it takes the VA's low 32 bits, `0`, instead of `0x01000000`.
Together they launched the wave at an unmapped address and it **hung**. `native_compute_store` hit the same
encoding bug and fixed its own copy in Session 25b, but the spike was never updated.

`TA_CS_BC_BASE_ADDR` used the same signed split (`_HI = 0x81`). The hardware tolerated that
value (ablation A6 below), and the repair switched it to `>>` anyway, so the program sets the
VA its comment describes.

## Why the WRITE_DATA marker could not detect the hang

The spike's proof of execution was a CP-side `WRITE_DATA` that puts `0xCAFEBABE` in `stub[0]`.
It proves the CP fetched and ran the IB, so it catches defect 1. It cannot catch defect 2:

- **The marker runs before `DISPATCH_DIRECT`**, so it lands before the hang.
- **A marker after `DISPATCH_DIRECT` lands too.** The ablation harness also wrote `0xC0FFEE12`
  after the dispatch packet. With the bad PGM encoding, both markers landed (A5, A5b, A8): the
  CP moves past `DISPATCH_DIRECT` while the wave is still hung.
- **The user fence gets written too** (`0x1` in A5/A5b, on the 4-chunk FENCE submit).
- **The syncobj signals with no error.** On kernel 7.2.3 the compute job times out after ~2 s,
  the ring reset *succeeds*, and the job is completed. Exported to a sync_file, the syncobj
  reports status 1 from `SYNC_IOC_FILE_INFO`: signalled, no error (A8).

So after only defect 1 was fixed, the original program reported **`OK`, exit 0, in 2022 ms**
(B1). A hung dispatch looked like a pass with a slow signal.

What does tell a hung dispatch from a finished one is the context's reset state:
`AMDGPU_CTX_OP_QUERY_STATE2` returns `out.state.flags = 0x1` (RESET) after the ring-reset
completion (A5b, A8, NC1) and `0x0` after a clean dispatch (A0, A6, A7, every repaired run).

## The repair (4.1.3)

- The IB is mapped with `_NATIVE_PERM_IB` (READABLE | EXECUTABLE).
- `PGM_LO` / `PGM_HI` and `TA_CS_BC_BASE_ADDR` are split with `>> 8` / `>> 40`.
- **New post-dispatch reset check:** after the wait the program sleeps 100 ms (the kernel
  signals the job and records the reset in the same timeout path, and the program does not
  depend on their order), then issues `AMDGPU_CTX_OP_QUERY_STATE2`. A failed query or any
  non-zero flag fails the gate with **exit 10**. Flags are printed only when the query
  succeeds. On a failed ioctl the union still holds the request, whose `op = 4` would read as
  the GUILTY bit.
- **The GFX-ring retry is gone.** A compute canary that quietly passes on another ring hides a
  compute-submit regression, and a `DISPATCH_DIRECT` stream on the GFX ring can hang the ring
  that drives the display. A COMPUTE submit failure is exit 7.
- **Stale comments are corrected.** The 256-DW pad is a layout choice, not a kernel
  `align_mask` requirement: a 66-DW IB behaves the same.
- `NATIVE_KNOWN_FAIL` is empty.

The spike still submits with `native_cs_submit_inline_bos` (3 chunks, no FENCE). It is the
only hardware caller of that path.

## Evidence (2026-09-16, Cezanne, kernel 7.2.3)

**Ablation.** A scratch harness started from a baseline carrying the Session-25b submit
contract: 4-chunk FENCE submit, canonical-high stub, `BC_BASE = 0`, a 66-DW IB, a pre-dispatch
`0xCAFEBABE` marker, and a post-dispatch `0xC0FFEE12` marker. Each case put back **one**
element of the original spike. Two more cases (B1, B2) patched the original program directly.

| Case | What was put back / changed | Signal | Markers | QUERY_STATE2 flags | Kernel | Result |
|---|---|---|---|---|---|---|
| A0 | baseline | 0 ms | both | 0x0 | none | pass |
| A1 | 256-DW pad | 0 ms | both | — | none | pass |
| A2 | pre-dispatch marker → user-low BO @0x200000 | 0 ms | both | — | none | pass |
| A3 | 3-chunk submit (no FENCE, IB flags 0) | 0 ms | both | — | none | pass |
| A4 | IB READABLE-only | 194 ms | none | — | illegal opcode, ring reset failed, **MODE2** | fail |
| A5 | signed-`/` PGM encoding | 2012 ms | both | — | ring timeout, ring reset OK | **false pass** |
| A5b | A5, with the reset query | 2064 ms | both | **0x1** | ring timeout, ring reset OK | caught |
| A6 | signed-`/` `BC_BASE` | 0 ms | both | 0x0 | none | pass |
| A7 | user-low stub @0x200000 (the spike's layout) | 0 ms | both | 0x0 | none | pass |
| A8 | signed-`/` PGM + 3-chunk submit | 2055 ms | both | **0x1** | ring timeout, ring reset OK | caught |
| B1 | original program + IB R\|X only | 2022 ms | `0xCAFEBABE` | — | ring timeout, ring reset OK | **program printed OK** |
| B2 | original program + IB R\|X + `>>` PGM | 0 ms | `0xCAFEBABE` | — | none | pass |

("—" means that build did not issue the query.) A4 isolates defect 1 and A5/A5b isolate defect
2. B2 shows that fixing just those two makes the original program pass. Every other element of
the original sequence passes on its own: the low stub, the low marker target, the `BC_BASE`
value, the 3-chunk submit, and the 256-DW pad.

**Repaired spike:**

- First run: 0 ms, flags 0x0, `0xCAFEBABE`, exit 0, no kernel lines.
- Five consecutive runs: all 0 ms, flags 0x0, `0xCAFEBABE`, no kernel lines.
- An independent re-run from a separate tree: 1 ms, flags 0x0, exit 0.
- `make test-native-all`: 71 passed, 0 failed, 2 skipped (DRM master), 0 known-fail. No kernel
  lines.

**Negative control (NC1).** This was the repaired spike with only the PGM bug put back. It is
deliberately a dispatch that the kernel's ring reset recovers from, not one that needs a full
GPU reset: 2023 ms, `stub[0] = 0xCAFEBABE`, flags **0x1**, **exit 10**, then "Ring comp_1.2.0
reset succeeded". The reset check fails the run that B1 shows the old program passing.

## What the filing got wrong

- **"Bit-rotted" / "superseded".** Neither. Both defects date from the first commit, and
  `native_compute_store` fixed its own copies of the same mistakes in Session 25b without the
  spike being updated. It is not redundant either. It is the smallest compute canary (an
  `s_endpgm` no-op, so a failure isolates the submit path from shader correctness), and the
  only hardware caller of the 3-chunk submit.
- **"2 s is not the 10 s TDR window."** On this box's kernels the compute ring timeout *is*
  ~2 s: the filed 2015 and 2206 ms runs on 7.1.8 (2026-08-19) were ring timeouts
  (observation A), and the PGM-defect cases took 2012–2064 ms on 7.2.3. The amdgpu
  `lockup_timeout` parameter is at its default. The 10 s figure came from the 2026-04 notes on
  kernel 6.18.
- **"Plain drift in a hand-rolled 2026-04 PM4/CS sequence."** The ablation put back each
  hand-rolled element on its own, and each one passes. Only the IB permission and the PGM
  encoding fail.
- **Session 24's "the 3-chunk submit never queues the IB to MEC; FENCE is what queues it".**
  On 7.2.3 that is false: A3, B2 and the repaired spike all complete on the 3-chunk path.
  Session 24 changed four things at once (FENCE, BO_HANDLES sentinels, IB flag 0x08,
  priorities), and in that session the 4-chunk submit did not execute the IB either. The claim
  reached the code comments in Session 25b without a test that isolated the FENCE chunk. All
  along, the in-tree 3-chunk caller (this spike) had the READABLE-only IB. The comments in
  `src/backend_native_amdgpu.cyr` are corrected in 4.1.3. Session 9's BO_LIST-vs-BO_HANDLES
  conclusion was also drawn with this spike's READABLE-only IB, and has not been re-tested.
- **"The four flaky render/sampler gates look like contention."** They were not contention.
  The spike caused them. See the next section.

## The "four flaky render/sampler gates" were the spike's MODE2 reset

The original note said that on the first `make test-native-all` run, `array-sample`,
`bc-array`, `bilinear-sample` and `compressed-sample` failed together, then passed on a re-run
and individually.

**The render bug.** Until 4.1.3 the native draw path never wrote `PA_CL_VTE_CNTL` (Mesa
gfx9.json `map.at` 165912 = 0x28818). A draw therefore ran with whatever value the context
register state held. Diagnostics on 2026-09-16 showed:

- **Before any GPU reset on that boot:** the state did not survive idle or a context switch.
  A value written in the same context still held for an immediate draw (4096 pixels
  covered), but after ~150 ms of idle or a context switch draws saw VTE = 0 and covered only
  pixel (0,0); the other 4095 pixels kept the `0x55555555` clear sentinel. Writing VTE =
  `0x43F` explicitly covered all 4096.
- **After a MODE2 reset:** the state persisted across idle, context switches and processes,
  so draws inherited a working value and passed. Writing VTE = 0 from one process made the
  next process's `native_texture_sample_e2e` fail. Restoring `0x43F` made it pass again.

In short: the inherited state did not survive idle or a context switch before a reset, and
persisted after one.

**How the spike masked it.** Defect 1 of the spike produces exactly such a MODE2 reset.
`test-native-all` runs gates in alphabetical order:

- `array-sample`, `bc-array`, `bilinear-sample` and `compressed-sample` run **before**
  `compute-spike`.
- `cube-sample`, `load-png` and `texture-sample` run **after** it.

**What the logs show:**

- **2026-08-19** (kernel 7.1.8). The kernel log shows defect 1's 2026-09-16 signature
  (illegal opcode, failed ring reset, MODE2 reset) at 17:38:12, from a process whose name is
  truncated to `native_compute_`, which fits both `native_compute_spike` and
  `native_compute_store`. `native_compute_store` passed in the same runs. The four gates that
  ran before the reset failed, the three after it passed, and every later re-run passed.
  (The later ring-timeout events are observation A.)
- **2026-09-16**, spike parked in `NATIVE_KNOWN_FAIL`, so it did not run and there was no
  reset. The first `test-native-all` failed **all seven** of these render/sampler gates (63
  passed, 7 failed). After the spike repro reset the GPU, the same suite passed 71/0.

The render fix is the explicit render state the stream now owns (`PA_CL_VTE_CNTL` plus the
audited set): every context register the 4.1.3 render-state audit found the draw inheriting
is now written by the draw's own PM4 stream. VTE is the one proven on hardware to cause these
gate failures. It belongs in the 4.1.3 CHANGELOG, and
the roadmap's "4.1.3 closeout" item tracks its hardware confirmation. The clean-boot closeout
run confirms it along with the spike. For the render fix, that run only counts if no GPU reset
has happened on that boot, because a reset is exactly what hid the bug.

## Found while resolving this

- **`programs/native_compute_store.cyr`** carried the same blind spot. Its "likely TDR" hint
  fired only at ≥ 9000 ms, and its `0xC0FFEE12` marker was described as proof that the CP
  "cleared" the dispatch. 4.1.3 gives it the same `QUERY_STATE2` check: exit 17 after
  dispatch #1, 18 after dispatch #2. The hint moves to a ≥ 1.5 s threshold, and the comments
  say what the marker does and does not prove. The new check is built and lint-clean. Its
  hardware run belongs to the 4.1.3 closeout run.
- **The library has the same blind spot.** `gpu_compute_dispatch` on the native AMD path
  (`native_compute_dispatch_cached*` in `src/backend_native.cyr`) returns the syncobj wait's
  result and nothing else. A5b used the same `native_cs_submit_4chunk` + `native_syncobj_wait`
  primitives, and the wait returned 0 after 2064 ms on a hung dispatch. So a consumer gets
  success, and stale output, from a dispatch the kernel timed out and ring-reset. This is
  tracked in `docs/development/roadmap.md`.
