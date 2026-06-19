# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius.
> 38 domain modules, ~12,500 lines, 2102 CPU assertions across 11
> functionality-named domain test files + 11 GPU integration programs +
> 9 CPU benches + 13 GPU benchmarks. `dist/mabda.cyr` bundle at
> ~12,500 lines.
>
> _Header stats current as of v3.1.1 (2026-06-15): mipmaps (3.1.0) +
> multi-queue (3.1.1) shipped, native HW-verified on Cezanne; v3.2 arc
> (texture & shader breadth) planned, `v3.2` branch cut, not yet started._

This document is **forward-looking**. For detail on every shipped
release, see [`CHANGELOG.md`](../../CHANGELOG.md) — that is the
source of truth for completed work. Shipped-section details were
pruned 2026-04-21 when the v2.x line closed so this file stays
useful for planning instead of bloating with history.

## The Long Arc

Mabda is the **public GPU API** for the Cyrius ecosystem. It ships
now via a C shim over wgpu-native so real projects (soorat, rasa,
ranga, bijli, aethersafta, kiran) can depend on a stable,
sovereign-owned surface immediately. In v3.0 a **pure Cyrius DRM/KMS
backend is added alongside** the C launcher path — both coexist,
selectable per consumer, so the same bench suite exercises both. This
gives us a clean A/B across two axes: **(C hooks vs native Cyrius) ×
(pre-5.6.x vs post-5.6.x compiler optimizations)**. The public API
does not change when the native backend lands; consumer code stays
byte-identical.

**Vendor scope of the native backend.** v3.0's native backend is
**AMD-only** at first ship: it talks to the Linux `amdgpu` kernel
driver via direct DRM ioctls, builds GFX9 PM4 packet streams, and
ships pre-compiled GFX9 ISA. Phase B.4 verified the path end-to-end
on AMD Cezanne (gfx90c). Other AMD generations (GFX10/11/12, RDNA*)
share the path but need per-generation bring-up. **NVIDIA support is
scoped to v4.0** (nouveau / nvgpu — different submission path
entirely, no PM4) and **Intel support is tentatively v5.0** (i915 /
Xe — different ISA and command streamer). NVIDIA and Intel hardware
continues to run on the wgpu backend through v3.x, which remains the
cross-vendor default.

