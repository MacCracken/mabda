# v3 Backend Interface — `@internal` dispatch surface

**Status:** Draft (v3 branch, 2026-04-28 — written immediately after Phase B.4 verified)
**Related:**
[ADR 006](../adr/006-native-cyrius-gpu-backend.md) (dual-backend),
[ADR 005](../adr/005-public-api-surface-marking.md) (`@public` boundary),
[v3-native-api-principles](v3-native-api-principles.md) (consumer-facing API),
[B.4 verified handoff](../handoff/2026-04-28-session25-b4-verified.md)

## Framing

ADR 006's follow-up bullet 1 reads: *"Flesh out the `Backend`
interface in a proposal under `docs/proposals/` before writing
`src/backend.cyr`."* This is that proposal.

It is **not** the consumer API design — that's
`v3-native-api-principles.md`. This proposal sets the
`@internal` indirection point both backends will land on. The
public API never sees this surface.

## Why now

Phase B.4 just landed. Both halves of the dual-backend story now
exist in code:

- `src/wgpu_ffi.cyr` (65-slot fn-pointer table) + `src/buffer.cyr`,
  `src/compute.cyr`, `src/texture.cyr`, etc. — the `wgpu` path,
  shipping today.
- `src/backend_native.cyr` (40+ `native_*` primitives) — the
  pure-Cyrius DRM/KMS path, proven end-to-end via
  `programs/native_compute_store.cyr`.

What does **not** exist: a single function-pointer table that
either backend can fill, that the `@public` API calls into without
caring which is wired up. Without it, every `@public` operation
that wants to stay backend-agnostic has to branch internally —
and we lose the ADR 005 promise (the public surface is the
contract; everything below is implementation).

## Anchor — what the working dispatch actually exercised

The Backend interface's first concrete purpose is to host the
operations `native_compute_store` already performed. Naming the
operations after the proven flow, not a speculative superset, is
the only honest starting point.

The flow, in order:

1. **Open device** — `native_open_render_node()` on native;
   `wgpu_create_instance` + adapter request on wgpu.
2. **Create context** — kernel ctx_id on native; device + queue on wgpu.
3. **Allocate BO + map to VA** — five calls in the program:
   shader / stub / output / IB / fence. Each is GTT-allocated,
   memory-mapped for CPU writes, and VA-mapped for GPU reads.
4. **Build IB** — PM4 register-setup + dispatch + post-dispatch
   marker. Native-only artifact; `wgpu` builds equivalent state
   internally via `wgpuComputePassEncoderDispatchWorkgroups`.
5. **Submit + wait** — 4-chunk submit on native; queue submit + fence
   on wgpu.
6. **Readback** — CPU read of `out_va`. Identical on both backends
   from the consumer's perspective.

Every operation above maps to a `Backend` slot. PM4 IB construction
does not — it's a native-implementation detail that hides behind
`backend_compute_dispatch`.

## The Backend struct

`Backend` is a fixed-offset struct of `fnptr` slots. One slot per
operation. `GpuContext` carries a pointer to its `Backend`; every
backend-aware function reads `ctx->backend->slot` and `fncall`s
through it.

### Layout (v0 — minimum to host the proven compute flow)

```text
Backend struct (88 bytes — 11 slots × 8 bytes):
  +0:   ctx_create_from_preinit  (preinit_ptr)        → ctx
  +8:   ctx_release              (ctx)                → 0
  +16:  buffer_create            (ctx, size, usage)   → buf_handle
  +24:  buffer_write             (ctx, buf, off, ptr, n) → 0|err
  +32:  buffer_read              (ctx, buf, off, ptr, n) → 0|err
  +40:  buffer_release           (ctx, buf)           → 0
  +48:  shader_module_create     (ctx, bytes_ptr, n)  → mod
  +56:  shader_module_release    (ctx, mod)           → 0
  +64:  compute_dispatch         (ctx, mod, x, y, z, bindings_ptr) → 0|err
  +72:  device_wait_idle         (ctx)                → 0
  +80:  backend_kind             (ctx)                → BACKEND_WGPU | BACKEND_NATIVE
```

