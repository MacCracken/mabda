# Native SPIR-V→GFX9 rejects several constructs silently, and gives up well under its own id cap

**Status:** ✅ **RESOLVED in v4.1.0** — every item below is fixed and HW-verified on
Cezanne. See "Resolution" at the end for what shipped, what the filing got wrong, and the
one item that was never a mabda defect.
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

---

## Resolution (v4.1.0)

Every item, in the filing's own priority order.

### 1. "Return a reason" — DONE

`gpu_last_shader_error()` returns the compiler's own code; `gpu_shader_error_name(code)`
renders it; `gpu_shader_error_print()` writes it to stderr. The stored value is the
**stage code**, not a lossy fold onto `GPU_ERR_SHADER` — the whole point was telling these
apart. Bands: MIR 20-32, lowering 26-34, regalloc 50-52, waitcnt 60-61, encode 70-87,
isel 100-104, staging 110-120.

⭐ **One line caused the entire problem.** `backend_native.cyr` read
`if (gfx9_compile(...) != 0) { return 0; }` — the specific stage code was computed, then
discarded at the comparison. Every failure in this filing arrived as a bare 0 because of
that. The non-compile paths (BO create, VA map, OOM, the f64 gate, source-kind mismatch)
were equally silent and now have codes too.

### 2. Early `OpReturn` inside a selection — DONE, now a loud rejection

`GISEL_ERR_RETURN_IN_SELECTION`. This was the only item that was **not** already a
rejection: it compiled, dispatched `rc=0`, and wrote nothing, because a divergent if runs
its then-block under a masked EXEC and `s_endpgm` there ends the **whole wave** before the
merge. Mutation-proven — disabling the guard makes the kernel select successfully again.

Nesting (`GISEL_ERR_NESTED_SELECTION`) and unclosed regions
(`GISEL_ERR_UNCLOSED_SELECTION`) were rejected all along; they were merely unnamed.

### 3. `OpFDiv` — DONE for f32, bit-exact

⚠ **The filing's diagnosis was wrong in a useful way.** `_spirv_lower_fdiv` did exist, and
the filing inferred "this is selection, not lowering". Neither: the lowering was **f64-only
and explicitly rejected f32** (`test_spirv_lower_fdiv_f32_rejects` asserted exactly that),
because wgpu/naga covered f32 and no native consumer existed. ranga is that consumer.

f32 now lowers through the same correctly-rounded 11-instruction GFX9 sequence as f64.
⚠ Deliberately **not** `mul(N, rcp(D))` — `v_rcp_f32` is a ~1-ulp approximation, so the
cheap form is not IEEE division. `programs/native_spirv_f32_div_e2e.cyr` pins bit patterns,
including a 1/3 ULP probe that the cheap form fails; **8/8 lanes bit-exact on Cezanne**.

### 4. Liveness budget — DOCUMENTED

`src/gfx9_regalloc.cyr` now carries the real numbers: no spilling (linear scan, overflow is
an error), VGPRs ~253-255 usable of 256, SGPRs ~90 of 102 for a typical 2-binding kernel,
**dropping to 100 the moment the kernel contains a divergent if** (s[100:101] holds the
saved EXEC), two SGPRs consumed per binding before any value is allocated, and f64 taking
even-aligned pairs.

⭐ **The filing's core insight was right and is now written down:** the id cap and the
register budget are independent limits, and hitting the register one first at 201 ids is
normal, not a bug. The fix for a kernel that does not fit is to shorten live ranges, not to
reduce value count — which is why the "pack the texels, extract later" restructuring the
filing tried did not help.

### 5. `OpSelect` and nested selections — DONE

An `OpSelect` whose condition is an **integer** compare now lowers, when the compare is the
immediately preceding instruction: `v_cndmask_b32` reads VCC directly, so it is a *single*
instruction — no mask materialization at all. ⚠ Adjacency is the correctness argument: VCC
is one physical register, so a non-adjacent compare fails loud
(`LOWER_ERR_SELECT_NOT_ADJACENT`) rather than reading a stale flag, and a uniform compare
(SCC, not VCC) fails as `LOWER_ERR_SELECT_UNIFORM_COND`.

This removes the need for the `d = (b1 - 1 - v1) | (b2 - 1 - v2)` underflow trick.

### Beyond the filing: loops

`OpLoopMerge` / `OpPhi` were listed as blocking `gaussian_blur`. Both now lower. Two
independent blockers had to go:

- **Back-edge liveness.** A forward linear scan frees a value at its textual last use, so a
  value read at the top of a loop body could have its register reused later in that body —
  correct on iteration 1, corrupt from iteration 2. Intervals whose last use falls inside a
  loop now extend to the loop's end, to a fixed point so nested loops converge.
- **Phi placement.** SPIR-V writes `OpPhi` at the top of the merge block, but the copy it
  implies must happen at the **end of each predecessor** — at the loop header it would
  re-run every iteration and reset the counter. Copies are emitted on the incoming edges,
  and the allocator now assigns each value one register across both of its definitions.

`programs/native_spirv_loop_e2e.cyr`: a 4-iteration counted loop carrying **two** phis
(counter and accumulator), `out[i] == 4 * a[i]`, 8/8 lanes correct on Cezanne. Both
allocator changes are mutation-proven.

### The one item that was never a mabda defect

⚠ **`ULessThan` is NOT absent from the binop map.** `_spirv_cmp_to_mir` has mapped
`SPIRV_OP_ULESSTHAN -> MIR_OP_ICMP_ULT` since the map was written. The listed workaround
(swap the operands, use `UGreaterThan`) was unnecessary. Whatever ranga hit there was
something else — most likely the bare 0 from an unrelated failure in the same kernel, which
is precisely the confusion item 1 existed to end.

### Consumer guidance

The workarounds in this filing can be retired: early returns no longer need inverting (the
form is rejected loudly, so restructure knowingly), reciprocals no longer need passing in as
uniforms, and the `OpSelect` bit-31 underflow trick is unnecessary for an adjacent divergent
compare. Nested selections still need flattening into two sequential ifs — that one is a
real remaining limit, and it now says so by name.