```
  v2.0.0 → v2.3.0  ─▶  Cyrius port + Rust-v1 parity + P(-1) audit      (shipped)
  v2.4.0 → v2.4.5  ─▶  v1.0 parity completion + FFI validation +
                        benchmark parity + cache hot-path unblock      (shipped)
  v2.5.0           ─▶  render graph (DAG pass orchestration)           (shipped)
       │
       │                   kernel GPU driver work (parallel, AGNOS scope)
       ▼
  v2.5.x           ─▶  render graph follow-ups (out-of-order toposort,
                        aliasing pass) — driven by consumer demand
  v3.0             ─▶  dual-backend — native Cyrius (AMD/amdgpu/GFX9)
                        added alongside wgpu+C; API unchanged; A/B
                        bench matrix
  v3.1   [SHIPPED] ─▶  mipmaps (3.1.0) + multi-queue (3.1.1) — arc done
  v3.2   [ACTIVE]  ─▶  compressed textures (3.2.0 SHIPPED) + SPIR-V +
                        native SPIR-V→GFX9 compiler (3.2.2–3.2.6) +
                        f64 compute (3.2.4/3.2.7+) — texture/shader breadth
  v3.3             ─▶  image loading (gated on pure-Cyrius decoder)
  v3.x+            ─▶  WebGPU / WASM (blocked on Cyrius WASM backend)
  v4.0             ─▶  NVIDIA native backend added; AMD wgpu path
                        retires (AMD consumers run on AMD native only)
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
**on that vendor's hardware**, the wgpu path is dropped for that
vendor — but the wgpu binding stays in-tree to serve the vendors
whose native backends haven't shipped yet. This keeps the consumer
migration smooth: nobody on a wgpu-only chipset is forced to move
before their native backend is real.

The concrete cutovers:
- **v4.0** — AMD wgpu retires. AMD consumers now run on AMD native
  only. NVIDIA + Intel still on wgpu.
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

## Shipped — one-line history

Source of truth: [`CHANGELOG.md`](../../CHANGELOG.md). Use that for
detail; this table is a jump list.

| Release | Theme |
|---------|-------|
| [2.5.0](../../CHANGELOG.md#250--2026-04-21) | Render graph — DAG pass orchestration, single encoder + single submit, 44 new regression assertions |
| [2.4.5](../../CHANGELOG.md#245--2026-04-21) | Cache hot-path unblock — u64-keyed hashmap migration (cyrius v5.5.20); `bind_group_cache_hit` reaches Rust parity |
| [2.4.4](../../CHANGELOG.md#244--2026-04-21) | Benchmark parity — 13 GPU benches ported, `depth_texture_new` / `rtb_build` latent stubs fixed |
| [2.4.3](../../CHANGELOG.md#243--2026-04-20) | Render-pass FFI + render E2E (closes v1.0 checklist) |
| [2.4.2](../../CHANGELOG.md#242--2026-04-20) | GPU runtime validation — FFI offset / enum sweep; compute E2E actually runs on GPU |
| [2.4.1](../../CHANGELOG.md#241--2026-04-19) | Sakshi observability — opt-in structured logging and spans |
| [2.4.0](../../CHANGELOG.md#240--2026-04-19) | v1.0 parity (partial) — compute E2E program, LOW audit sweep |
| [2.3.0](../../CHANGELOG.md#230--2026-04-19) | P(-1) security audit — 0 CRITICAL / 2 HIGH / 6 MED / 6 LOW across 29 modules |
| [2.2.0](../../CHANGELOG.md#220--2026-04-19) | Scaffolding refresh — flat layout, `cyrius.cyml`, `cyrius distlib` |
| [2.1.2](../../CHANGELOG.md#212--2026-04-12) | Rust-source removal hygiene release |
| [2.1.1](../../CHANGELOG.md#211--2026-04-12) | Stdlib inclusion — `dist/mabda.cyr` single-file bundle |
| [2.1.0](../../CHANGELOG.md#210--2026-04-12) | Feature catch-up — `typed_buffer`, GPU timestamps, texture / render pipeline FFI |
| [2.0.0](../../CHANGELOG.md#200--2026-04-11) | Cyrius port — complete Rust → Cyrius port of every module |
| [1.0.0](../../CHANGELOG.md#100--2026-04-09) | Last Rust release, frozen as the Cyrius-port reference |

---

## v2.5.x — Render graph follow-ups (not yet scheduled)

Everything below is in the out-of-scope list in `docs/guides/render-graph.md`.
None of it is blocking a consumer today; each item ships when a specific
consumer request arrives. Listed so they don't get lost.

- **Full out-of-order toposort.** Today's Kahn implementation only
  counts edges in insertion-order direction; that validates users
  who build graphs in a correct linear order. Programmatic consumers
  that build graphs with cross-inserted dependencies need multi-
  version read/write tracking.
- **Resource aliasing pass.** The `rg_aliasing(g, 1)` flag and the
  `first_use` field on `TransientResource` are already scaffolded.
  Ship the alias analysis that walks transients and reuses backing
  storage between disjoint lifetimes — relevant for consumers with
  dozens of transient buffers per frame.
- **Per-node debug scopes.** Wrap each node's encoding in
  `debug_push(node.label)` / `debug_pop` so GPU profilers
  (RenderDoc, PIX, RGP) show the graph structure.
- **Graph visualizer.** Export a DOT file or similar from the
  toposorted node/edge list. Consumer-requested; not core.

---

## v3.0 — Dual Backend (AMD Native Added Alongside C Path)

v3.0 **adds** a pure Cyrius GPU backend **for AMD hardware**
(amdgpu kernel driver via direct DRM ioctls; GFX9 ISA + PM4 packet
streams; AGNOS kernel-driver integration eventually) **alongside**
the existing wgpu-native + C launcher path. Both coexist; neither is
retired. Consumers on AMD hardware can opt into the native path; all
other consumers continue on wgpu unchanged. The bench suite runs both
on AMD, and the resulting matrix is the evidence base for future
cutover decisions.

**Vendor status going into v3.0:**
- **AMD** — first-class on the native path. GFX9 (Cezanne / gfx90c)
  end-to-end verified Phase B.4 (2026-04-28). Other generations
  (GFX10/11/12, RDNA*) share the path; bring-up per-generation.
- **NVIDIA** — wgpu only in v3.x. Native path scoped to v4.0.
- **Intel** — wgpu only in v3.x. Native path tentative for v5.0.
- **macOS / Windows** — wgpu only across the v3.x line. The native
  backend is Linux-DRM only; non-Linux consumers stay on wgpu.

**Why dual rather than swap.** The C-hooked path is our measurement
baseline. Keeping it in-tree lets us quantify: FFI overhead per call,
native-backend regressions, pre-vs-post-5.6.x O-pass attribution (C
path isolates the cyrius codegen changes from backend architecture
changes), and consumer-by-consumer cutover timing. Retiring the C
path before that data existed would destroy the comparison.

**The one invariant that matters:** the public mabda API (`# @public`
files from v2.1.1) does not change. Consumers who don't opt in to the
native backend see zero behavioural change; consumers who do flip a
single build flag. The `examples/stdlib-consumer/` project is the
regression test — if it still compiles against both backends, the
contract held. The v2.5 render graph must replay unchanged on both.

