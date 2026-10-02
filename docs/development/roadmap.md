# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius. **Three backends behind one
> public API**: wgpu-native (cross-vendor default), native AMD (amdgpu DRM /
> GFX9 / PM4), and native NVIDIA (nouveau DRM, Turing/SM75). Baseline:
> **v4.1.4** (2026-09-16). Module/assertion/bundle counts live in the
> filesystem + `CHANGELOG.md`, not here — they go stale on every cut.

This document is **forward-looking**. For detail on every shipped
release, see [`CHANGELOG.md`](../../CHANGELOG.md) — that is the
source of truth for completed work. Shipped-section details are
pruned from this file as each arc closes so it stays useful for
planning instead of bloating with history (v2.x pruned 2026-04-21;
v3.0 / v3.1 / v3.2 / v3.3 / v3.4 pruned 2026-06-19; v4.0 / v4.0.1 / v4.0.2
pruned 2026-07-02 — NVIDIA native backend, AMD-wgpu deprecation, and the
fncall6 struct-pack retirement + cyrius 6.3.35 bump, arcs closed. v4.0.3
(2026-07-13) is a maintenance cut — cyrius 6.4.62 pin + chitra 0.3.0 dep
refresh, no source change; v4.0.4 (2026-07-13) wired chitra's baseline JPEG
loader (`-D MABDA_JPEG`) + a provenance-stamp refresh; v4.0.5 (2026-07-13)
tracked the NVIDIA single-BO deferrals against the multi-BO backlog item below;
v4.0.6 (2026-07-16) is a maintenance cut — cyrius 6.4.64 pin, no source change;
v4.0.7 (2026-07-16) lifted the NVIDIA single-BO cap — multi-size BO allocator +
the constant-cache dispatch fix, HW-proven on the TU116; v4.0.8 (2026-07-30) is a
maintenance cut — cyrius 6.4.64 → 6.5.3 pin, no `src/` change, plus two latent
call-arity bugs in the test/bench suites that 6.5.1's arity gate exposed; v4.0.9
(2026-08-13) fixed a latent register-aliasing hazard in the GFX9 regalloc ceiling and
repaired the 6.5.3 pin, which named a toolchain the driver could not reach; v4.0.10 (2026-08-19) is a maintenance cut — cyrius 6.5.29 pin + chitra 0.3.1
dep refresh + a 73-file `cyrfmt` reflow, no behaviour change; v4.1.0 (2026-08-19) closed
both native SPIR-V filings — compile-failure reason codes, the saxpy scratch-buffer
overrun + a static sizing gate, f32 OpFDiv, OpSelect on integer conditions, structured
loops with OpPhi, `gfx9_rsrc1_ex`, the wgpu-native v29.0.1.1 bump, and `make
test-native-all`; v4.1.1 (2026-09-06) migrated to cyrius 6.6.0's `Result` value form —
113 pair-bind conversions and one Breaking signature, `gpu_result_unwrap(res)` →
`gpu_result_unwrap(t, v)`; v4.1.2 (2026-09-12) is a maintenance cut — cyrius 6.6.2 pin +
samvada 1.0.1 / chitra 1.0.3 dep refresh, no `src/` change; v4.1.3 (2026-09-16) is a
maintenance cut — cyrius 6.6.4 pin, `cyrius.cyml` comment cleanup, a doc currency sweep for
the samvada 1.0.1 / chitra 1.0.3 moves, a `test-native-all` FAIL-reason fix, per-harness
`make fuzz` logs, the `native_compute_spike` repair, and the native draw `PA_CL_VTE_CNTL`
fix behind the flaky render/sampler gates; v4.1.4 (2026-09-16) closed the items that cut and its
P(-1) audit left open — native AMD VA reclaim, NVIDIA job-error detection, the loader texture
leak, shader-cache / depth-texture release, and the bench and launcher leftovers).

## The Long Arc

Mabda is the **public GPU API** for the Cyrius ecosystem. It shipped
first via a C shim over wgpu-native so real projects (soorat, rasa,
ranga, bijli, aethersafta, kiran) could depend on a stable,
sovereign-owned surface immediately. At v3.0 a **pure-Cyrius DRM/KMS
backend landed alongside** the C launcher path (AMD / amdgpu / GFX9);
at v4.0 a **second native backend** landed (NVIDIA / nouveau / SM75).
All coexist, selectable per consumer, so the same bench suite exercises
each. This gives a clean A/B across two axes: **(C hooks vs native
Cyrius) × (pre-5.6.x vs post-5.6.x compiler optimizations)**. The public
API does not change across backends; consumer code stays byte-identical.

