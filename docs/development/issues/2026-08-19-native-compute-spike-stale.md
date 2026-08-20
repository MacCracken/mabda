# `native_compute_spike` is bit-rotted and superseded — retire it or repair it

**Status:** 🟡 **OPEN (known-fail, not blocking)** — listed in the Makefile's
`NATIVE_KNOWN_FAIL` so `make test-native-all` reports it as a named known-fail rather than
either hiding it or blocking the suite.
**Placement:** `programs/native_compute_spike.cyr`, `Makefile:test-native-compute-spike`.
**Discovered:** 2026-08-19, by `make test-native-all` on its first ever run (v4.0.11).
**Severity:** **Low** — a stale exploration artifact, not a defect in shipping code. The
path it probes is covered, and green, elsewhere.
**Affects:** the in-tree spike program only. No consumer-facing surface.

## Symptom

```
$ make test-native-compute-spike
HW_IP_INFO GFX: rc=0 available_rings=0x1
HW_IP_INFO COMPUTE: rc=0 available_rings=0xf
submitted to COMPUTE ring
dispatch completed (sync-obj signaled) in 2206 ms
stub[0] = 0x0  (FAIL — want 0xCAFEBABE)
```

Reproducible, twice in a row, on Cezanne (gfx90c) — 2015 ms and 2206 ms.

## Why it is NOT a native-compute regression

`programs/native_compute_spike.cyr` is the **v3 Phase B.3.d** spike: the first live AMDGPU
dispatch from pure Cyrius, dating to 2026-04. It uploads a bare `s_endpgm` shader and proves
the CP processed the IB via a CP-side `WRITE_DATA` landing `0xCAFEBABE` in `stub[0]`.

`test-native-compute-store` **supersedes it completely and passes**:

```
post-dispatch marker = 0xC0FFEE12 OK (CP cleared dispatch)
output[0] = 0xDEADBEEF (want 0xDEADBEEF)
dispatch #2 marker = 0xC0FFEE12 OK
dispatch #2 output[0] = 0xDEADBEEF OK — multi-dispatch on cached IB+fence path works
```

That gate carries the **same** CP-side marker proof *and* a real shader write *and* a second
dispatch on the cached IB+fence path. Everything the spike was written to establish is
established there, more strongly. A further 20+ `test-native-spirv-*` e2e gates dispatch
compiled kernels on the same path and pass.

⭐ So the native compute path is healthy; this one program has rotted around it.

## Likely cause (hypothesis, not established)

The spike's own header warns about exactly this shape:

> if it lands in `stub[0]` post-submit, the CP processed our IB. That distinguishes real
> completion from AMDGPU TDR (the kernel resetting our ring after 10 s and signalling the
> syncobj as if completed).

A ~2 s dispatch for an `s_endpgm` no-op, with the syncobj signalled and the marker absent,
is the signature that header describes. ⚠ But 2 s is not the 10 s TDR window it names, so
"it is a TDR" is a hypothesis to test, not a conclusion. Whoever picks this up should check
`dmesg` for a ring reset during the run before assuming.

The spike also predates several changes to the submit path it does not share with
`compute-store` (which uses the current N-BO cached-submit helpers), so plain drift in a
hand-rolled 2026-04 PM4/CS sequence is at least as likely.

## Options

1. **Retire it.** It is an exploration artifact whose findings are recorded in
   `CHANGELOG.md` and whose coverage is subsumed. Deleting the program and its Makefile
   target removes a permanently-red gate that teaches nothing.
2. **Repair it** as a minimal-dispatch regression canary — arguably useful precisely because
   it is simpler than `compute-store`, so a failure isolates the submit path from the shader.
3. Leave as-is in `NATIVE_KNOWN_FAIL`. ⚠ Acceptable only briefly: a known-fail list is a
   place to park *understood* red, not indefinite red.

Recommendation: **(1)**, unless someone wants the canary in (2). Either way it should not
survive another two releases as a known-fail.

## Notes

- This was found only because v4.0.11 added `make test-native-all`. Before that the 71
  `test-native-*` targets had no roll-up, and this gate — like
  `native_spirv_saxpy_e2e`, red for four releases — was simply never run. That is the
  same structural gap, caught by the same fix.
- ⚠ The roll-up also surfaced **flakiness**: on its first run,
  `test-native-array-sample-e2e`, `test-native-bc-array-e2e`,
  `test-native-bilinear-sample-e2e` and `test-native-compressed-sample-e2e` failed, then all
  four passed on a re-run and pass individually. Four render/sampler gates going red
  together under back-to-back load looks like contention rather than four separate bugs, but
  it is **unexplained** and worth its own investigation. Recorded here so it is not lost.