### Scope (high level, refined closer to the work)
- **Backend abstraction layer** — extract the wgpu-handle dispatch
  points behind an internal `Backend` interface (context/buffer/
  compute/texture/render-pipeline/render-pass entry points). The
  public API stays untouched; the indirection lives under `@internal`.
  Proposal: [`docs/proposals/v3-backend-interface.md`](../proposals/v3-backend-interface.md).
- **AMD/amdgpu DRM backend in pure Cyrius** — no libdrm, no
  libwayland; direct `ioctl(DRM_IOCTL_AMDGPU_*)`. PM4 packet streams
  for the command processor; pre-compiled GFX9 ISA. The shape of
  this work is captured in `src/backend_native.cyr` and
  `programs/native_compute_store.cyr`; B.4 verified
  [2026-04-28](../handoff/2026-04-28-session25-b4-verified.md).
- **WGSL → AMD-ISA lowering path** — required for consumers that
  ship WGSL shaders (most of v2.x). The native backend takes
  pre-compiled GFX9 ISA today; landing a lowering path is in scope
  for v3.0. Cross-vendor lowering (PTX/SPIR-V/Gen) is out of scope
  here — it lives with each vendor's native arc.
- **Backend selector** — `wgpu` (C launcher, default, unchanged for
  existing consumers) vs. `native` (AMD, new, opt-in). Compile-time
  constant in `src/lib.cyr` (per the v3-backend-interface proposal);
  promotable to a `cyrius.cyml` flag in v3.1+ if consumer CI matrices
  need it.
- **Dual-backend bench harness** — `make bench-gpu` runs the
  13-bench suite under each backend on AMD hardware, emits CSV
  columns for both, `bench-history.csv` grows a `backend` column.
- **ADR 006** — pure Cyrius GPU backend via DRM/KMS. **Supplements**
  ADR 004 rather than superseding it; both backends are
  architecturally load-bearing. Now also the precedent for v4.0
  (NVIDIA) and v5.0 (Intel).

### Release cadence (rc.3 / rc.4 / 3.0.0 GA)

The path from rc.2 → 3.0.0 was re-shaped 2026-05-12 to split the
soak window across two rc cuts:

- **rc.3** — ≤6h burn-in proof (the dirty-fast gate). Detail in
  [`3-0-rc-3-punchlist.md`](../archive/punchlists/3-0-rc-3-punchlist.md).
- **rc.4** — 24h → 3-day soak. **24h-clean is the earliest cut
  for 3.0.0 GA.** The 3-day window is the focus of the first few
  3.0.x patches (anything surfacing between 24h and 72h goes to
  the patch backlog, not the GA gate). Detail in
  [`3-0-rc-4-punchlist.md`](../archive/punchlists/3-0-rc-4-punchlist.md).

This split keeps regressions cheap to catch (you spend 6h, not 3
days, before learning something is broken) while preserving the
multi-day observation that informs the post-GA patch stream.

### Exit criteria
- `dist/mabda.cyr` ships with both backends compiled in; default is
  still `wgpu` for API-stability reasons.
- On AMD hardware: `examples/stdlib-consumer/`, `programs/phase0.cyr`,
  `programs/compute_e2e.cyr`, `programs/render_e2e.cyr`,
  `programs/render_graph_e2e.cyr` all pass under both backends.
- On NVIDIA / Intel / macOS / Windows: same programs pass under the
  `wgpu` backend (regression check — native is not expected to run).
- All 1991 CPU assertions + 13 GPU benches pass under both backends
  on AMD (CPU assertions are backend-agnostic and should be untouched).
- soorat / rasa / ranga / bijli / aethersafta / kiran continue to
  build and run under the `wgpu` default on every supported platform;
  at least one consumer runs a CI matrix entry under `native` on AMD
  hardware to prove the path.
- Bench matrix published (AMD only): 13 benches × {wgpu, native} ×
  {pre-5.6.x, post-5.6.x} — four columns per bench in
  `bench-history.csv`, with `docs/benchmarks-rust-v-cyrius.md`
  refreshed to tell the story.

---

## v3.1 — Mipmaps + Multi-queue (Consumer Catch-up)

Both items were backlog'd against v1.0 / v2.x — 3.1 is their natural
home now that the v3.0 backend abstraction exists. They are
**independent** and ship as a sequenced **3.1.x arc**, smallest-risk
first. Planned 2026-06-15 (subsystem map + design decisions); see the
**punchlist** and two **design proposals**:

- Punchlist (archived, shipped): [`3-1-punchlist.md`](../archive/punchlists/3-1-punchlist.md)
- Mipmaps: [`docs/proposals/v3.1-mipmap-generation.md`](../proposals/v3.1-mipmap-generation.md)
- Multi-queue: [`docs/proposals/v3.1-multiqueue.md`](../proposals/v3.1-multiqueue.md)

### 3.1.0 — Mipmap generation (first; self-contained) — ✅ SHIPPED 2026-06-15