**Vendor scope of the native path.** AMD shipped native at **v3.0**
(verified on Cezanne / gfx90c; other AMD generations — GFX10/11/12,
RDNA\* — share the path but need per-generation bring-up). NVIDIA
shipped native at **v4.0** (Turing / SM75, HW-verified on a GTX 1660
SUPER / TU116; other SM families need bring-up). **Intel is tentatively
v5.0** (i915 / Xe — different ISA and command streamer). Intel hardware
continues on the wgpu backend, which remains the cross-vendor default;
AMD-on-wgpu is **deprecated** as of v4.0.1 (see the retirement policy
below).

```
  v2.0.0 → v4.1.4  ─▶  shipped — see CHANGELOG.md (Cyrius port → dual
                        backend → texture/shader breadth → asset loading →
                        array/cube textures → NVIDIA native → AMD-wgpu
                        deprecation)
  v3.x+            ─▶  WebGPU / WASM (blocked on Cyrius WASM backend)
  v5.0             ─▶  Intel native backend added (tentative); NVIDIA
                        wgpu path retires (NVIDIA → NVIDIA native only)
  v5.1             ─▶  Full wgpu retirement once Intel native is in
                        production — wgpu-native + C launcher + the
                        FFI binding code all leave the tree
```

Wgpu retirement is **per-chipset, not all-at-once**. Each major
vendor gets a release window where wgpu and native coexist for that
vendor (so existing consumers can flip on their schedule). Once a
vendor's native backend has been in production for a release cycle
**on that vendor's hardware**, the wgpu path is first *deprecated* for
that vendor (a one-shot warning, but it still works — an escape hatch,
with an opt-in strict flag to enforce), then dropped in a later release
— but the wgpu binding stays in-tree to serve the vendors
whose native backends haven't shipped yet. This keeps the consumer
migration smooth: nobody on a wgpu-only chipset is forced to move
before their native backend is real.

The concrete cutovers:
- **v4.0** — NVIDIA native shipped. AMD wgpu **still supported** (one
  release window of coexistence on AMD before deprecation).
- **v4.0.1** — AMD wgpu **deprecated** (one-shot warning; still works by
  default — escape hatch open; `-D MABDA_AMD_WGPU_STRICT` enforces
  native-only). Full AMD-wgpu retirement is deferred to a later release.
  NVIDIA + Intel still on wgpu.
- **v5.0** — NVIDIA wgpu retires. NVIDIA consumers now on NVIDIA
  native. Intel still on wgpu.
- **v5.1** — Intel wgpu retires; wgpu+C path leaves the tree
  entirely. Mabda is fully native-Cyrius across every supported
  vendor.

Each retirement is gated by **(a)** that vendor's native backend
having been in production across a full release cycle, and **(b)**
every consumer that ships on that vendor having flipped. Retirement
is consumer- and vendor-driven; the version numbers above are the
target shape, not a commitment.

Everything in this roadmap prioritizes **API stability** over
**backend correctness**. The public surface (context, buffer, compute,
texture, render_pipeline, render_pass, render_graph, etc.) is the
load-bearing contract. The FFI layer underneath is explicitly marked
internal (`@internal` header on line 1 of every FFI module) so no
consumer accidentally couples to it.

---

## Moving the cyrius pin to 6.6.5

⛔ Before bumping the pin to 6.6.5: cross-reference the six deferral comments that turn the
*Untracked deferrals* CI step red. 6.6.5's cyrlint folds case and reads a whole comment paragraph,
so `--strict-deferrals` now sees prose the 6.6.4 cyrlint never matched.

cyrius 6.6.5 is not tagged yet. Nothing below can land against the pin until it is, except items
marked **(can land now)**. The pin is 6.6.4 today, and this section lists only what 6.6.5 itself
changes.

