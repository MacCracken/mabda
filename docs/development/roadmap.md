# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius. **Three backends behind one
> public API**: wgpu-native (cross-vendor default), native AMD (amdgpu DRM /
> GFX9 / PM4), and native NVIDIA (nouveau DRM, Turing/SM75). Baseline:
> **v4.3.0** (2026-11-16). Module/assertion/bundle counts live in the
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
leak, shader-cache / depth-texture release, and the bench and launcher leftovers; v4.1.5
(2026-09-27) moved to cyrius 6.6.6 and made the bundle build for agnos; v4.1.6 (2026-09-30)
renamed two names that collided with other cyrius folds; v4.1.7 (2026-10-06) moved to cyrius
6.6.18 and its compile-verified `dist/mabda.deps`; v4.2.0 (2026-10-08) moved to cyrius 6.7.5,
made the eleven `F64_*` colour constants f64 `const`s, and closed the 6.6.5 / 6.6.6 pin-move
lists below, which it pruned; v4.3.0 (2026-11-16) shipped the render tier's MB1 — the pipeline
descriptor, bind groups, draw-time calls and the error channel, on an 81-slot launcher — and
fixed the 2026-11 filings: the render-loop leak, the pin and optional deps, the profiler's pass
timings, NVIDIA's shader-kind gate and binding array, and the byte-loop copies; v4.3.1
(2026-11-16) shipped MB2–MB5 — depth and shadows; render formats, MSAA, views, mip generation,
copies and readback; compute on wgpu; a Wayland window surface — on an 83-slot launcher).

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
  v2.0.0 → v4.3.1  ─▶  shipped — see CHANGELOG.md (Cyrius port → dual
                        backend → texture/shader breadth → asset loading →
                        array/cube textures → NVIDIA native → AMD-wgpu
                        deprecation → render tier MB1 → MB2–MB5)
  v4.4 → v4.x      ─▶  N9 (SPIR-V → SASS), the texture encoder, the native
                        render tier (below)
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

## The render tier — MB1 to MB5 (shipped 4.3.0 / 4.3.1)

Filed 2026-11-15 by soorat's Cyrius port (`issues/2026-11-15-render-tier-for-geometry-consumers.md`,
its ledger L2–L6). **MB1 shipped in 4.3.0** (the pipeline descriptor, bind groups, draw-time calls,
public constant names, the error channel); **MB2–MB5 shipped in 4.3.1** (CHANGELOG [4.3.1]) on an
83-slot launcher — the 4.3.0 revision lacked the compute pass encoder's release and the compute
pipeline's bind group layout, added as slots 81–82 (the one further consumer rebuild, approved
2026-11-16):

- **MB2 — depth and shadows.** Depth32F render targets and textures, the pass descriptor's depth
  attachment (load / clear / store / read-only), depth-only passes, a depth target bound as a
  `texture_depth_2d` for a comparison sampler (HW: `programs/target_tier_e2e.cyr` part A; soorat
  P6/MB2-5 `shadow`).
- **MB3 — formats, MSAA, views, mips, copies, readback.** Textures and targets in RGBA8 sRGB,
  BGRA8 (+ sRGB), RGBA16F; KTX2 / DDS containers of those map to their own ids (punchlist AL.0 for
  the uncompressed formats; the compressed sRGB variants still collapse, flagged by
  `mabda_*_format_is_srgb`); 4x MSAA with automatic or explicit resolve; up to eight colour
  targets; texture views (a cube face or mip level as an attachment; bindable; readable); mipped
  cubes; mip generation on wgpu (a render blit per level, any uncompressed colour format, arrays
  and cubes); texture-to-texture copies; render-target readback (part B; soorat `hdr`, `ibl`,
  `copy`, `msaa`).
- **MB4 — compute on wgpu.** Compute pipelines from a descriptor with explicit mixed layouts,
  compute passes, `gpu_compute_dispatch` on wgpu, storage readback, f64 end to end
  (`programs/compute_tier_e2e.cyr`; soorat `particles`).
- **MB5 — a window surface.** A `WGPUSurface` from a Wayland `wl_display` / `wl_surface`,
  capabilities, configure in any listed format with AUTO_VSYNC / AUTO_NO_VSYNC, frames as texture
  objects, resize. **HW proof open:** `programs/surface_wayland_e2e.cyr` (a Wayland client driven
  from Cyrius) needs a running compositor; neither the Cezanne dev box (a tty session) nor `chew`
  (an SDDM greeter) has one, and Hyprland will not start headless there without a seat. The
  program loads libwayland-client through the process's libc and SKIPs at `wl_display_connect`;
  run it from a Wayland session (`make test-surface-wayland-e2e`).