On-device mip-chain generation: create a texture with N levels, write
level 0, GPU-downsample levels 1..N-1 (2×2 box filter). Same public API
both backends (`gpu_texture_create_2d_rgba8_mipped` /
`gpu_texture_generate_mipmaps`). wgpu uses texture views + a WGSL
downsample shader; native uses a **hand-authored GFX9 ISA** downsample +
a new image-descriptor-table (V#) path — **no WGSL frontend needed**
(that stays deferred to v3.x). Per-level barriers ride the single queue
via the existing cache-flush PM4 primitive (Step 6.10), so **mipmaps do
not depend on multi-queue**. Consumers: soorat (texture-heavy UI), kiran
(asset pipeline).

### 3.1.1 — Multi-queue coordination (logical ordering abstraction) — ✅ SHIPPED 2026-06-15

Named logical queues (GRAPHICS / COMPUTE / TRANSFER) with explicit
cross-queue ordering. **Hard constraint reckoned with 2026-06-15:**
WebGPU is one-queue-per-device, so real parallelism is impossible on the
wgpu backend. Resolution: multi-queue is an **ordering/dependency
abstraction**, not a parallelism guarantee — wgpu serializes all logical
queues onto its single queue (correct, no overlap); native AMD maps each
to a hardware ring (GFX/COMPUTE/DMA) with timeline-syncobj cross-ring
sync (real overlap). Same public API (ADR 005), backend-specific
performance — the v3.0 story.

**Shipped in 3.1.1 (direct-dispatch MVP, HW-verified on Cezanne):** the
logical queue API (`gpu_queue_get` / `_barrier` / `_wait_idle` +
current-queue dispatch); native GFX + COMPUTE timeline queues with
persistent timeline syncobjs; a cross-ring barrier via an in-CS
`SYNCOBJ_TIMELINE_WAIT` (no CPU stall); and the SDMA `COPY_LINEAR`
foundation (HW-proven on the DMA ring). The 3-ring
compute→barrier→graphics + SDMA-consume e2e
(`programs/native_multiqueue_e2e.cyr`) passes on real hardware. wgpu
aliases the single device queue (submit-order barrier, no overlap).

**Deferred to 3.1.2:** the `TRANSFER → AMDGPU_HW_IP_DMA` ring flip + a
public buffer-copy API (with wgpu parity) — the SDMA path is proven but
a DMA-ring queue with no public copy op would only be a footgun. Plus the
**render-graph multi-queue refactor** (per-node queue affinity +
cross-queue fence edges; the arc's biggest risk — the v2.5 graph is
single-submit). Consumers: rasa (compute+present overlap), bijli (large
transfer).

---

## v3.2 — Texture & Shader Breadth

A sequenced **3.2.x arc** (maintainer decisions 2026-06-15): the roadmap's
texture/shader breadth — **compressed textures → SPIR-V → f64**, SPIR-V/f64
on **both backends** (which pulls the native **SPIR-V → GFX9 compiler**,
Phase N, into the line) — **plus** every item that would otherwise have
been deferred: native compressed *sampling*, the v3.1.2 TRANSFER/buffer-copy
work, and render-graph multi-queue. **Nothing in this arc defers to v3.3 or
v4** — the arc grows to fit (**3.2.0 → 3.2.14**). The two items blocked only
by absent dev-box hardware (ASTC/ETC2 native decode; full-rate `V_MFMA_F64`)
ship their code in-arc with the HW-verification gap flagged in the
punchlist, not dropped. Planned 2026-06-15 (subsystem map + seven design
proposals); see the **punchlist** + **proposals**:

- Punchlist: [`3-2-punchlist.md`](3-2-punchlist.md)
- Compressed textures (storage): [`v3.2-compressed-textures.md`](../proposals/v3.2-compressed-textures.md)
- TRANSFER + buffer-copy: [`v3.2-transfer-copy.md`](../proposals/v3.2-transfer-copy.md)
- Native compressed sampling: [`v3.2-native-compressed-sampling.md`](../proposals/v3.2-native-compressed-sampling.md)
- SPIR-V (wgpu): [`v3.2-spirv-ingestion-wgpu.md`](../proposals/v3.2-spirv-ingestion-wgpu.md)
- SPIR-V → GFX9 native compiler: [`v3.2-spirv-gfx9-native-lowering.md`](../proposals/v3.2-spirv-gfx9-native-lowering.md)
- f64 compute: [`v3.2-f64-compute.md`](../proposals/v3.2-f64-compute.md)
- Render-graph multi-queue: [`v3.2-render-graph-multiqueue.md`](../proposals/v3.2-render-graph-multiqueue.md)

The version map + bite-level steps live in the punchlist. Phase summaries:

### 3.2.0 — Compressed textures, storage (Phase T) — ✅ SHIPPED 2026-06-15

BC1–BC7 / ETC2 / ASTC: format enums + v29 `WGPUTextureFormat` mapping,
block-based size math/validation, format-parameterized create/upload,
capability gating. wgpu is full (create/upload/bind/sample). Native ships
storage + readback this minor; **native compressed *sampling* lands in
Phase TS (3.2.2–3.2.3), in this arc** — it is not deferred. Consumers:
kiran, aethersafta.

### 3.2.1 — TRANSFER ring + public buffer-copy (Phase X)

Absorbs the v3.1.2 carryover. Real native `gpu_buffer_*` (create/write/
read/release are stubs today); `_native_queue_kind_to_ring(TRANSFER)`
flips to `AMDGPU_HW_IP_DMA`; public `gpu_buffer_copy` /
`gpu_queue_transfer_copy` on both backends (native SDMA `COPY_LINEAR`,
already HW-proven; wgpu `copyBufferToBuffer`).

### 3.2.2–3.2.3 — Native compressed (and general) sampling (Phase TS)

Re-introduces the GFX9 **T# image descriptor + S# sampler + tiled surface +
`image_sample`** path the v3.1 mipmap pivot deleted — native AMD can now
*sample* textures. Uncompressed (linear + T#/S#) sampling MVP first
(3.2.2), then the tiled-swizzle compressed path (BC on Cezanne, 3.2.3);
ETC2/ASTC native decode is a flagged HW-verification gap (code ships, cap
bit gated on silicon). Strikes Phase T's native storage-only limitation.

### 3.2.4 — SPIR-V ingestion, wgpu path (Phase S)

An explicit shader **source-kind tag** (WGSL / SPIR-V / GFX9-ISA) on the
byte-polymorphic boundary; `gpu_shader_module_create_spirv` +
`WGPUShaderSourceSPIRV` (codeSize in **words**) + source-kind-aware cache +
the consumer launcher gate. Native SPIR-V is Phase N. HW e2e is render-path
(wgpu compute is still a v3.0 stub).

### 3.2.5–3.2.9 — SPIR-V → GFX9 native compiler (Phase N)

**The dominant, highest-risk effort** — an in-tree, compute-only,
GFX9/Cezanne-only SPIR-V → GFX9 compiler behind the existing shader slot
(no new slot), its own AGNOS package. Staged: encoder-lift + byte-oracle
(N.0) → parser (N.1) → MIR + uniformity (N.2) → instruction selection
(N.3) → regalloc + waitcnt (N.4) → **MVP: recompile the downsample shader
from SPIR-V and byte-match it** (N.5) → novel kernel (N.6) → control flow
(N.7) → op breadth (N.8). A compiler emits bytes no human checked for
inputs no human saw — the entire mitigation is *manufactured oracles*. The
hand-authored shaders stay as oracle + fallback.

### 3.2.12 — f64 (double-precision) compute (Phase F) — SHIPPED

Consumer: **attn11**. f64 hard-gates on SPIR-V on both backends. **wgpu f64
route (HW-verified on Cezanne, F.8b 2026-06-18):** request the native
`WGPUNativeFeature_ShaderF64` (0x0003001D) at device creation and feed f64
SPIR-V through the ordinary `gpu_shader_module_create_spirv` path — naga's
SPIR-V frontend accepts f64 once ShaderF64 is on. (The base `WGPUFeatureName`
enum + WGSL still have no f64, gpuweb #2805 — but the *native* ShaderF64 bit
exists; the earlier "no ShaderF64 / only `SpirvShaderPassthrough`" framing was
wrong, and passthrough is in fact unsupported on RADV/Cezanne — it's kept only
as a general raw-SPIR-V escape hatch.) **The native route is the Phase N
SPIR-V→GFX9 compiler emitting `V_*_F64`** — and at 3.2.12 it ships: general
native f64 routes consumer SPIR-V through Phase N (no longer 3.2.13), HW-verified
bit-exact on Cezanne. `MABDA_NATIVE_F64 = 1`. The shipped toolkit is the
conformant f64 **arithmetic** set — add/sub/mul/div (correctly-rounded
reciprocal-Newton)/sqrt (correctly-rounded, full-range)/fma, i32↔f64 + f32↔f64
convert, OpConstant, Ldexp, ordered compares, OpSelect. **Transcendentals are
arithmetic-only**: GLSL.std.450 Exp/Log/Pow are 16/32-bit-only (spirv-val rejects
f64), so consumers hand-roll exp/ln/tanh/pow as polynomials over the toolkit (a
complete degree-13 exp macro was backed out as uninvocable). Correctness, not
speed, is the deliverable: f64 is ~1:16–1:32 on Cezanne (the throttled FP64 ALU —
f64 is for correctness-critical consumer math, not throughput); full-rate
`V_MFMA_F64` needs CDNA (no dev-box silicon → code ships, HW-verification flagged
in the punchlist).

### 3.2.13–3.2.14 — Render-graph multi-queue scheduling (Phase R)

Absorbs the v3.1.2 carryover. The v2.5 render graph is single-submit; this
adds per-node queue affinity + cross-queue fence edges (lowered to the
v3.1.1 `gpu_queue_barrier` in-CS timeline waits) + per-queue submit
batching — composing the existing queue primitives (no new Backend slots).
Multi-queue is opt-in (omitting affinity reproduces v2.5 ordering exactly);
wgpu is serialized-equivalent, native gets real overlap.

---

## v3.3 — Asset Loading

- **Image loading (PNG / JPEG)** — adopted only once a pure-Cyrius
  decoder exists in the AGNOS ecosystem. **Dependency-gated**: if
  no decoder ships by the 3.3 window, this moves to 3.4+. No C
  image library will be vendored to force the issue.

---

## v3.x+ — Web Target (Blocked)

- **WebGPU / WASM target** — blocked on the Cyrius WASM backend
  landing in the compiler. No version commitment until that
  prerequisite exists. Tracked here so it doesn't get forgotten.

---

## v4.0 — NVIDIA Native Backend; AMD wgpu Retires

v4.0 does two things: adds NVIDIA hardware to the native path **and**
retires the wgpu path for AMD. After v4.0, AMD consumers run on the
AMD native backend only; NVIDIA and Intel consumers continue on
wgpu. The wgpu binding code itself stays in-tree to serve those
remaining vendors.

Until v4.0 ships, NVIDIA consumers stay on the wgpu backend and AMD
consumers can run on either backend.

**What's different from v3.0's AMD work.** Almost everything below
the `Backend` interface:

- **Submission path.** No PM4. NVIDIA's command streamer takes a
  different packet format (Maxwell+ class methods over channels);
  the kernel-side abstraction is also different — either nouveau
  via DRM, or nvidia.ko via `/dev/nvidia*` character devices.
  Choice between nouveau (open-source, integrates with the existing
  DRM ioctl pattern) and nvidia.ko (proprietary, performant, but a
  C library / non-Cyrius dependency we'd be re-importing) is a
  v4.0 design-spike question.
- **Shader ISA.** No GFX9. NVIDIA shaders ship as PTX (a portable
  IR — needs JIT) or as SASS (per-SM-class assembly). The WGSL → ISA
  lowering path from v3.0 informs the architecture but the back end
  is fully replaced.
- **Memory model.** NVIDIA's BAR / staging behavior diverges from
  AMD's (ReBAR semantics, CUDA-style unified memory). The
  `gpu_memory_pooling` allocator decisions from v3 may need a
  vendor-specific tuning pass.

**Scope (refined closer to the work)**
- **`src/backend_nvidia.cyr`** — fills the same `Backend` slots that
  `src/backend_native.cyr` (AMD) does. The interface from v3.0 is
  the contract; if a slot is too AMD-shaped, that's an interface
  bug to revise, not a work-around.
- **NVIDIA `programs/nvidia_compute_store.cyr` analogue** — same
  proof-of-life flow that gated v3.0 B.4: dispatch → CPU readback
  of `0xDEADBEEF`. Different driver path; same proof.
- **`MABDA_BACKEND_KIND` gains `BACKEND_KIND_NVIDIA`.** Backend
  selection is still compile-time; `cyrius.cyml` flag remains an
  option if consumer CI matrices need runtime-ish selection.
- **Dual-backend bench harness** widens to a 4-axis matrix: vendor
  × {wgpu, native} × {pre-/post-5.6.x} × bench. AMD's table doubles;
  NVIDIA gets its own.
- **ADR 007** — proposed during v4.0 design. Documents the
  nouveau-vs-nvidia.ko decision and the SASS/PTX choice.

**Exit criteria**
- All v3.0 exit programs pass under `native` on NVIDIA hardware
  (specific generation TBD during v4.0 scoping; likely Ampere or
  Lovelace as the bring-up class).
- soorat (smoke-test consumer) builds and runs under
  `BACKEND_KIND_NVIDIA` in CI on NVIDIA hardware.
- The NVIDIA work doesn't regress AMD or wgpu paths.
- **AMD wgpu retirement**: every AMD-using consumer has been on the
  AMD native backend in production across a full v3.x release cycle.
  At v4.0 ship, the AMD code paths in `src/wgpu_*.cyr` /
  `src/backend_wgpu.cyr` either get a `BACKEND_KIND_AMD` guard that
  errors out, or the AMD-specific wgpu wiring is removed
  (the wgpu *binding* stays for NVIDIA + Intel; AMD consumers no
  longer have a wgpu route).
- **`samvada` C-shim retirement.** The `samvada` package's
  v3.x C shim around `libsystemd`'s `sd_bus_*` (mirroring
  mabda's `wgpu_main.c` pattern) gets replaced by a pure-Cyrius
  dbus marshaller, OR removed entirely if the v4.0 logind story
  evolves (e.g., kernel-level master delegation lands in a way
  that obviates dbus). Treated as a peer of the wgpu-native
  retirement: both C-shim deps for v3.x feature surfaces drop
  together at v4.0. If the pure-Cyrius dbus implementation
  isn't ready, the C shim can survive into v4.x — but the
  commitment is "both C-shim deps go at v4.0 if we ship them
  in v3.x."

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

Empty. New items land here first and graduate to a version once
there's consumer demand plus a clear scope.

---

## v1.0.0 Criteria (Rust v1.0 feature parity in Cyrius)

All mabda-side criteria complete as of v2.4.3. The last open item is
consumer-side.

- [x] All core modules ported (30 as of v2.5.0 — v1.0 had 25).
- [x] GPU context creation working.
- [x] Buffer create / write / release working.
- [x] Buffer readback round-trip (write → copy → map → verify) —
      `programs/phase0.cyr`.
- [x] Texture create / view / upload / release — `programs/phase0.cyr`.
- [x] Render pipeline create / release — `programs/phase0.cyr`.
- [x] Compute dispatch end-to-end test — `programs/compute_e2e.cyr`
      (v2.4.0 wrote; v2.4.2 got it running on real hardware).
- [x] Render pipeline end-to-end draw + readback —
      `programs/render_e2e.cyr` (v2.4.3).
- [ ] Consumer integration: soorat port to Cyrius (consumer-side,
      tracked in soorat's repo).

---

## Architectural Decisions

| ADR | Decision |
|-----|----------|
| [001](../adr/001-gpucontext-public-fields.md) | GpuContext uses accessor functions (`load64` at fixed offsets) |
| [002](../adr/002-runtime-alignment-validation.md) | Runtime alignment check for uniform buffers (bitwise AND) |
| [003](../adr/003-fixed-vertex-types.md) | Fixed vertex types, manual layout, no codegen |
| [004](../adr/004-c-launcher-ffi.md) | C launcher with function table for wgpu-native FFI — permanent, coexists with native backend from v3.0 onward |
| [005](../adr/005-public-api-surface-marking.md) | Public API surface marking (`# @public` / `# @internal`) — the stability boundary that holds across backends |
| [006](../adr/006-native-cyrius-gpu-backend.md) | Native Cyrius GPU backend via DRM/KMS — adds a second backend alongside ADR 004 during v3.x; wgpu path retires at v4.0 (Proposed, v3.0 design) |