- [ ] ⛔ The *Untracked deferrals (cyrlint --strict-deferrals)* step (`.github/workflows/ci.yml:60`)
      fails on six notes. Put a tracking pointer (CHANGELOG / issue / roadmap / `docs/` path / a
      version such as `v4.2`) on a line the phrase itself touches, or mark prose that is not a
      deferral `#skip-lint`. **(can land now)** — a same-line pointer or `#skip-lint` does not
      depend on the toolchain.
  - `src/debug.cyr:14` — "For now, provide no-op stubs".
  - `src/gfx9_abi.cyr:101` — "Deferred out of 4.0.9" (history: the deferred API shipped in 4.1.0).
  - `src/vertex.cyr:121` — "For now, provide attribute arrays".
  - `src/wgpu_ffi.cyr:228` — "Deferred to the next launcher fn-table revision". Its
    `docs/audit/2026-07-02-audit.md` pointer is on `:229`, which does not track `:228`.
  - `programs/nvidia_compute_store.cyr:3` — "NOT YET GREEN".
  - `tests/tcyr/native.tcyr:4278` — "Not yet wired into the render composer".

  See the cyrius CHANGELOG [6.6.5] entry "cyrlint read every rule ONE PHYSICAL LINE at a time".
- [ ] The same step can call `cyrius lint --strict-deferrals "$f"` instead of the pinned `cyrlint`
      binary (`ci.yml:61-76`): the wrapper now forwards a flag in any position. See the CHANGELOG
      [6.6.5] entry "The whole CLI took flags as file names, dropped flags written after the
      operand, and dropped extra operands".
- [ ] Cheat-sheet entries A3 (`:92`), A4 (`:110`), B3 (`:164`), C4 (`:240`), D3 (`:360`) and E3
      (`:416`) in `docs/development/2026-04-30-toolchain-issues.md` go stale. Each is fixed in 6.6.5.
