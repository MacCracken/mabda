# Migrating to the native AMD backend (v3.0)

> Written against mabda 3.0.0 / Cyrius 5.11.28. Pairs with
> [`integration.md`](integration.md) (which covers the wgpu path).
> Closes the gap from "I have a wgpu-on-mabda consumer" to "I have a
> wgpu-OR-native consumer that swaps backends without changing
> rendering code."
>
> **v4.0.1 update — this migration is now mandatory on AMD.** The AMD wgpu
> route is retired: an AMD adapter is rejected on the wgpu path
> (`gpu_context_from_preinit` returns `GPU_ERR_AMD_WGPU_RETIRED`). On AMD you
> MUST select `BACKEND_KIND_AMD` native and ship precompiled GFX9 ISA shaders.
> NVIDIA + Intel keep their wgpu route (per-chipset retirement). Note the
> native-AMD path does not yet cover every wgpu feature — notably instancing
> (`ic > 1`) and odd-dimension render targets are rejected — so confirm your
> consumer doesn't rely on those before flipping.

## Why two backends

mabda v3.0 ships two GPU paths against the same public API:

| Backend | Stack | Where it shines |
|---|---|---|
| `BACKEND_KIND_WGPU` (v2 default) | wgpu-native v29 → Vulkan/Metal/DX12 | Cross-platform, every desktop GPU works, mature drivers |
| `BACKEND_KIND_AMD` (v3 new) | direct AMDGPU DRM ioctls (no libdrm) | Own-the-stack, no wgpu-native runtime, AMD-only |

The public surface — `gpu_buffer_*`, `gpu_compute_dispatch`,
`gpu_texture_*`, `gpu_render_*`, `gpu_surface_*` — is the same on
both. The choice is a single `gpu_context_new_*` selector at init.

## Pick the backend

```cyrius
# wgpu — exactly as v2.x. No code changes.
var ctx = gpu_context_new(fn_table_ptr, preinit_ptr);

# native — AMD only, requires /dev/dri/renderD128 readable
var ctx = gpu_context_new_native();
```

Both return a `GpuContext*` with the same shape (112 bytes in v3.0;
the new +96 / +104 slots are surface-stash that backend-agnostic
code never reads).

## Shaders are byte-polymorphic

`gpu_shader_module_create(ctx, bytes_ptr, n)` is byte-polymorphic at
the backend boundary:

| Backend | Interpretation |
|---|---|
| wgpu | bytes are WGSL UTF-8 source |
| native | bytes are pre-compiled GFX9 ISA |

Consumer ships **two-form bundles** in v3.0 — one WGSL string and one
GFX9 binary per shader. The build picks which one flows based on the
backend selection. In-mabda WGSL → GFX9 lowering is deferred to v3.x
(see [`docs/proposals/v3-wgsl-frontend-choice.md`](../proposals/v3-wgsl-frontend-choice.md)).

For the GFX9 form, two paths:

1. **In-tree library** — mabda ships `native_gfx9_shader_*` helpers
   in `src/backend_native.cyr` that emit the dwords for the MVP
   shader set (`solid_red`, `fullscreen_triangle_vs`,
   `store_deadbeef`). Use these for the smoke / e2e shapes.

2. **Hand-encode or compile from a single source** at the consumer's
   build time:

   ```sh
   clang -target amdgcn--amdhsa -mcpu=gfx90c -O2 -nogpulib \
         -o shader.o shader.cl
   llvm-objdump -d shader.o   # verification
   ```

   then ship `shader.o`'s `.text` section as the bytes.

## Surface API (v3.0 new)

The v3 surface API is **not** in v2.x — it's new-in-v3.0. Three
backend-specific configure entries + three backend-agnostic
per-frame ops:

```cyrius
# Configure — one of three, depending on what you're shipping
var s1 = gpu_surface_configure_wgpu(ctx, wgpu_surface, w, h);
var s2 = gpu_surface_configure_native_kiosk(ctx, card_fd, w, h);
var s3 = gpu_surface_configure_native_logind(ctx, w, h);

# Per-frame — backend-agnostic
var fb = gpu_surface_acquire(ctx, surface);   # back-buffer ptr
# ... render into fb ...
gpu_surface_present(ctx, surface);             # block on vsync
gpu_surface_release(ctx, surface);             # at shutdown
```

**Choose the configure entry based on session shape:**

| Entry | When | Pre-conditions |
|---|---|---|
| `_wgpu` | wgpu backend | Caller already created a `WGPUSurface` from the windowing library |
| `_native_kiosk` | native, dev/no-compositor | Caller has opened `/dev/dri/cardN` and called `SET_MASTER` themselves |
| `_native_logind` | native, in-session (compositor running) | `samvada` C-shim linked + initialized; logind delegates DRM master via `TakeDevice()` |

`_native_logind` is the production path for desktop apps. `_native_kiosk`
is the bring-up / kiosk path. `_wgpu` continues to work unchanged.

## samvada wiring (logind path only)

If you ship `_native_logind`, your consumer needs to link
`samvada/deps/samvada_main.c` alongside mabda's `deps/wgpu_main.c`.
The C side calls `samvada_main(table)` once during init to populate
samvada's static fn-table reference; mabda then routes through
samvada to ask logind for the master fd.

```sh
# in your consumer's build
gcc -c samvada/deps/samvada_main.c -o samvada_main.o
# ... link both samvada_main.o and wgpu_main.o into your binary
```

