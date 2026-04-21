# 2026-04-19 — `fncall6 + wgpu-native` crash: resolution + policy

**Severity:** MEDIUM — not a blocker (the existing struct-pack
workaround is correct); this issue documents **why** the
workaround is correct and codifies it as a policy so new FFI
slots don't reintroduce the bug.

**Affected:** any call through `src/wgpu_ffi.cyr` that would
directly `fncall6` into a wgpu-native C function.

**Status (2026-04-19):** resolved upstream-side —
**the crash is NOT a cyrius bug**. The cyrius-side `fncall6`
inline asm has been proven correct in isolation by a new
regression test in the cyrius repo at
`tests/tcyr/fncall_repro.tcyr` (v5.4.13-dev; see "Upstream
verification" below). Mabda has **no code changes required**;
the struct-packing pattern used in `deps/wgpu_main.c` (lines
57-124, `WgpuMapArgs` / `WgpuCopyArgs`, plus the new
`WgpuBeginPassArgs` / `WgpuCopyTexToBufArgs` proposed in
`docs/proposals/2026-04-19-render-pass-ffi.md`) is the correct
long-term approach and should be the default for any wgpu FFI
slot that meets the criteria in "Policy" below.

**Blocks:** nothing directly. Unblocks a cleaner note in
`feedback_fncall6_wgpu` memory (can be updated from "fncall6
bug" to "struct-by-value ABI quirk requires C shim").

---

## Summary

Cyrius v2.4.0+'s `lib/fnptr.cyr` exposes `fncall0` through
`fncall6` for calling function pointers with up to 6 integer
args. Mabda observed a crash when calling wgpu-native C
functions (notably `wgpuBufferMapAsync`) with 6 args via
`fncall6`, documented in the `feedback_fncall6_wgpu` memory as
a cyrius bug. The working theory at the time was a defect in
cyrius's 6th-arg inline asm.

**That theory is wrong.** Cyrius v5.4.13-dev adds
`tests/tcyr/fncall_repro.tcyr`, which exercises `fncall0`
through `fncall6` cyrius-only (no wgpu) with:

- Distinct prime-like argument values (5, 13, 21, 29, 37, 41)
  — a mis-loaded register produces a uniquely identifiable
  wrong sum, not a masked-by-symmetry wrong sum.
- A callee-with-local-stack variant — forces the 6th-arg
  register to flow through a store/load round-trip, which
  would fail if stack alignment or red-zone usage were
  subtly wrong.
- Register-order discrimination — catches swapped-register
  bugs (e.g. r9 ← b instead of r9 ← f).

All nine assertions PASS on x86_64. The `fncall6` inline asm
in `lib/fnptr.cyr:165-178` correctly loads
`rdi, rsi, rdx, rcx, r8, r9` in the right order, and the
64-byte local frame leaves RSP 16-byte aligned at the
`call rax` instruction, matching SysV ABI requirements.

## What the crash actually is

The wgpu-native C functions that "crash with fncall6" all
share one property: **at least one of their parameters is a
struct-by-value, not a scalar or pointer**.

Concrete example from `deps/wgpu_main.c:68-76`:

```c
void wgpu_shim_buffer_map(WGPUDevice device, WgpuMapArgs* args) {
    WGPUBufferMapCallbackInfo cb = { ... };
    wgpuBufferMapAsync(args->buffer, (WGPUMapMode)args->mode,
                       args->offset, args->size, cb);
}
```

`wgpuBufferMapAsync`'s signature is effectively:

```c
void wgpuBufferMapAsync(
    WGPUBuffer buffer,               // pointer  → rdi
    WGPUMapMode mode,                // u32      → rsi
    size_t offset,                   // u64      → rdx
    size_t size,                     // u64      → rcx
    WGPUBufferMapCallbackInfo cb     // struct-by-value, ~40 bytes
);
```

The last parameter is a **struct-by-value passed by aggregate
classification rules** (SysV §3.2.3). `WGPUBufferMapCallbackInfo`
in wgpu-native v29 is larger than 16 bytes, so the ABI puts
it entirely on the stack, NOT in a register. Cyrius's
`fncall6` does the opposite: it loads the 6th arg into `r9`
(integer register) and calls. The callee reads its 5th arg
from the stack (where cyrius never wrote), gets garbage, and
eventually crashes inside wgpu's callback-dispatch path when
the garbage "mode" or "callback" field is dereferenced.

The same mechanism explains every other "mysterious fncall6
crash" in wgpu-native: any time the C callee has a
struct-by-value parameter, cyrius's register-only calling
convention breaks the handoff. It's not specific to
`fncall6` — `fncall3`, `fncall4`, `fncall5` have the same
property if one of their args is a struct-by-value. `fncall6`
was noticed first because wgpu's buffer-map and render-pass
surfaces happen to use 6-argument signatures.

## Upstream verification

Cyrius v5.4.13-dev repo, test run on x86_64 Linux:

```
$ cat tests/tcyr/fncall_repro.tcyr | build/cc5 > /tmp/repro
$ chmod +x /tmp/repro
$ /tmp/repro
=== fncall0..6 — arg-count coverage ===
=== fncall6 — register-order discrimination ===
=== fncall6 — callee with stack ===

9 passed, 0 failed (9 total)
```

The test will be committed at v5.4.13 release. Lang-agent
verified RSP alignment at the `call rax` site is 16-byte
aligned (64-byte local frame, ABI-compliant).

aarch64 verification is the natural next step. AAPCS64 passes
args 1-8 in `x0..x7` registers (not register-starved at 6
args like SysV), so the single-arg struct-by-value failure
mode above is arch-specific. On aarch64, the crash shape will
be different: aggregates >16 bytes are also passed on the
stack (AAPCS64 §B.4), and aggregates 9-16 bytes get
register-paired — so the struct-by-value class fails there
too, just with different symptoms.

## Policy — when to struct-pack vs direct-call

Adopt as the default rule for new entries in
`src/wgpu_ffi.cyr` and `deps/wgpu_main.c`:

### Direct `fncallN` — OK when ALL of:

1. N ≤ 6 arguments (today — raises to N ≤ 8 after cyrius
   v5.4.13 lands `fncall7` / `fncall8`).
2. **Every argument is a scalar (integer, pointer, or enum
   widened to i64)**. No struct-by-value parameters.
3. **No float/double arguments.** SysV puts floats in
   `xmm0..xmm7`, not `rdi..r9`; cyrius's `fncallN` doesn't
   load xmm registers. Float args need a C shim that accepts
   int-encoded bits and casts inside.
4. **Not a variadic function.** SysV variadic convention
   requires `AL = number of SSE registers used` before the
   `call`; cyrius's `fncallN` doesn't set `AL`. Any variadic
   C callee (e.g. `printf`, `wgpuLog*`) needs a C shim with
   a fixed signature.

### C shim (struct-pack) — REQUIRED when ANY of:

1. The C function takes a **struct-by-value** parameter
   (regardless of struct size). This is the wgpu failure
   class. `WGPURenderPassDescriptor`, `WGPUBufferMapCallbackInfo`,
   `WGPUTexelCopyTextureInfo`, and most descriptor structs
   fall here.
2. The C function takes a **float or double** argument
   (rare in wgpu — most numeric args are u32/u64/size_t).
3. The C function is **variadic**.
4. The argument count exceeds the current `fncall` ceiling
   (now 6, raising to 8 — above that, struct-pack).

### Shim shape

Accept a `SomethingArgs*` pointer from Cyrius, unpack fields
in C, call the real wgpu function with ABI-correct layout.
Cyrius calls the shim via `fncall2(shim_fp, subject_handle,
&args_struct)`. The pattern already in `deps/wgpu_main.c`
for `wgpu_shim_buffer_map` / `wgpu_shim_copy_buffer_to_buffer`
is canonical — replicate it.

## Concrete follow-ups (for mabda agent)

1. **Update `feedback_fncall6_wgpu` memory** — the entry
   currently describes this as a cyrius bug. Rewrite to:
   > wgpu-native C functions with struct-by-value parameters
   > can't be called directly via `fncallN` because cyrius's
   > calling convention passes all args in integer registers.
   > SysV aggregate-classification rules put struct-by-value
   > args >16 bytes on the stack, which cyrius never sets up.
   > Always use a C shim that accepts `SomethingArgs*` and
   > unpacks in C. Policy: `docs/issues/2026-04-19-fncall6-wgpu-crash-resolution.md`.

2. **Audit `src/wgpu_ffi.cyr`** — every slot currently using
   a C shim is correct and should stay. Any slot added
   directly (no shim) that calls a wgpu function with a
   struct-by-value parameter is latent-broken; convert to
   shim form before exercising at runtime.

3. **Proposal doc update** — the existing render-pass FFI
   proposal at `docs/proposals/2026-04-19-render-pass-ffi.md`
   line 41 says the shim is needed "because direct call hits
   the `fncall6 + wgpu` ABI bug per `feedback_fncall6_wgpu`".
   Correct but imprecise — tweak to "because
   `WGPURenderPassDescriptor` is a struct-by-value parameter
   per SysV §3.2.3 aggregate rules, which cyrius's `fncallN`
   doesn't handle (see
   `docs/issues/2026-04-19-fncall6-wgpu-crash-resolution.md`)."

4. **Pin bump**: ✅ done. `cyrius.cyml` bumped
   `cyrius = "5.4.10"` → `"5.5.11"` (picks up `fncall7` /
   `fncall8` added at 5.4.13, plus the updated `lib/fnptr.cyr`
   header documenting the struct-by-value handshake). Those
   let future all-scalar 7- or 8-arg wgpu signatures skip the
   shim layer — BUT only if they also satisfy the scalar /
   no-float / non-variadic criteria above, AND only on x86_64
   (cyrius's 6-register convention diverges from AAPCS64 past
   arg 6, so fncall7/8 are not AAPCS64-compatible for C
   interop on aarch64). Do not retrofit existing shims; the
   shim layer is cheap and locks in correct ABI handshake.

5. **Nothing to change in existing shims.** `wgpu_shim_buffer_map`,
   `wgpu_shim_copy_buffer_to_buffer`,
   `wgpu_shim_create_command_encoder`,
   `wgpu_shim_command_encoder_finish`,
   `wgpu_shim_queue_submit_one` are all shaped correctly per
   this policy.

## References

- Cyrius repro test: `tests/tcyr/fncall_repro.tcyr` (at
  v5.4.13 release).
- Cyrius `fncall6` implementation:
  `lib/fnptr.cyr:165-194` (inline asm unchanged;
  byte-identical across v5.4.0 → v5.4.12-1).
- SysV ABI, Intel 386 and AMD x86-64 supplement §3.2.3
  (aggregate classification).
- ARM AAPCS64, IHI 0055F §B.4 (aggregate passing rules).
- wgpu-native v29 `WGPUBufferMapCallbackInfo` layout in
  `deps/wgpu_ffi/include/webgpu/webgpu.h`.
