# `native_spirv_saxpy_e2e` has been red since at least 4.0.5 — `gfx9_compile` returns `MIR_ERR_ID_OOR`

**Status:** 🟡 **OPEN** — found running the HW gates during the 4.0.10 closeout sweep.
**Placement:** `src/mir.cyr` / `src/spirv_lower.cyr` / `src/gfx9_isel.cyr` — the same
subsystem as [`2026-08-19-native-spirv-compile-limits.md`](2026-08-19-native-spirv-compile-limits.md),
and plausibly the same root cause. Schedule the two together.
**Discovered:** 2026-08-19, mabda 4.0.10 closeout (not by a consumer).
**Severity:** **Medium** — an in-tree hardware gate is failing and has been for four
releases. No consumer has reported it, but it is the repo's own multi-binding oracle.
**Affects:** mabda **4.0.5 → 4.0.10** (every tag tested; see bisect below). Native AMD
(amdgpu / GFX9), HW: Cezanne (gfx90c), `/dev/dri/renderD128`.

## Symptom

```
$ make test-native-spirv-saxpy-e2e
mabda native SPIR-V multi-binding e2e (N.6) — novel 2-buffer kernel on Cezanne
----------------------------------------------------------------------------
FAIL: gfx9_compile rc=-25
make: *** [Makefile:537: test-native-spirv-saxpy-e2e] Error 2
```

The program never reaches dispatch — it fails at compile
(`programs/native_spirv_saxpy_e2e.cyr:213`). Its kernel is
`y[gl_LocalInvocationID.x] = 3*x[lid.x] + y[lid.x]` (uint, LocalSize 8, two storage
bindings, SPIR-V id bound 25, 576 bytes).

## What `-25` is

`0 - MIR_ERR_ID_OOR` (`src/mir.cyr:175`). Exactly three sites return it:

| Site | Condition |
| --- | --- |
| `src/mir.cyr:244` (`_mir_set_val`) | `id <= 0` — a **zero or negative** id |
| `src/mir.cyr:245` (`_mir_set_val`) | `id >= cap_ids` |
| `src/mir.cyr:308` (`mir_emit`) | `result != 0 && result >= cap_ids` |

⚠ **Which of the three fires is NOT yet known** — and the distinction is the whole
question. Two of them mean "the caps are too small"; the first means "the lowering
produced an unresolved (0) id", which would be a genuine defect rather than a
provisioning problem. Instrumenting the three sites to print which one fired is the
cheapest possible first step and should be step 1 of the fix.

## It is NOT the known under-provisioned-test-buffer trap

This repo has a documented failure mode where undersized `vals`/`instrs`/`alloc`
buffers let `mir_mod_init`/regalloc `memset` past them and produce a bogus late error.
That is **ruled out here by measurement**, not by argument.

A scratch copy of the program was built outside the repo with **every** capacity raised
~4×, all at once:

| Knob | Original | Probe |
| --- | --- | --- |
| `CC_CAP_IDS` | 26 | 100 |
| `mir_mod_init` `cap_ids` | 40 | 160 |
| `mir_mod_init` `cap_instrs` | 32 | 128 |
| `blocks[]` / cap | 32 | 128 |
| `vals[]` | 1440 | 5760 |
| `instrs[]` | 1024 | 4096 |
| `types[]` / `consts[]` / `deco[]` / `outbi[]` | 640 / 448 / 448 / 256 | 2560 / 1792 / 1792 / 1024 |
| `isel[]` / `isel2[]` / `alloc[]` | 768 / 768 / 640 | 3072 / 3072 / 2560 |
| `ptrs[]` | 256 | 1024 |
| `isa[]` | 512 | 2048 |

**Result: byte-identical failure, `rc=-25`.** So either the id is `<= 0` (site 1), or
something is producing an id ≥ 160 for a kernel whose SPIR-V id bound is 25 — and both
readings point at the lowering, not at the test's stack budget.

To rebuild the probe: copy `programs/native_spirv_saxpy_e2e.cyr` to a scratch directory,
apply the right-hand column above, and `cyrius build <scratch>/saxpy_probe.cyr` from the
repo root (the `include "src/lib.cyr"` resolves against cwd).

## Bisect — not a 4.0.10 regression

Reproduced with the **identical** `rc=-25` from a clean detached worktree at each of:

| Tag | Result |
| --- | --- |
| `4.0.5` | `FAIL: gfx9_compile rc=-25` |
| `4.0.7` | `FAIL: gfx9_compile rc=-25` |
| `4.0.8` | `FAIL: gfx9_compile rc=-25` |
| `HEAD` (`a43bb15`, = 4.0.9 + the other issue's doc) | `FAIL: gfx9_compile rc=-25` |
| working tree (4.0.10) | `FAIL: gfx9_compile rc=-25` |

⛔ **So this predates both the 6.5.29 pin and the 6.5.20 pin, and predates the 4.0.10
`cyrfmt` reflow.** It is not caused by any of them. Earlier tags were not tested; 4.0.5
is a floor, not a known-good boundary — the true first-bad commit is still unidentified.

## Why it survived four releases unnoticed

`test-native-spirv-saxpy-e2e` is a **standalone** Makefile target (`Makefile:535-537`).
It is in no aggregate: not `test-all`, not `test-gpu`, and not among the two native
gates CLAUDE.md's closeout pass names (`test-native-compute-store`,
`test-native-render-e2e` — **both of which pass**). CI cannot run it at all, having no
amdgpu hardware. So nothing routinely executes it.

⭐ **That is the more general finding, and it is worth fixing independently of the
compile bug:** the 71 `test-native-*` targets have no roll-up target, so "the native HW
suite passes" is not a statement anyone can make in one command. A
`test-native-spirv-all` (or similar) that fans over them and reports a tally would have
caught this in whichever release actually broke it.

## What still passes on the same box, same commit

This is narrowly scoped, not a broad native-path outage. Verified green on Cezanne
during the same session:

- `test-native-compute-store` — incl. the second dispatch on a cached IB+fence path
- `test-native-spirv-compute-e2e`
- `test-native-spirv-downsample-e2e` — ⚠ **also two bindings** (`bindings=2`,
  `isa_len=208`), and it pixel-matches CPU
- `test-native-spirv-f64-arith-e2e` — all 8 lanes bit-exact
- `test-native-render-e2e` — `pixel(0,0) = (0xFF,0x00,0x00,0xFF)`
- `test-native-kms-summary` — render → PRIME → master AddFB2, `fb_id=142 PASS`

⭐ **Multi-binding alone is therefore not the trigger** — downsample compiles two
bindings fine. The most obvious structural difference in the saxpy kernel is that it
**reads and writes the same binding** (`y` is both an input and the destination), a
read-modify-write on one storage buffer. That is a hypothesis for the first bisect cut,
explicitly not a conclusion.

## Relationship to the ranga M6 filing

[`2026-08-19-native-spirv-compile-limits.md`](2026-08-19-native-spirv-compile-limits.md)
reports `gpu_shader_module_create_spirv` returning a bare **0** for a family of valid
SPIR-V, with "id bound exceeded" named as one of the reason codes it wants
distinguished. This filing is the same subsystem reached one layer lower — the in-tree
program calls `gfx9_compile` directly, so it *does* get a code back, and that code is
the id-range one.

⚠ **Do not assume they are one bug.** They may share a root cause or may not; the
evidence for a link is that both are id/liveness-shaped failures in
`spirv_lower`/`isel`. What *is* certain is that fixing the other filing's item 1 —
returning a reason instead of a null handle — would also make this failure
self-describing, which is why they should be worked together.

## Proposed order of work (v4.0.11)

1. **Instrument the three `MIR_ERR_ID_OOR` sites** so the code says *which* fired and
   with what id. One session's worth of work; unblocks everything below.
2. If site 1 (`id <= 0`): trace back to the lowering step that emitted an unresolved id.
   That is a real defect and likely explains part of the ranga filing too.
3. If sites 2/3 (`id >= cap_ids`): find what synthesizes ids past a 160 ceiling for a
   25-id module — a runaway synth loop, not a budgeting shortfall.
4. **Add a CPU assertion** in `tests/tcyr/compiler_*.tcyr` that compiles this exact
   saxpy SPIR-V and asserts `rc == 0`, so the fix is defended without hardware. The
   kernel bytes are already in `build_saxpy_kernel` in the e2e program.
5. **Add a roll-up `test-native-*` target** with a pass/fail tally, so a red HW gate is
   discoverable in one command instead of four releases.

## Notes

- Per this repo's convention the fix must land with an assertion that would have caught
  the original bug (step 4 above), not merely a green e2e.
- `MIR_ERR_ID_OOR` is `25`; the compile driver returns errors negated, hence `-25`. The
  disjoint-range comment at `src/gfx9_compile.cyr:26` is the map for decoding any other
  code seen here (mir 20-25, lower 26-27, regalloc 50-52, waitcnt 60-61, gisel 100-101,
  cmp 70-87).
