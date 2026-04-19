# Mabda — Development Roadmap

> GPU foundation layer for AGNOS. Written in Cyrius.
> 29 domain modules, ~4,000 lines, 286 CPU assertions + 1 GPU
> integration test. `dist/mabda.cyr` bundle at 4,093 lines.

## The Long Arc

Mabda is the **public GPU API** for the Cyrius ecosystem. It ships
now via a C shim over wgpu-native so real projects (soorat, rasa,
ranga, bijli, aethersafta, kiran) can depend on a stable,
sovereign-owned surface immediately. The C shim is **transitional
scaffolding** — it exists exclusively to get mabda in front of
consumers while the native GPU backend is being built. When the
native backend lands, consumer code will not change by a single byte.

```
  v2.3.0  ───▶  wgpu-native via C shim          (shipping, audited)
       │
       │        kernel GPU driver work (in parallel, AGNOS scope)
       ▼
  v3.0    ───▶  pure Cyrius GPU backend         (backend swap — API unchanged)
```

Everything in this roadmap prioritizes **API stability** over
**backend correctness**. The public surface (context, buffer, compute,
texture, render_pipeline, etc.) is the load-bearing contract. The FFI
layer underneath is explicitly marked internal (`@internal` header on
line 1 of every FFI module) so no consumer accidentally couples to it.

---

## v2.0.0 — Cyrius Port (Shipped)

Complete port from Rust to Cyrius. All Rust modules ported. GPU FFI
operational via C launcher + struct-packing shims.

### Shipped
- All core modules: error, color, capabilities, context, profiler, resource
- All buffer/compute modules: buffer, compute, shader_cache, pipeline_cache, bind_group_cache
- All graphics modules: vertex, blend, sampler, depth, texture, bind_group, instancing
- All render modules: render_target, render_pipeline, render_pass, surface, debug
- FFI layer: wgpu_types, wgpu_descriptors, wgpu_ffi, C launcher
- 89 standalone tests + 4 GPU integration tests (including buffer round-trip)
- Buffer readback round-trip (write → copy → map → verify) — **v1.0 criterion met**
- Cyrius compiler fixes upstreamed: PIC codegen (3.4.12), mmap rename (3.4.12), `_cyrius_init` (3.4.14)
- GPU discovery via yukti 1.2.0
- Struct-packing shim pattern for wgpu functions with 6+ i64 args (avoids `fncall6` bug)

---

## v2.1.0 — Feature Catch-up (Shipped)

Rust-parity catch-up release. Every v2.1 item landed along with a
batch of wgpu v29 enum-value fixes that were latent in the v2.0 tree
(silently compiled against an older wgpu version — first noticed
when texture and render pipeline FFI actually dispatched into
wgpu-native).

- `typed_buffer.cyr` port
- Standalone `.tcyr` tests for pure-data modules (+191 assertions)
- GPU timestamp profiling (`gpu_timestamps.cyr`)
- Texture creation via FFI (slots 45–51)
- Render pipeline FFI (slots 52–53, `render_pipeline_create_simple`)
- Surface FFI (slots 54–57)
- CPU-only benchmark harness (`tests/mabda.bcyr`)

**Outcome:** 290 assertions, 10 GPU-backed phase-0 checks. Fixed a
latent cache-dangling-pointer bug that shipped in v2.0 (shared
`_hash_to_heap_key` helper in `cache_key.cyr`).

---

## v2.1.1 — Stdlib Inclusion (Shipped)

Goal: ship mabda as a Cyrius stdlib dep so every Cyrius user gets a
sovereign GPU surface they can build against.

- `dist/mabda.cyr` single-file bundle
- `[lib]` section in the manifest
- Toolchain pin bumped to Cyrius 3.4.19 (the release that activates
  `[deps.mabda]` as a first-class dep)
- Public API surface marked with `# @public` / `# @internal` on
  line 1 of every `src/*.cyr` file (see ADR 005)
- `examples/stdlib-consumer/` reference project
- `docs/stdlib-integration.md` consumer guide
- Vendored `lib/*.cyr` replaced by a symlink to `$HOME/.cyrius/lib/`
- `scripts/version-check.sh` — version consistency gate

---

## v2.1.2 — Rust-source Removal (Shipped)

Hygiene release. The frozen `rust-old/` tree was removed from the
working tree; the full Rust v1.0.0 source remains accessible via
`git checkout 1.0.0`. Preserved the Rust benchmark history CSV and
the Rust coverage snapshot inlined into
`docs/benchmarks-rust-v-cyrius.md`.

---

## v2.2.0 — Scaffolding Refresh (Shipped)

Project layout brought in line with the first-party AGNOS convention
(yukti / vidya / patra). Toolchain pin jumped from Cyrius 3.4.19 to
5.4.7. No library API changes — every consumer call site keeps
working without modification.

- `cyrius.toml` → `cyrius.cyml` (yukti format,
  `version = "${file:VERSION}"` templating)
- Flat layout: `src/lib.cyr` (renamed from `src/mabda.cyr`) declares
  the single include chain; 29 domain modules remain flat
- `tests/tcyr/mabda.tcyr` — consolidated CPU-only suite (273
  assertions)
- `tests/bcyr/mabda.bcyr` — benchmark harness in conventional subdir
- `programs/smoke.cyr` — link-check entry point for `cyrius build`
- `programs/phase0.cyr` — renamed GPU integration test
- CI rewritten to match yukti: lint / fmt / vet / dist-sync / test /
  bench gates
- Release workflow regenerates `dist/mabda.cyr` via `cyrius distlib`
- Makefile shrunk to a thin wrapper over the `cyrius` CLI
- `scripts/bundle.sh` retired (`cyrius distlib` replaces it)

