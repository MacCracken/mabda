# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius. **Three backends behind one
> public API**: wgpu-native (cross-vendor default), native AMD (amdgpu DRM /
> GFX9 / PM4), and native NVIDIA (nouveau DRM, Turing/SM75). Baseline:
> **v4.0.7** (2026-07-16). Module/assertion/bundle counts live in the
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
the constant-cache dispatch fix, HW-proven on the TU116).

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
  v2.0.0 → v4.0.7  ─▶  shipped — see CHANGELOG.md (Cyrius port → dual
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
  `deps/wgpu_main.c`, `deps/wgpu-native/`.** The 65-slot wgpu fn
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

- **samvada C-shim → pure-Cyrius dbus.** The samvada `libsystemd` C-shim
  retirement (paired with the AMD-wgpu step) was **deferred at v4.0.1** under
  the roadmap escape hatch. Forward plan: evolve the samvada dbus project into
  a pure-Cyrius native dbus client (AF_UNIX system bus + SASL EXTERNAL +
  SCM_RIGHTS master-fd passing); when samvada ships pure-Cyrius 1.0, mabda
  swaps via a one-line `[deps.samvada]` tag bump + `cyrius deps`, then drops
  libsystemd. mabda's coupling is dormant today (`#ifdef MABDA_LOGIND`, off by
  default) — call signatures are impl-agnostic.
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
- ~~**NVIDIA multi-BO / bigger-BO surfaces.**~~ **Shipped at v4.0.7** — the
  multi-size BO allocator (surface-derived 64 KiB-aligned spans, the AMD
  `_native_align_up` idiom) plus the constant-cache-invalidate dispatch fix its
  HW gate surfaced. HW-proven on the TU116 (`make test-nvidia-bigbo-e2e`); see
  `CHANGELOG.md` [4.0.7]. Kept here one cycle as the graduation record — the
  probe-before-scheduling rule stands (the dev box's GPU is swapped between
  vendors; verify with `lspci` before calling anything HW-blocked).

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
