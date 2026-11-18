# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius. **Three backends behind one
> public API**: wgpu-native (cross-vendor default), native AMD (amdgpu DRM /
> GFX9 / PM4), and native NVIDIA (nouveau DRM, Turing/SM75). Baseline:
> **v4.4.0** (2026-11-17). Module/assertion/bundle counts live in the
> filesystem + `CHANGELOG.md`, not here — they go stale on every cut.

This document is **forward-looking**: every planned release is pinned below, in order.
[`CHANGELOG.md`](../../CHANGELOG.md) is the source of truth for shipped work, and shipped
sections are pruned from this file when their arc closes (v2.x pruned 2026-04-21; v3.0–v3.4
2026-06-19; v4.0–v4.0.2 2026-07-02; v4.0.3–v4.2.0 and the render tier MB1–MB5, v4.3.0–v4.3.1,
2026-11-16; v4.4.0, compilers + NVIDIA, 2026-11-17).

The current agent hand-over — repo state, how to verify, the boxes, the gotchas — is
[`handoff.md`](handoff.md).

## The Long Arc

Mabda is the **public GPU API** for the Cyrius ecosystem. It shipped
first via a C shim over wgpu-native so real projects (soorat, rasa,
ranga, bijli, aethersafta, kiran) could depend on a stable,
sovereign-owned surface immediately. At v3.0 a **pure-Cyrius DRM/KMS
backend landed alongside** the C launcher path (AMD / amdgpu / GFX9);
at v4.0 a **second native backend** landed (NVIDIA / nouveau / SM75).
All coexist, selectable per consumer, so the same bench suite exercises
each. The public API does not change across backends; consumer code
stays byte-identical.

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
  v2.0.0 → v4.4.0  ─▶  shipped — see CHANGELOG.md
  v4.5.0           ─▶  textures + language: texture encoding, compressed sRGB ids,
                        render-graph aliasing, the cyrius 6.7 language minors
  v5.0             ─▶  native rendering beyond the vertex-less subset; Intel native
                        (tentative); NVIDIA wgpu path retires
  v5.1             ─▶  full wgpu retirement — wgpu-native + C launcher + the FFI
                        binding code leave the tree
  blocked upstream ─▶  WebGPU / WASM, trait Backend, two toolchain / samvada items —
                        each lands in the first mabda release after its blocker clears
