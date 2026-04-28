# ADR 006: Native Cyrius GPU backend (DRM/KMS), added alongside ADR 004

**Status:** Proposed (v3.0 design phase, branch `v3`)
**Date:** 2026-04-21
**Updated:** 2026-04-28 — vendor scope clarified after Phase B.4 verified; wgpu retirement reframed per-chipset (v4.0 AMD, v5.0 NVIDIA, v5.1 Intel) instead of a single v4.0 event. See [development/roadmap.md](../development/roadmap.md).
**Supersedes:** n/a
**Related:** ADR 004 (C launcher FFI — stays in-tree until per-chipset retirement; full removal at v5.1), ADR 005 (public API surface marking)

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

Ship a second mabda backend in v3.0, vendor-by-vendor across the
v3.x–v5.x line:

- **Name:** `native` (vs. the existing `wgpu` backend), with vendor
  variants — `BACKEND_KIND_AMD` (v3.0), `BACKEND_KIND_NVIDIA`
  (v4.0), `BACKEND_KIND_INTEL` (v5.0, tentative).
- **v3.0 scope:** pure Cyrius DRM/KMS on Linux for **AMD hardware
  only** — direct `ioctl(DRM_IOCTL_AMDGPU_*)`, GFX9 PM4 packet
  streams, pre-compiled GFX9 ISA. No libdrm, no libwayland, no
  wgpu-native, no C launcher. Phase B.4 verified end-to-end on AMD
  Cezanne (gfx90c). NVIDIA and Intel hardware run on the existing
  wgpu path through v3.x. AGNOS kernel GPU driver integration is a
  downstream v3.x+ follow-up once the userspace DRM/KMS path is
  stable.
- **Selection:** per-consumer build-time. Compile-time constant
  `MABDA_BACKEND_KIND` in `src/lib.cyr` (per
  [`docs/proposals/v3-backend-interface.md`](../proposals/v3-backend-interface.md));
  promotable to a `cyrius.cyml` flag in v3.1+ if consumer CI
  matrices need it.
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

5. **Consumer migration path** (per-chipset retirement)
   - v3.0: `wgpu` is the default for everyone. AMD consumers can opt
     into `BACKEND_KIND_AMD` (native). NVIDIA + Intel + macOS +
     Windows consumers stay on `wgpu`.
   - v3.x: AMD consumers test `native` in their CI matrix when ready.
   - v4.0: NVIDIA native (`BACKEND_KIND_NVIDIA`) lands; **AMD wgpu
     retires**. AMD consumers run on AMD native only. NVIDIA + Intel
     still on wgpu. The wgpu binding stays in-tree to serve them.
   - v5.0: Intel native (`BACKEND_KIND_INTEL`) lands (tentative);
     **NVIDIA wgpu retires**. Intel may still be on wgpu if v5.0
     ships only the NVIDIA-side retirement.
   - v5.1: **Intel wgpu retires** and the entire wgpu+C scaffolding
     leaves the tree (`deps/wgpu_main.c`, `deps/wgpu-native/`,
     `src/wgpu_*.cyr`, `src/backend_wgpu.cyr` all removed). Mabda is
     fully native-Cyrius across every supported vendor.
   - Each per-chipset retirement is gated on every consumer on that
     vendor having flipped voluntarily; calendar dates are not the
     gate. v5.1 is gated on Intel native (v5.0) having shipped.

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
  implemented on native yet" error). At v4.0+ the surface count
  grows (AMD native + NVIDIA native + wgpu remainder); at v5.x it
  peaks (three native backends + wgpu) before contracting at v5.1
  back down to three.
- CI matrix roughly doubles the GPU test time per supported vendor.
  Mitigated because CPU-only tests (387 assertions, 7 CPU benches)
  run once regardless.
- `src/` grows by the abstraction layer + per-vendor backend
  modules; `dist/mabda.cyr` grows accordingly until v5.1's wgpu
  removal.

**Neutral:**

- The per-chipset retirement timeline is set by consumer cutover,
  not calendar. If consumers move fast on a vendor, that vendor's
  retirement window is near; if not, that vendor stays on wgpu
  longer. Either outcome is fine under this ADR.
- v5.1 is the explicit "wgpu fully out" milestone. If Intel native
  (v5.0) doesn't actually ship, v5.1 doesn't ship either — the
  wgpu binding stays in-tree to serve Intel until the gate is met.

## Open questions

Resolved during v3.0 design spike (some by Phase B.4 verification on
2026-04-28, others by the v3-backend-interface proposal):

- **AMD WGSL → GFX9 ISA lowering.** v3.0 takes pre-compiled GFX9 ISA
  bytes; lowering path landed as a v3.0 in-scope item (not yet
  implemented in code, but scoped — see roadmap). NVIDIA / Intel
  lowering paths are per-vendor v4.0 / v5.0 design items.
- **Selector ergonomics.** Resolved: compile-time constant
  `MABDA_BACKEND_KIND` in `src/lib.cyr`, per
  [`docs/proposals/v3-backend-interface.md`](../proposals/v3-backend-interface.md).
  Promotable to a `cyrius.cyml` flag in v3.1+ if needed.

Still open at the time of this update:

- **Memory model.** How does the native backend integrate with
  mabda's manual `alloc`/`store64` discipline? Does DRM/KMS BO
  management surface to the `Resource` tracker? (B.4 sidesteps this
  by allocating per-program; real consumer flows need an answer.)
- **Surface / present path.** DRM/KMS provides raw scanout.
  Consumers (soorat, aethersafta) expect a surface abstraction
  compatible with what wgpu gives them today. Mapping TBD; gated on
  Phase D.
- **Error model.** wgpu errors come back through the FFI callback
  path. Native backend errors come through syscall return. The
  `GpuErr` codes already exist (`src/error.cyr`); mapping TBD when
  the backend interface lands in code.
- **Per-vendor ADRs.** Each vendor expansion (NVIDIA v4.0, Intel
  v5.0) gets its own ADR. ADR 007 = NVIDIA. ADR 008 = Intel. Both
  filed when their respective design phases open, not now.

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