- [ ] Retire the workarounds that 6.6.5 makes redundant. Neither script is wired into CI.
  - The measured-clock windowing in `programs/benchmarks.cyr:113-138` (`_clock_jitter_ns`, `:153`):
    `lib/bench.cyr` no longer subtracts a mean floor from a minimum (entry "`lib/bench.cyr`
    subtracted a MEAN clock floor").
  - `scripts/check-ffi-call-alignment.py` and the hoist-C-calls discipline: nested calls are
    16-byte aligned (entry "rsp was 8 bytes off 16-byte alignment at every call emitted inside an
    expression").
  - `scripts/check-split-deferrals.py`: cyrlint joins a wrapped phrase itself (the cyrlint entry
    above).
- [ ] Close the four mirror issues in `docs/development/issues/` (bench floor, nested-call
      alignment, deps tamper, lint wrapper), and the *Upstream cyrius issues* bullet under v4.1.x /
      v4.2. The deps fix is the entry "`cyrius deps` refused an untouched dep cache as
      "tampered"".
- [ ] At the bump, re-run `cyrius deps` — the aarch64 syscall peer moved SYS_UNLINKAT 35 → 263, so
      an un-re-vendored peer's sys_unlink would run nanosleep.

---

## v4.1.x / v4.2 — what the v4.1.0 findings left open

Everything below is a **named** limit now rather than a silent one: each has an error code
a consumer sees from `gpu_shader_error_name`, so nobody has to bisect to find it.

- **Nested divergent ifs** (`GISEL_ERR_NESTED_SELECTION`). Rejected, not miscompiled;
  consumers flatten into two sequential ifs. Supporting them needs a region stack in
  `gfx9_isel` and a second saved-EXEC pair, which trades against the SGPR budget.
- **`OpSelect` with a non-adjacent integer compare** (`LOWER_ERR_SELECT_NOT_ADJACENT`) and
  **with a uniform condition** (`LOWER_ERR_SELECT_UNIFORM_COND`). The first needs the
  compare materialized into a 0/-1 mask at its own site (which needs a pre-pass to know a
  select consumes it); the second needs `s_cselect_b32`.
- ⭐ **The scratch-buffer contract.** `mir_mod_init`, `gfx9_regalloc`,
  `_spirv_resolve_builtins` and the three `spirv_build_*_table` builders each memset a
  caller-supplied extent into a buffer whose size they cannot see. v4.1.0 added a runtime
  self-check for the layout that bit us and a static gate over all 123 call sites; passing
  explicit **lengths** would make the invalid state unrepresentable. `@internal`, so no
  public break — but it touches every call site, hence its own cut.
- ⭐ **Clean-boot native HW closeout (4.1.3 / 4.1.4).** Several fixes were HW-proven only on a
  Cezanne that had already taken MODE2 resets that day (the resets make context-register state
  persist, which hid the render bug; the state-poison harness in `programs/diagnostics/state_poison/`
  was used to test around that). After a fresh boot, with no GPU reset first: `make test-native-all`
  must pass with `NATIVE_KNOWN_FAIL` empty, plus one run of the linked example consumer. On chew,
  the five card-node / DRM-master NVIDIA gates (kms-summary, kms-scanout, kms-modeset, present-e2e,
  surface-present-e2e) need a console session. See `issues/2026-08-19-native-compute-spike-stale.md`.
- **Upstream cyrius issues** (filed 2026-09-16 in `cyrius/docs/development/issues/`; mabda keeps
  its records and workarounds): the cycc nested-call stack misalignment (Low, C interop only;
  mabda hoists and gates with `scripts/check-ffi-call-alignment.py`), `lib/bench.cyr`'s
  min-minus-mean timer floor, the `cyrius deps` tamper check on a stale index, `cyrius lint`
  dropping `--strict-deferrals`, and cyrlint missing deferrals split across two lines. When a fix
  ships, drop the matching mabda workaround.
---

## v3.x+ — Web Target (Blocked)

- **WebGPU / WASM target** — blocked on the Cyrius WASM backend
  landing in the compiler. No version commitment until that
  prerequisite exists. Tracked here so it doesn't get forgotten.

---

## v5.0 — Intel Native Backend (Tentative); NVIDIA wgpu Retires

v5.0 tentatively adds Intel hardware to the native path **and**
retires the wgpu path for NVIDIA. Promoted from "tentative" on the
Intel side once a consumer or AGNOS hardware target with Intel
GPUs makes the work justified — Intel discrete (Arc) and Intel
integrated graphics (Gen12+, Xe) are different bring-up classes.

After v5.0 ships, NVIDIA consumers run on NVIDIA native only; Intel
consumers run on Intel native if v5.0 covers them, otherwise still
on wgpu.

**Likely scope.** Same shape as v4.0 NVIDIA, retargeted:

- **`src/backend_intel.cyr`** filling the v3.0 `Backend` slots.
- **Submission path.** i915 (legacy) or Xe (new). Choice is a v5.0
  design-spike question; Xe is the strategic target if it's stable
  enough by then.
- **Shader ISA.** Gen ISA. WGSL → Gen lowering separately scoped;
  pre-compiled bring-up first.
- **`MABDA_BACKEND_KIND` gains `BACKEND_KIND_INTEL`.**
- **ADR 008** — Intel native, proposed during v5.0 design.

**Exit criteria** — same shape as v4.0 NVIDIA, against Intel
hardware. **NVIDIA wgpu retirement** at v5.0 ship: every
NVIDIA-using consumer has been on the NVIDIA native backend in
production across a full v4.x release cycle. NVIDIA-specific wgpu
wiring is removed (the binding stays in-tree for Intel until v5.1).
**If Intel native slips past v5.0**, the NVIDIA retirement still
ships at v5.0 — Intel native is a separate gate that doesn't block
NVIDIA's progress. v5.0 just becomes "NVIDIA wgpu retires; Intel
native still pending."

---

## v5.1 — Full wgpu Retirement

v5.1 is the last cutover: the Intel wgpu path retires, and with it
the entire wgpu+C scaffolding leaves the tree. Mabda is fully
native-Cyrius across every supported vendor.

**Scope**
- **Retire Intel wgpu wiring.** Conditional on Intel native (v5.0)
  having been in production for a full release cycle on Intel
  hardware. If v5.0 didn't actually ship Intel native, v5.1 doesn't
  ship either — the version is a placeholder for that completion.
- **Remove `src/wgpu_*.cyr`, `src/backend_wgpu.cyr`,
  `deps/wgpu_main.c`, `deps/wgpu-native/`.** The 67-slot wgpu fn
  table and its struct-packing shims go away.
- **Remove the `Backend` indirection**, optionally. If only one
  backend kind remains per vendor, the `ctx->backend` slot is
  redundant; whether to flatten it back into direct calls is a
  design decision at the time, gated on whether multi-vendor
  per-binary becomes a real requirement.
- **macOS / Windows.** v5.1 retirement also forecloses cross-OS
  support unless a non-wgpu story for them exists. AGNOS-on-Mac is
  not a current goal; this is an explicit acceptance, not a
  surprise. If a consumer needs cross-OS support after v5.1 they're
  on the v3.x–v5.0 wgpu line indefinitely — same shipping mechanism
  the rust v1 line uses today.

**Gate.** All three conditions must hold at v5.1 ship:

1. Every vendor mabda supports has a native backend in production
   on its target hardware (AMD ✓ at v3.0, NVIDIA ✓ at v4.0, Intel
   ✓ at v5.0 if it shipped).
2. Every consumer (soorat / rasa / ranga / bijli / aethersafta /
   kiran) is running native on every chipset they target.
3. No new consumer has joined the project that requires a chipset
   the native path doesn't yet cover.

If any of these slips, v5.1 slips with it. The wgpu binding stays
in-tree until all three are met. Retirement is consumer- and
vendor-driven, not calendar-driven.

---

## Backlog (not yet scheduled)

- **`_sk_info_cstr` lives in sakshi's `_sk_` namespace** (recorded by cyrius 6.6.12, 2026-10-02). No
  collision today, but a sakshi helper of that name would silently replace mabda's (last definition wins);
  rename it into mabda's own prefix.

Unscheduled forward work — lands here first, graduates to a version once
there's consumer demand plus a clear scope.

> **Probe before scheduling.** The dev box's GPU is swapped between vendors —
> verify with `lspci` before calling anything HW-blocked. (Standing rule,
> kept after the v4.0.7 NVIDIA multi-BO item graduated out of this list.)

- **NVIDIA block-linear (tiled) scanout + ADDFB2 format modifiers.** `programs/nvidia_kms_scanout.cyr`
  and the shipped nouveau present slots (`_nv_surface_alloc_fb`) force a LINEAR VRAM scanout BO with
  modifier 0 (ADR 007 Fork 4). Scanning out a block-linear BO needs ADDFB2 with
  `DRM_MODE_FB_MODIFIERS` and an NVIDIA block-linear `DRM_FORMAT_MOD`; it matters if render targets
  or scanout buffers move to NVK's block-linear layout. Moved here from a note in
  `nvidia_kms_scanout.cyr` (4.1.3).
- **samvada pure-Cyrius dbus — mabda live-bus validation.** The upstream half is
  done: samvada 1.0 shipped a pure-Cyrius dbus client (`samvada_native_init()` —
  no C shim, no `libsystemd`), and mabda has pinned samvada 1.0.1 since v4.1.2
  (the planned one-line `[deps.samvada]` tag swap). **Remaining mabda-side work:**
  a live-bus end-to-end run of `gpu_surface_configure_native_logind` with
  `samvada_native_init()` from a seated session. No mabda gate builds
  `-D MABDA_LOGIND` today (off by default; the consumer, not mabda, initializes
  samvada). samvada's deletion of its `libsystemd` C shim at samvada 1.1.0 waits
  on that run.
- **Encoder debug-group FFI.** The `debug_*` fns are no-op stubs until
  `wgpuCommandEncoderPushDebugGroup` / `…Pop` land in the fn table — a
  coordinated `deps/wgpu_main.c` launcher change (adding slots forces every
  consumer to rebuild). The 3.4.3 per-node scope wrapping makes them visible in
  profilers once wired.
- **Fully-transparent aliasing backing reuse.** Actually sharing one GPU
  allocation across disjoint-lifetime render-graph transients (rather than the
  consumer applying the planner's offsets) needs a native transient-allocation
  subsystem — wgpu's safe API has no placed/aliased resources, and the native
  path has no rg-transient backing today. Its own future arc.

---

## Architectural Decisions

| ADR | Decision |
|-----|----------|
| [001](../adr/001-gpucontext-public-fields.md) | GpuContext uses accessor functions (`load64` at fixed offsets) |
| [002](../adr/002-runtime-alignment-validation.md) | Runtime alignment check for uniform buffers (bitwise AND) |
| [003](../adr/003-fixed-vertex-types.md) | Fixed vertex types, manual layout, no codegen |
| [004](../adr/004-c-launcher-ffi.md) | C launcher with function table for wgpu-native FFI — coexists with the native backends; leaves the tree at v5.1 |
| [005](../adr/005-public-api-surface-marking.md) | Public API surface marking (`# @public` / `# @internal`) — the stability boundary that holds across backends |
| [006](../adr/006-native-cyrius-gpu-backend.md) | Native Cyrius GPU backend via DRM/KMS — a second backend alongside ADR 004 (AMD v3.0, NVIDIA v4.0); wgpu deprecates/retires per-chipset (AMD **deprecated** at v4.0.1, NVIDIA retires at v5.0, Intel/full at v5.1) |
| [007](../adr/007-nvidia-native-kernel-path.md) | NVIDIA native kernel path (nouveau DRM, SM75 SASS) — the per-vendor expansion of ADR 006 (shipped in v4.0) |

## Moving the cyrius pin to 6.6.6

**Current pin: `cyrius = "6.6.4"` (cyrius.cyml:8).** Nothing in mabda's own
source has to change first — bump the pin.

**Windows is a non-event here, checked not assumed.** README:187 says "Linux is
the only OS the native backend targets"; a grep for `O_APPEND` / `O_TRUNC`
across `src/`, `programs/` and `tests/` (excluding vendored `lib/`) returns
nothing, so 6.6.6's PE append/truncate data-corruption fix does not reach any
mabda code path.

**What 6.6.6 actually touches in this repo:**

- **`WGPU_COMPARE_LESS` is declared twice** — `src/sampler.cyr:35` and
  `src/depth.cyr:76`, both `= 2`, and both are co-linked (`src/lib.cyr:157-158`,
  `cyrius.cyml:62-63`). 6.6.6 makes "last definition wins" true from program
  start and makes a same-name/**different type or size** redeclaration a compile
  error. Both of these are plain i64 scalars with the identical value, so
  **neither rule fires** — verified with a probe against 6.6.2/6.6.5/6.6.6, and
  `cyrius build` under 6.6.6 emits no diagnostic for it. Still a real duplicate;
  collapse it whenever `depth.cyr` is next touched.
- **Everything else in the 6.6.6 tightening list is absent.** mabda declares one
  struct (`vamap`) and uses it pointer-style; there are zero `: <StructName>`
  parameter/variable annotations, zero struct-to-struct copies, zero `async`
  fns, zero `operator` fns, zero top-level `{ }` blocks, and zero
  `: cstring` parameters. The new compile errors have nothing to bite.
- **`lib/regression.cyr` is not vendored here** and nothing calls a
  `regression_*` helper, so the new exec deadline (`CYRIUS_CHECK_TIMEOUT`) is
  irrelevant.
- **`[deps] stdlib` already declares `vec`** and mabda defines no `vec_*` of its
  own, so assert.cyr's new transitive `include "lib/vec.cyr"` cannot collide.

**One thing that is NOT mabda's to fix.** `cyrius fmt --check dist/mabda.cyr`
exits 1 — but not for formatting:

```
cyrfmt: file too large to format-check (>1028KB) — raise _MAX_FILE
```

`dist/mabda.cyr` is 1,368,705 B. Every one of mabda's own `src/*.cyr` passes
`cyrius fmt --check` under 6.6.6; regenerating or reformatting the dist cannot
clear this, because the cap is in cyrfmt. It is the only dist in the sibling
slice over the cap. Per CLAUDE.md's "when a gate blocks a legitimate feature,
fix the gate" — this belongs upstream in cyrius (`_MAX_FILE`), not here. Do not
split `dist/mabda.cyr` to appease it.

**Verify after bumping:** re-run `cyrius deps` so vendored `lib/` picks up
6.6.6's `lib/io.cyr` (now self-sufficient — it includes `fmt`/`string` itself,
which removes five `warning: undefined function` lines for any consumer that
included it alone) and `lib/syscalls_windows.cyr`. Then `cyrius build`,
`cyrius test`, `cyrius fmt --check` over `src/` only, and `cyrius distlib`.
Measured here: `cyrius build` under 6.6.4 and under 6.6.6 both exit 0 with an
identical (empty) warning set and a 1,288,696-byte smoke binary.