samvada itself is a sister AGNOS package (`[deps.samvada]
tag = "0.4.1"` in your `cyrius.cyml`). It currently uses
libsystemd via its C-shim. **v4.0.1 retires the AMD wgpu route**
(AMD adapters are rejected on the wgpu path — this migration is
now mandatory on AMD), but the paired **libsystemd/samvada C-shim
retirement is deferred** under the roadmap escape hatch: the
pure-Cyrius dbus replacement is upstream samvada work that isn't
ready, so `[deps.samvada]` stays `0.4.1` and `MABDA_LOGIND` stays
opt-in. When samvada ships pure-Cyrius 1.0, mabda swaps via a
one-line tag bump. (v4.0 itself shipped NVIDIA native.)

If you don't need the logind path (kiosk / development / no
compositor in your session), skip samvada entirely — `_native_kiosk`
takes a raw card_fd you opened yourself.

## Surface dimension constraints (v3.0)

- **wgpu path** — honours `width`/`height` exactly as wgpu does
  (driver may reject if not aspect-compatible with the swap chain).
- **native path** — `width`/`height` must match the EDID-preferred
  mode of the first connected connector. If they don't match,
  configure returns 0 (use `native_kms_summary` to discover the
  panel's preferred resolution first). v3.x will add explicit mode
  selection by `(w, h)` so consumers can pick non-preferred modes
  cleanly.

This matters because the v3.0 audit (2026-04-30, HIGH-1) closed a
silent-substitution bug: previously the native path discarded the
consumer's `width`/`height` and used the EDID preferred mode without
saying so. Cross-backend identity (write portable code, swap
backends, see the same pixels) requires loud failure on dimension
mismatch.

## Render target dimension constraints (native)

Native render targets created via `gpu_render_target_*` must have
**even** width and height. The v2-native fixed shader's viewport
math uses `rt_width / 2` (integer divide) and odd dimensions lose
the half-pixel. The allocator returns `GPU_ERR_TEXTURE_DIMENSION`
on odd dims.

This is a v3.0 limitation — v3.x will switch to f64-based viewport
math and drop the constraint.

## Compute / draw caveats (native v3.0)

- Single-shader-pair-per-pipeline. v2-native's render path uses a
  fixed FS+VS pair set up at pipeline create time.
- `vc` / `ic` (vertex / instance counts) on `render_pass_draw` are
  accepted for ABI parity but the dispatch is fixed at 3-vertex /
  1-instance (i.e. fullscreen-triangle). v3.x extends to honour
  caller-supplied counts.
- `gpu_buffer_create` / `_write` / `_read` slots return
  `GPU_ERR_OTHER` (stub) on the native backend in v3.0. Use the
  in-tree `native_buf_pair_*` private API for compute store/load
  smoke shapes; consumer-facing buffer abstractions are v3.x scope.
- `gpu_shader_module_create` accepts pre-compiled GFX9 ISA bytes;
  see "Shaders are byte-polymorphic" above.

## Test matrix the consumer should run

```sh
# on a box with wgpu-native available
make test-phase0
make test-render-e2e

# on a box with amdgpu hardware (render-node access enough)
make test-native-compute-store
make test-native-render-e2e

# on a box with amdgpu hardware + DRM master (kiosk / vt-switch / logind)
make test-native-kms-modeset
make test-native-present-e2e
```

The first two prove the wgpu path didn't regress. The compute /
render natives prove BO + CS submission. The KMS programs prove the
present pipeline end-to-end. Consumers should add an
`integration_native_<your-thing>.cyr` smoke alongside the
existing `integration_<your-thing>.cyr` they already have on
the wgpu path.

## When to flip

- **Stay on wgpu** if cross-platform matters, or if NVIDIA/Intel are
  in scope. Native is AMD-only in v3.0.
- **Consider native** if your consumer is AGNOS-targeted (AMD only),
  or if the wgpu-native runtime cost / dependency is a problem you
  want to drop. The native path has ~2x lower CPU overhead for the
  smoke shapes (no FFI marshalling, direct ioctl) but a much smaller
  feature surface in v3.0.
- **Ship both** by writing your code against the public API and
  selecting the backend at consumer-init time. That's the v3.0
  architectural sell — same code, two backends, swap at startup.

## Known v3.0 limitations (deferred to v3.x)

From the [2026-04-30 audit](../audit/2026-04-30-audit.md), the
following items file as v3.x backlog:

- Caller-supplied dimension overflow guards (MED-2)
- Two-pass DRM discovery TOCTOU clamping (MED-4)
- `set_master` `-EINVAL` ambiguity (MED-5)
- Bump-allocator leaks in long-running compositors (MED-7, LOW-5)
- `vc` / `ic` honoured in native render (LOW-4)
- Native `gpu_buffer_*` / `gpu_shader_module_*` slot impls (LOW-3)
- Defense-in-depth nits (LOW-1, LOW-2)

None of these block normal consumer integration today; all surface
on either contrived input (caller-supplied silly dimensions) or
long-running production-shape workloads (compositor / game engine).

## Questions / report back

If your consumer hits a wall the audit didn't predict, file under
`docs/issues/` with the `native-backend` tag. Phase B/C/D coverage
is bounded by the in-tree test matrix — your real workload is the
real test.