```

Every item the roadmap carries is in one of these. Version numbers are the order of work,
not dates. A release that finds it needs a new launcher slot asks first: every slot forces
every consumer to rebuild its launcher copy (83 slots since 4.3.1).

### wgpu retirement policy

Wgpu retirement is **per-chipset, not all-at-once**. Each major
vendor gets a release window where wgpu and native coexist for that
vendor (so existing consumers can flip on their schedule). Once a
vendor's native backend has been in production for a release cycle
**on that vendor's hardware**, the wgpu path is first *deprecated* for
that vendor (a one-shot warning, but it still works — an escape hatch,
with an opt-in strict flag to enforce), then dropped in a later release
— but the wgpu binding stays in-tree to serve the vendors
whose native backends haven't shipped yet.

The concrete cutovers:
- **v4.0.1** — AMD wgpu **deprecated** (one-shot warning; still works by
  default; `-D MABDA_AMD_WGPU_STRICT` enforces native-only). Full
  AMD-wgpu retirement is deferred to a later release.
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
**backend correctness**. The public surface is the load-bearing
contract. The FFI layer underneath is explicitly marked internal
(`@internal` header on line 1 of every FFI module) so no consumer
accidentally couples to it.

---

## v4.5.0 — textures + language

Phase 4 of the 2026-11 plan with the texture, render-graph and language items already on the
roadmap.

- **Texture encoding** (`issues/2026-10-13-gpu-texture-compression-encoder.md`; filed from sankoch,
  placed in mabda by the user). ✅ First **KTX2 supercompression decode** — schemes 2 (zstd) and
  3 (zlib), through sankoch's codecs: done (CHANGELOG [4.5.0]). The consumer registers the
  decoders, so mabda's bundle never names sankoch; the result is byte-exact in the GPU BO on the
  Cezanne. ✅ Then **a CPU
  reference block encoder**: BC7, chosen by the maintainer (2026-11-17), done (CHANGELOG
  [4.5.0]). It is checked against the Cezanne's texture unit on every mode, with a PSNR bar
  recorded in `tests/tcyr/texture_bc7.tcyr`. The item as filed was for the
  first format a consumer names (BC1 / BC3 / BC4 / BC5 / BC7, or ASTC 4×4 / 6×6 / 8×8), validated
  against an independent decoder and mabda's upload-and-sample path, with a PSNR bar; if no
  consumer has named one by then, ask the user.
- ✅ **Compressed sRGB ids** — done (CHANGELOG [4.5.0]). The BC1/3/7, ETC2 and ASTC sRGB
  container variants collapsed to the UNORM id; they have their own ids (17–24) now, as the
  uncompressed ones got in 4.3.1, and wgpu samples them with the sRGB decode. At the
  maintainer's call (2026-11-17) native AMD decodes too: every sRGB id it samples (RGBA8, BC1,
  BC3, BC7 sRGB) takes NUM_FORMAT_SRGB, checked on the Cezanne by `test-native-srgb-e2e` and
  matched by wgpu's `test-wgpu-srgb-sample-e2e`. That is the sampling half of v5.0's native sRGB
  decode, pulled forward.
- ✅ **Render-graph aliasing backing reuse** — done (CHANGELOG [4.5.0]). `rg_build_ctx` gives a
  native AMD graph's transients one GTT allocation (buffers, and RGBA8 textures usable as targets
  and sampled), and with aliasing on, disjoint-lifetime transients share bytes. The executor
  waits, before a reusing node, for every queue that used the slot. Checked on the Cezanne by
  `test-native-rg-alias-e2e`, whose independent-nodes phase fails without the wait. Found and
  fixed on the way: a queue held one pending wait (a second barrier replaced the first);
  same-queue edges relied on submit order; the planner sized every texture as RGBA8. wgpu
  keeps today's behaviour (its safe API has no placed or aliased resources). Native NVIDIA
  has no render-graph executor (`rg_build_ctx` / `rg_execute_mq` return
  GPU_ERR_NOT_IMPLEMENTED); the executor is scheduled in v5.0 below, with the NVIDIA wgpu
  retirement it blocks.
- **The cyrius 6.7 language minors** (recorded by the W2 stdlib wave, 4.2.0; each changes a public
  name's kind or type, which is why they ride a minor):
  - S2: about 1,570 top-level integer `var NAME = <literal>` constants become `const`, after a
    per-name collision survey (a `const` beside a same-name `var` is a compile error, and cyrius
    folds mabda next to other stdlibs);
  - S3: the `*_is_*` / `*_has_*` predicates return `: bool` (cyrius 6.7.3);
  - S4: if-expressions at the ~77 assignment sites where they read better (cyrius 6.7.4);
  - M6: retire the measured-clock windowing in `programs/benchmarks.cyr` (`_clock_jitter_ns`),
    with a `make bench-gpu` run before and after.

---

## v5.0 — Native Rendering; Intel Native Backend (Tentative); NVIDIA wgpu Retires

v5.0 tentatively adds Intel hardware to the native path **and**
retires the wgpu path for NVIDIA. Promoted from "tentative" on the
Intel side once a consumer or AGNOS hardware target with Intel
GPUs makes the work justified — Intel discrete (Arc) and Intel
integrated graphics (Gen12+, Xe) are different bring-up classes.

**Native rendering beyond the vertex-less subset — first, because the NVIDIA retirement
needs it.** On native AMD and NVIDIA the render tier serves a vertex-less, unbound draw into
one single-sample RGBA8 target, a cleared one-attachment descriptor pass, and its readback;
everything else is `GPU_ERR_NOT_IMPLEMENTED` (`src/backend_native_rtier.cyr`). A consumer that  #skip-lint
draws geometry (soorat, and kiran through it) cannot leave wgpu on NVIDIA until vertex fetch,
bind groups, depth, blending, MSAA, the render formats, views and copies run natively. That
needs a render-capable native shader path (a WGSL front end, or SPIR-V render lowering beside
the compute lowering), plus native BGRA8 / RGBA16F sampling and sRGB render targets (the
sRGB decode on sampling landed in 4.5.0).

**A native NVIDIA render-graph executor** (found in 4.5.0; placed here because a graph
consumer cannot leave wgpu on NVIDIA without it). `rg_execute_mq` runs native graphs on AMD only,
and `rg_build_ctx` backs transients on AMD only; on an NVIDIA context both return
`GPU_ERR_NOT_IMPLEMENTED` (until 4.5.0, `rg_execute_mq` fell into the wgpu path there). It  #skip-lint
needs NVIDIA's queue timelines and barriers under the same per-node waits, and the transient
views over one NVIDIA allocation.

**NVIDIA scanout follow-ons** (left out of 4.4.0's block-linear scope, "swizzle at present",
2026-11-16; scheduled here at the 4.4.0 cut because they need native render targets):
- **Block-linear render targets and textures**, so a frame rendered on the GPU scans out with no
  copy (zero-copy present).
- **A copy-engine present** (`native_nv_push_ce_copy_bl`, already HW-checked in
  `test-nvidia-blocklinear-e2e`) in place of the CPU swizzle, which costs about 15 ms a frame
  at 1440p (5.9 ms at 1080p).

**Intel — likely scope.** Same shape as v4.0 NVIDIA, retargeted:

- **`src/backend_intel.cyr`** filling the `Backend` slots.
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
production across a full v4.x release cycle (N9, in 4.4.0, is what
lets a compute consumer get there; native rendering, above, a drawing one). NVIDIA-specific wgpu wiring is removed
(the binding stays in-tree for Intel until v5.1). **If Intel native
slips past v5.0**, the NVIDIA retirement still ships at v5.0.

---

## v5.1 — Full wgpu Retirement

The last cutover: the Intel wgpu path retires, and with it the entire
wgpu+C scaffolding leaves the tree. Mabda is fully native-Cyrius
across every supported vendor.

**Scope**
- **Retire Intel wgpu wiring.** Conditional on Intel native (v5.0)
  having been in production for a full release cycle on Intel
  hardware. If v5.0 didn't actually ship Intel native, v5.1 doesn't
  ship either — the version is a placeholder for that completion.
- **Remove `src/wgpu_*.cyr`, `src/backend_wgpu*.cyr`,
  `deps/wgpu_main.c`, `deps/wgpu-native/`.** The 83-slot wgpu fn
  table and its shims go away.
- **Remove the `Backend` indirection**, optionally — a design
  decision at the time, gated on whether multi-vendor per-binary
  becomes a real requirement.
- **macOS / Windows.** v5.1 forecloses cross-OS support unless a
  non-wgpu story for them exists. AGNOS-on-Mac is not a current
  goal; a consumer needing cross-OS after v5.1 stays on the
  v3.x–v5.0 wgpu line.

**Gate.** All three must hold:

1. Every vendor mabda supports has a native backend in production
   on its target hardware (AMD ✓ at v3.0, NVIDIA ✓ at v4.0, Intel
   ✓ at v5.0 if it shipped).
2. Every consumer (soorat / rasa / ranga / bijli / aethersafta /
   kiran) is running native on every chipset they target.
3. No new consumer has joined that requires a chipset the native
   path doesn't yet cover.

---

## Blocked upstream — each lands in the first mabda release after its blocker clears

- **WebGPU / WASM target** — needs the Cyrius WASM backend in the compiler.
- **S7: a `trait Backend`** for the hand-rolled `backend_get_slot` / `BACKEND_SLOT_*` dispatch —
  needs cyrius to ship checked `dyn`.
- **M10: samvada's `sys_recvmsg` is undefined on x86_64-windows** (`cyrius distlib` warns; mabda
  reaches it only under `-D MABDA_LOGIND`) — to be filed in samvada once its tree is clean.
- **M11: `cyrius fmt --check dist/mabda.cyr` refuses the file** (>1028 KB, cyrfmt's `_MAX_FILE`) —
  cyrius's cap to raise; CI fmt-checks the sources only.

New work lands in the release it belongs to, or here with its blocker named. Before calling
anything hardware-blocked, check the box (`lspci`): the dev box's GPU has been swapped between
vendors.

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
