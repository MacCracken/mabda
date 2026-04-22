# ADR 006: Native Cyrius GPU backend (DRM/KMS), added alongside ADR 004

**Status:** Proposed (v3.0 design phase, branch `v3`)
**Date:** 2026-04-21
**Supersedes:** n/a
**Related:** ADR 004 (C launcher FFI — v3.x-era backend, retires v4.0), ADR 005 (public API surface marking)

## Context

Mabda ships today on a C launcher + wgpu-native foundation (ADR 004).
That path gets consumers onto a stable GPU surface immediately and
inherits wgpu-native's portability work. It is load-bearing.

For three reasons, mabda also needs a pure-Cyrius GPU backend:

1. **Sovereignty.** AGNOS's stated direction is to own the stack down
   to the kernel. Indefinitely depending on wgpu-native — a Rust
   artifact linked as C — undermines that. A native Cyrius backend is
   a prerequisite for the AGNOS kernel GPU driver arc.
2. **Measurement story.** Without a second backend, we cannot cleanly
   attribute future benchmark wins. Cyrius 5.6.x is landing a
   compiler optimization arc (O1–O6). If the only GPU path in mabda
   is the C-FFI one, the benchmark comparison conflates *backend*
   and *codegen* changes. Two backends × two cyrius generations
   gives us a 4-way matrix where each axis can be isolated.
3. **Long-horizon API stability.** Consumers wrote against the v2.1.1
   `# @public` surface on the promise that it would survive backend
   changes. Validating that promise requires running the same bench
   suite and the same `examples/stdlib-consumer/` project under an
   independent backend.

**What this ADR is not:** a replacement for ADR 004 during v3.x. ADR 004
stays accepted and load-bearing throughout the v3.x line. **v4.0** is
when the C path retires; that retirement is consumer-driven
(every consumer has been running native in production across a full
release cycle), not calendar-driven.

## Decision

Ship a second mabda backend in v3.0:

- **Name:** `native` (vs. the existing `wgpu` backend).
- **Scope:** pure Cyrius DRM/KMS on Linux — no libdrm, no libwayland,
  no wgpu-native, no C launcher. AGNOS kernel GPU driver integration
  is a downstream v3.x+ follow-up once the DRM/KMS path is stable.
- **Selection:** per-consumer build-time. Probably a `cyrius.cyml`
  flag (e.g., `[mabda] backend = "wgpu"` / `"native"`) or a
  compile-time constant. Finalized before v3.0 ships.
- **Default for v3.0:** `wgpu`. Consumers opt into `native`
  explicitly. This keeps v3.0 byte-compatible for every existing
  consumer and makes the native backend the thing under test rather
  than the thing under load.
- **Public API is unchanged.** ADR 005's `@public` surface from
  v2.1.1 stays the same across both backends. ADR 005 is the
  machine-readable contract.

### Architectural pieces

1. **Backend abstraction layer (`src/backend.cyr`, `@internal`)**
   - Defines the set of operations every backend must implement:
     context creation, buffer create/write/map/release, compute
     pipeline create/dispatch, texture create/upload/release, render
     pipeline create/release, render pass begin/end, surface
     acquire/present, query timestamp.
   - Each operation is a function-pointer slot on a `Backend`
     struct. `GpuContext` holds a pointer to its backend.
   - Existing wgpu-path fn-pointer dispatch (wgpu_ffi.cyr's 65-slot
     table) sits under the wgpu backend's implementation; it does
     not surface to `@public` code.

2. **`src/backend_wgpu.cyr` (`@internal`)**
   - Implementation of the `Backend` interface that calls into the
     existing wgpu FFI tables. Existing code moves here behind the
     new indirection; `@public` callers never see the move.

3. **`src/backend_native.cyr` (`@internal`)**
   - Implementation of the `Backend` interface that talks directly
     to DRM/KMS. New module; scope carved up in follow-up proposals
     (Vulkan-on-kernel-driver vs. raw submission, WGSL lowering path,
     command ring buffer, allocator integration).