11 slots. Smaller than the wgpu_ffi 65-slot table because that
table is "every wgpu function the codebase calls"; this one is
"every operation the consumer-facing API calls *through the
abstraction*."

`bindings_ptr` is a raw byte buffer of `(slot, kind, handle)`
triples — bindless descriptor shape per `v3-native-api-principles`
non-negotiable #2. Bind-group / layout objects are not in the
interface. The native side packs into USER_DATA registers; the
wgpu side translates to wgpu bind groups internally.

### What's deferred (named, not in v0)

These exist in mabda today on the wgpu path. They're not in v0
because the native path doesn't have them yet, and shipping the
interface with stub-`return GPU_ERR_NOT_IMPLEMENTED` slots that
don't have a native test path is the wrong move:

| Operation | Gating |
|-----------|--------|
| `texture_create / write / release` | needs native texture format/tiling work (Phase C) |
| `render_pipeline_create / release` | needs native graphics ring path (Phase C) |
| `render_pass_begin / end` | needs surface acquire (Phase D) |
| `surface_configure / acquire / present` | needs DRM/KMS scanout path (Phase D/E) |

Each gets added to `Backend` when its native counterpart has a
working e2e program. Fail-loud (interface compile-error) is better
than fail-silent (`GPU_ERR_NOT_IMPLEMENTED` at runtime).

## File layout

```text
src/backend.cyr                — @internal: Backend struct, BACKEND_KIND_*,
                                  null-slot helpers, generic dispatch wrappers
src/backend_wgpu.cyr           — @internal: fills Backend with wgpu impls
src/backend_native.cyr         — @internal: fills Backend with native impls
                                  (already exists; gains a builder fn)
src/context.cyr                — @public: GpuContext gains a backend ptr at +32
                                  (preserves existing +0..+24 layout)
src/buffer.cyr, compute.cyr    — @public: bodies become 1-line dispatchers
                                  through ctx->backend
```

`GpuContext` grows from 32 → 40 bytes:

```text
GpuContext struct (40 bytes):
  +0..+24: instance / adapter / device / queue   (existing — wgpu only,
                                                  native leaves zero)
  +24..+32: queue                                (existing)
  +32..+40: backend_ptr                          (new — Backend struct ptr)
```

The `+24` queue slot stays for ABI compatibility with consumers
that read it; on native, `backend_ptr->queue_id` carries the
amdgpu ctx_id.

## Selector

Per ADR 006, the selector ergonomics are open. This proposal
recommends a **compile-time constant** in `src/lib.cyr`:

```cyrius
# src/lib.cyr
var MABDA_BACKEND_KIND = BACKEND_KIND_WGPU;   # change to BACKEND_KIND_NATIVE
```

…and a `gpu_context_from_preinit` body that reads it and wires
the right `Backend` into `ctx->backend`. The reasons to prefer
constant over `cyrius.cyml` flag at this stage:

- One `cyrius distlib` regenerate gives you a bundle pinned to
  one backend — the v3.0 bench harness goal needs both bundles
  side by side, no per-build conditional logic.
- Cyrius doesn't yet have first-class build-time conditionals
  beyond constants; emulating one with a manifest flag pushes
  parsing into `src/lib.cyr` for no real win.
- `cyrius.cyml` flag remains an option for v3.1+ once consumer
  CI matrices need it; promoting from constant to flag is a
  one-commit change.

`MABDA_BACKEND_KIND` is `@internal`. Consumers never see it —
they see the same `gpu_context_from_preinit` they always have.

## Migration sequencing

Five steps, each verifiable end-to-end before the next:

1. **Land `src/backend.cyr` with the Backend struct + the 11
   v0 slots.** No callers yet. Tests assert the struct layout
   (`sizeof Backend == 88`, slot offsets at multiples of 8).
2. **Land `src/backend_wgpu.cyr`.** Fill all 11 slots with
   wrappers around the existing wgpu functions. `programs/phase0.cyr`
   continues to pass when `MABDA_BACKEND_KIND = BACKEND_KIND_WGPU`.
3. **Refactor `src/buffer.cyr`, `src/compute.cyr`, etc.** to
   dispatch through `ctx->backend`. Public API surface unchanged;
   ADR 005 `@public` audit still passes.
4. **Land `src/backend_native.cyr` builder.** Wire the existing
   `native_*` primitives into the 11 slots. Lift the working
   `native_compute_store` dispatch into `native_backend_compute_dispatch`.
   `programs/phase0.cyr` is then expected to pass when
   `MABDA_BACKEND_KIND = BACKEND_KIND_NATIVE` — that's the v3.0
   exit criterion for the abstraction layer.
5. **Dual-backend bench harness.** `make bench-gpu` recompiles
   the bundle once per backend, runs the 13-bench suite under
   each, emits CSV columns for both.

Each step is a CLAUDE.md "small bite" — assertions added, lint
clean, distlib regenerated.

## Risks and what we'll watch for

- **Slot creep before native parity.** Tempting to add
  `texture_*` slots to the v0 interface and stub them on native.
  Don't. Adds maintenance surface that doesn't earn its keep.
- **`fnptr` calling-convention bugs.** Cyrius `fnptr` works at
  5.4.x, but all fnptr-heavy code in mabda today goes through
  `wgpu_ffi.cyr`'s `fncall*` wrappers against extern C. A
  Cyrius-fn-to-Cyrius-fn `fncall` through a struct slot is a
  shape we haven't extensively exercised; a smoke test should be
  step 0 of this work.
- **GpuContext layout migration.** Existing `gpu_ctx_*` getters
  read fixed offsets. The `+32` extension is additive but every
  `alloc(32)` in the codebase has to grow to `alloc(40)`. Single
  greppable transform.
- **Native-side BO ownership.** The working `native_compute_store`
  allocates 5 BOs per dispatch and frees them at end of program.
  Real consumer flows allocate buffers once and dispatch many
  times. The `Backend.buffer_*` slot signature anticipates this
  (`buf_handle` is opaque; native impl carries the `(handle, va)`
  pair). The `compute_dispatch` slot doesn't allocate the IB BO
  for the consumer — it pulls it from a per-context IB ring buffer.
  IB-ring lifecycle is gated on the same Phase B follow-up that
  lets `native_compute_store` dispatch twice without leaking.

## Out of scope for this proposal

- Render path. Phase C (the texture / render-pipeline / render-pass
  slots) gets its own interface revision once the native side has
  a working `programs/native_render_e2e.cyr`.
- Multi-queue. v3.1 work; the `Backend` struct layout above
  accommodates extension via a `+88` queue-handle slot when the
  time comes.
- WGSL → ISA lowering. The interface takes `bytes_ptr + n` for
  shader modules; the bytes are SPIR-V on wgpu and pre-compiled
  GFX9 ISA on native. The lowering decision (ADR 006 open
  question) doesn't touch this proposal — it changes what bytes
  consumers feed, not the slot signature.

## Decision required from the user

Before step 1 lands:

- **Sign-off on the v0 11-slot list.** Is this the right anchor
  set, or do you want to defer one and pull another forward?
- **Sign-off on compile-time constant selector** (vs `cyrius.cyml`
  flag) for v3.0. The constant is reversible at zero cost; the
  question is whether it's the right shape for the bench harness.
- **Is there an active feature flag / kill switch you want?** ADR 006
  says `wgpu` stays the default; this proposal honors that. If
  you want a runtime override (e.g., environment variable) for
  CI debugging, name it now so step 1 can include it.