---

## v2.3.0 — P(-1) Security Audit (Shipped)

Last audit-gated stdlib-candidate release before mabda is promoted
to first-party trusted status alongside yukti / patra / sakshi. Full
findings in
[`docs/audit/2026-04-19-audit.md`](../audit/2026-04-19-audit.md) —
0 CRITICAL / 2 HIGH / 6 MED / 6 LOW across 29 modules; every HIGH
and MED fixed.

- **HIGH-1** `surface_state_present` name collision → accessor was
  silently shadowed by the mutating present helper, breaking
  `_surface_state_configure`
- **HIGH-2** `rpb_label` 4-byte heap overflow (80-byte alloc, 8-byte
  store at +76)
- **MED-1** workgroup helpers: zero-divisor SIGFPE
- **MED-2** growable buffer: `cap * 2` overflow
- **MED-3** texture upload: dimension product overflow + missing
  zero-dim guard
- **MED-4** storage buffer write: `write_count × element_size`
  overflow
- **MED-5** storage buffer wrap: capacity / count / element_size
  inconsistency
- **MED-6** profiler: `clock_gettime` return-value unchecked
- 13 new audit-regression assertions (273 → 286)
- `CLAUDE.md` gains P(-1) + Security Hardening sections

### Scheduled (not blocking 2.3.0)
- **LOW-2** `read_buffer` size cap
- **LOW-3** wire `validate_dispatch` / `validate_dimensions` into
  internal dispatchers
- **LOW-4** bounded `strlen` in `wgpu_string_view`
- **LOW-5** resource cleanup on `compute_pipeline_new` failure paths
- **LOW-6** clamp color components in `texture_from_color`

---

## v2.4.0 — v1.0-Parity Closeout (Next)

Finish the last unchecked items from the v1.0 criteria list below —
the Rust v1.0 criteria that haven't been re-validated end-to-end in
Cyrius yet. Small release, no structural changes.

| # | Item | Status |
|---|------|--------|
| 1 | Compute dispatch end-to-end test in `programs/` | Planned — write → dispatch → read-back assertion, mirroring the buffer round-trip that already ships |
| 2 | Render pipeline end-to-end test in `programs/` | Planned — draw-and-readback off an offscreen render target |
| 3 | Pick up scheduled LOW audit items (LOW-2 → LOW-6) | Planned — none individually block, but 2.4.0 is a good time to sweep |

Exit criteria: `programs/phase0.cyr` + two new programs together
exercise every v1.0 criterion and close the checklist below.

---

## Future — Native GPU Backend (direction, unscoped)

The wgpu-native + C shim path is transitional. The long-term
direction is a pure Cyrius GPU backend (DRM/KMS today, AGNOS kernel
driver eventually) so Cyrius projects depend on zero C artifacts for
GPU work. **Scope is deliberately left open** — this will be tackled
when there's appetite for it, not front-loaded as a commitment.

**The one invariant that matters:** the public mabda API (`# @public`
files from v2.1.1) does not change when the backend swaps. Consumers
bump their `[deps.mabda]` tag and their C launcher requirement
disappears. The `examples/stdlib-consumer/` project is the regression
test — if it still compiles after the swap, the contract held.

Everything else — which vendor first, what the shader lowering looks
like, whether to take a compute-only waypoint — is a call for a later
version when the work actually starts. Not spec'd here.

---

## Backlog (demand-gated, orthogonal to the backend swap)

| Feature | Status | Notes |
|---------|--------|-------|
| Render graph (DAG pass orchestration) | Not started | Additive on top of `render_pass` + `render_pipeline` |
| Multi-queue coordination | Not started | Additive to GpuContext |
| Compressed textures (BC/ETC2/ASTC) | Not started | Builds on texture FFI |
| SPIR-V shader support | Not started | Currently WGSL only |
| Mipmap generation | Not started | Builds on texture FFI |
| Image loading (PNG/JPEG) | Not started | Consumer-side today; could adopt a pure-Cyrius decoder once one exists |
| WebGPU/WASM target | Not started | Blocked on Cyrius WASM backend |

---

## v1.0.0 Criteria (Rust v1.0 feature parity in Cyrius)

- [x] All 29 modules ported
- [x] GPU context creation working
- [x] Buffer create / write / release working
- [x] Buffer readback round-trip (write → copy → map → verify) —
      `programs/phase0.cyr`
- [x] Texture create / view / upload / release — `programs/phase0.cyr`
- [x] Render pipeline create / release — `programs/phase0.cyr`
- [ ] Compute dispatch end-to-end test (v2.4.0)
- [ ] Render pipeline end-to-end draw + readback (v2.4.0)
- [ ] Consumer integration: soorat port to Cyrius (consumer-side, not
      mabda-side — tracked in soorat's repo)

---

## Architectural Decisions

| ADR | Decision |
|-----|----------|
| [001](../adr/001-gpucontext-public-fields.md) | GpuContext uses accessor functions (`load64` at fixed offsets) |
| [002](../adr/002-runtime-alignment-validation.md) | Runtime alignment check for uniform buffers (bitwise AND) |
| [003](../adr/003-fixed-vertex-types.md) | Fixed vertex types, manual layout, no codegen |
| [004](../adr/004-c-launcher-ffi.md) | C launcher with function table for wgpu-native FFI — **transitional**, replaced in v3.0 |
| [005](../adr/005-public-api-surface-marking.md) | Public API surface marking (`# @public` / `# @internal`) — the stability boundary that survives the backend swap |
| 006 (planned, v3.0) | Pure Cyrius GPU backend via DRM/KMS — supersedes ADR 004 |