4. **Dual-backend bench harness**
   - `make bench-gpu` runs the 13-bench suite under each backend.
   - `bench-history.csv` grows a `backend` column.
   - `docs/benchmarks-rust-v-cyrius.md` refreshes with columns for
     Rust v1, wgpu-backend pre-5.6.x, wgpu-backend post-5.6.x,
     native-backend pre-5.6.x, native-backend post-5.6.x.

5. **Consumer migration path**
   - v3.0: `wgpu` default. `native` available behind a flag.
   - v3.1+: consumers test `native` in their CI matrix when ready.
   - v3.x late: once every consumer is in production on `native`
     across a full release cycle, v4.0 planning opens.
   - v4.0: wgpu path retires (`deps/wgpu_main.c`, `deps/wgpu-native/`,
     `src/wgpu_*.cyr`, `src/backend_wgpu.cyr` all removed).

## Why not these alternatives

- **Backend swap in v3.0 (original roadmap).** Rejected. Loses the
  measurement baseline, forces every consumer to move on our
  schedule rather than theirs, and makes a v3.0 regression in the
  native backend immediately user-visible without a fallback.
- **Native backend on a long-lived feature branch, never in-tree.**
  Rejected. Feature branches rot; the only way the native backend
  stays in parity with the public API is if it ships in-tree behind
  a flag and runs in CI.
- **libdrm wrapper.** Rejected. libdrm is another C dependency; the
  whole point is zero C artifacts for GPU work in the native path.
  DRM/KMS ioctls are callable directly from cyrius via `syscall`.

## Consequences

**Positive:**

- AGNOS's sovereignty trajectory advances without destabilizing
  existing consumers.
- The 4-way bench matrix isolates cyrius codegen wins from backend
  architecture wins — directly enables the "worth writing about"
  story post-5.6.x.
- The `@public` stability contract gets tested against a real second
  implementation instead of being an untested claim.
- Consumers migrate on their own timelines.

**Negative:**

- Two backends means two maintenance surfaces. Every new wgpu FFI
  entry needs a native-backend counterpart (or an explicit "not
  implemented on native yet" error).
- CI matrix roughly doubles the GPU test time. Mitigated because
  CPU-only tests (387 assertions, 7 CPU benches) run once.
- `src/` grows by the abstraction layer + two backend modules;
  `dist/mabda.cyr` grows accordingly.

**Neutral:**

- The v4.0 timeline is set by consumer cutover, not by calendar. If
  all consumers move fast, v4.0 is near; if not, v3.x gets a long
  tail. Either outcome is fine under this ADR.

## Open questions (deferred to v3.0 design spike on `v3` branch)

- **WGSL → ISA lowering.** Vendor-first or multi-vendor? If
  vendor-first, which vendor? Depends on AGNOS hardware targeting.
- **Memory model.** How does the native backend integrate with
  mabda's manual `alloc`/`store64` discipline? Does DRM/KMS BO
  management surface to the `Resource` tracker?
- **Surface / present path.** DRM/KMS provides raw scanout.
  Consumers (soorat, aethersafta) expect a surface abstraction
  compatible with what wgpu gives them today. Mapping TBD.
- **Error model.** wgpu errors come back through the FFI callback
  path. Native backend errors come through syscall return. The
  `GpuErr` codes already exist (`src/error.cyr`); mapping TBD.
- **Selector ergonomics.** `cyrius.cyml` flag? Compile-time constant
  in `src/lib.cyr`? Build-time environment variable? All three are
  implementable; pick one before v3.0 ships.

## Follow-up

- Flesh out the `Backend` interface in a proposal under
  `docs/proposals/` before writing `src/backend.cyr`.
- First DRM/KMS spike: compute-only path in `programs/native_compute_spike.cyr`.
  Success criterion: a trivial compute shader dispatches and reads
  back via the native backend with no wgpu-native linked.
- Update `docs/stdlib-integration.md` to document the backend flag
  once syntax is finalized.
- Add `backend` column to `bench-history.csv` on first bench run
  under either backend (schema change is forward-compatible).
- Log any cyrius-compiler friction encountered during native-backend
  work in `docs/proposals/cyrius-5.6x-optimization-requests.md` for
  upstream filing.