- **The native render tier** (soorat's ledger L7, after its 2.0). On native AMD and NVIDIA the
  tier serves the native render path's shape — a vertex-less, unbound draw into one single-sample
  RGBA8 target, a cleared one-attachment descriptor pass, and its readback (4.3.1) — and names
  everything else `GPU_ERR_NOT_IMPLEMENTED` (`src/backend_native_rtier.cyr`). The rest  #skip-lint
  (vertex fetch, bind groups, depth, blending) needs a render-capable native shader path: a WGSL
  front end, or SPIR-V render lowering beside the compute lowering. It bears on the v5.1 gate,
  which lists soorat among the consumers that must run native.

---

## N9 — SPIR-V → SASS for the NVIDIA backend (ranga waits on it)

Filed 2026-11-02 (`issues/2026-11-02-n9-spirv-to-sass-compiler-request.md`); listed under "Next"
since 4.0.0. The NVIDIA backend takes only pre-captured SASS, and a portable consumer ships
SPIR-V. The compiler is the analogue of `spirv_lower` → `gfx9_*`: the SPIR-V parse and the MIR are
reused; new for SM75 are instruction selection, register allocation, encoding and the control
words (stall counts, barriers, yields) that ptxas computes and the captured kernels carry. Its
modules set `NV_SHMOD_NBIND`, which 4.3.0's NVIDIA binding array already reads. Done when ranga's
11 kernels compile through `gpu_shader_module_create_spirv` on an NVIDIA context and its four GPU
suites pass on the TU116. A design note in `docs/proposals/` comes first.

---

## v4.2.x / v4.3 — what the v4.1.0 findings left open

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
  surface-present-e2e) need a console session. See `issues/archived/2026-08-19-native-compute-spike-stale.md`.

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
  `deps/wgpu_main.c`, `deps/wgpu-native/`.** The 81-slot wgpu fn
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
- **GPU texture compression — BC1–BC7 / ASTC encode** (filed 2026-10-13 from sankoch; the user placed
  it in mabda, not sankoch; scheduled after N9 in the 2026-11 plan, with KTX2 zstd / zlib
  supercompression decode as its first, concrete half). A CPU reference block encoder for the first format a consumer names,
  validated against an independent decoder and mabda's own upload-and-sample path, with a PSNR bar.
  sankoch's zstd / zlib already unlock KTX2 supercompression schemes 2 and 3 in `_ktx2_parse`.
  Details: `docs/development/issues/2026-10-13-gpu-texture-compression-encoder.md`.
- **Fully-transparent aliasing backing reuse.** Actually sharing one GPU
  allocation across disjoint-lifetime render-graph transients (rather than the
  consumer applying the planner's offsets) needs a native transient-allocation
  subsystem — wgpu's safe API has no placed/aliased resources, and the native
  path has no rg-transient backing today. Its own future arc.

### Recorded by the W2 stdlib wave (4.2.0, 2026-10-08)

The cyrius 6.7.x features each change a public name's kind or type, so each is
its own minor, not a patch. 4.2.0 took only the eleven colour constants (S1).

- **S2: integer constants as `const`.** About 1,570 top-level `var NAME = <literal>`
  constants (WebGPU / AMDGPU / NVIDIA enums, ioctl numbers, offsets) could be
  `const`: folded like a literal, no storage, no init order. It needs a per-name
  collision survey first: a `const` beside a same-name `var` is a compile error,
  and cyrius folds mabda next to its own `lib/` and the other folded stdlibs.
- **S3: `bool` predicates.** The `*_is_*` / `*_has_*` returns become `: bool`
  (cyrius 6.7.3).
- **S4: if-expressions where they read better** (cyrius 6.7.4). The W2 survey
  flagged about 77 `var x = a; if (c) { x = b; }` and `if/else` assignment sites,
  e.g. `src/asset_load.cyr`'s mip / layer / face-count clamps.
- **S7: a `trait Backend`** for the hand-rolled `backend_get_slot` /
  `BACKEND_SLOT_*` dispatch, once cyrius ships checked `dyn`.
- **M6: retire the measured-clock windowing** in `programs/benchmarks.cyr`
  (`_clock_jitter_ns`). It worked around `lib/bench.cyr`'s mean-floor minimum,
  which cyrius 6.6.5 fixed. Needs a `make bench-gpu` run before and after.
- **M10: samvada `sys_recvmsg` on Windows.** `cyrius distlib` warns that
  `sys_recvmsg (x86_64-windows)` is still undefined. It comes from samvada's
  bundle, which mabda calls only under `-D MABDA_LOGIND` (a Linux logind path).
  To be filed in samvada once samvada's tree is clean (it was not at the survey).
- **M11: `cyrius fmt --check dist/mabda.cyr` refuses the file** (>1028 KB,
  cyrfmt's `_MAX_FILE`; the bundle is 1,370,812 B at 4.2.0). It is cyrius's cap to
  raise, not a reason to split the bundle. CI fmt-checks the sources only.

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
