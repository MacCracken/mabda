# Native SPIR-V→GFX9 rejects several constructs silently, and gives up well under its own id cap

**Status:** 🟡 **OPEN** — found porting ranga's 14 GPU compute kernels to mabda's native AMD backend.
**Placement:** `src/spirv_lower.cyr` / `src/gfx9_isel.cyr` / register allocation.
**Discovered:** 2026-08-19, ranga M6 (11 of 14 kernels landed; the rest blocked on this)
**Severity:** **Medium-High** — every failure mode below is a silent `return 0`. No code, no message, no log.
**Affects:** mabda 4.0.9–4.0.10, native AMD (amdgpu/GFX9), HW: Cezanne (gfx90c)

## Summary

`gpu_shader_module_create_spirv` returns **0** for a range of valid SPIR-V, with no
diagnostic of any kind. Each case below was isolated by bisecting a *working* kernel one
construct at a time, on real hardware.

The single most valuable fix here is not any individual construct — it is **returning a
reason**. Every one of these cost a full compile-bisect to identify, because a null handle
is the only signal the API produces.

## Constructs rejected

| Construct | Result | Notes |
| --- | --- | --- |
| `OpFDiv` | returns 0 | Integer `OpUDiv`/`OpUMod` are fine. `_spirv_lower_fdiv` exists, so this is selection, not lowering. |
| `OpSelect` | returns 0 | `_spirv_lower_one_instr` has a case for it; it is dispatched but not selectable. |
| selection nested in a selection | returns 0 | Two SEQUENTIAL selections compile. Nesting one inside another does not. |
| `OpReturn` inside a selection | **compiles, then does nothing** | See below — the worst of the set. |

### The silent no-op

A bounds guard written the way WGSL writes it —

```
if idx >= count { return; }
<body>
```

— compiles to a **non-zero shader handle** and dispatches with **rc = 0**, and writes
nothing at all. `programs/native_spirv_divergent_if_e2e.cyr` has the shape that works:
body in the then-block, then-block ends with `OpBranch` to the merge, merge block holds
the function's only `OpReturn`. Consumers have no way to discover this except by noticing
their output buffer is unchanged.

Rejecting the early-return form outright would be far better than executing it as a no-op.

## Capacity: gives up well below the stated cap

`NATIVE_SHADER_CAP_IDS` is 256. Growing a kernel by adding texel loads and lerps, all
sharing ONE address:

```
texels=6  ids=204  instrs=228  compiles=1
texels=7  ids=229  instrs=253  compiles=1
texels=8  ids=254  instrs=278  compiles=0
```

That boundary is consistent with the 256-id cap and the synth-id headroom the comment in
`_native_shader_compile_spirv` describes. Fine.

**But ranga's bilinear resize fails at 201 ids / 225 instructions** — smaller than the
6-texel kernel that compiles. The difference is that it holds **four independent texel
addresses** live rather than reusing one. Restructuring to keep the four texels packed and
extract one channel at a time (cutting peak liveness from ~20 values to ~12) did not help.

Every individual construct in it compiles in isolation: the index arithmetic, the
`FLOOR`/`FMin` ext-inst calls, `ConvertFToS` used as an access-chain index, reading an f32
param, and a computed-index load with unpack and pack. Only the combination fails.

So the effective limit is **register pressure, not id count**, and it is not documented or
reported. A four-tap bilinear filter is an entirely ordinary compute kernel.

## Reproduction

`ranga/src/gpu_kernels.cyr` at the ranga commit for M6 — `gk_resize_bilinear` builds the
module, `tests/gpu_kernels.tcyr` asserts that `spirv_validate_stream` accepts it, that its
id bound is under 256, and that `gpu_shader_module_create_spirv` nonetheless returns 0.
The bisection kernels above are reconstructible from `_gk_load_unpack` + `_gk_lerp`.

## Proposed fix

In priority order:

1. **Return a reason.** An out-pointer or a `gpu_last_shader_error()` distinguishing
   "unsupported opcode X", "id bound exceeded", "register allocation failed", "unsupported
   control flow". This alone converts every case here from a bisect into a one-line fix.
2. **Reject the early-return-in-selection form** instead of compiling it to a no-op. A
   loud failure is strictly better than a shader that runs and does nothing.
3. **`OpFDiv`** — the most commonly needed missing op. Consumers currently have to pass
   reciprocals in as uniforms, which works but is contagious: every derived quantity has to
   be precomputed host-side.
4. **Spilling, or a documented liveness budget.** If the allocator cannot spill, saying so
   — with the actual live-value ceiling — lets consumers structure kernels to fit instead
   of discovering the wall by bisection.
5. `OpSelect` and nested selections, which together would remove most of the arithmetic
   contortions consumers need for a two-condition guard.

## Consumer-side workarounds in use

ranga ships 11 of 14 kernels on the native path today:

- **Early return** → invert the guard: `if (count > idx) { body } return`.
- **`OpFDiv`** → host computes reciprocals and passes them as f32 params.
- **`OpSelect` + nesting** → for `b1 > v1 && b2 > v2` on values below 2^31, form
  `d = (b1 - 1 - v1) | (b2 - 1 - v2)` and test `d < 0x80000000`; each term underflows and
  sets bit 31 exactly when its condition fails.
- **Two-bound 2D guards** → dispatch 1D and recover x/y with `UMod`/`UDiv`, so only one
  bound is needed.
- **`ULessThan`** (absent from the binop map) → swap the operands and use `UGreaterThan`.

`resize_bilinear` and `blend` (13 modes) are blocked on capacity; `gaussian_blur` is
blocked separately on `OpLoopMerge`/`OpPhi` being rejected.
