# Migrating to the native AMD backend (v3.0)

> Written against mabda 3.0.0; still applicable through 4.0.1 (see the
> v4.0.1 update below for what changed). Pairs with
> [`integration.md`](integration.md) (which covers the wgpu path).
> Closes the gap from "I have a wgpu-on-mabda consumer" to "I have a
> wgpu-OR-native consumer that swaps backends without changing
> rendering code."
>
> **v4.0.1 update — AMD-on-wgpu is deprecated; this migration is recommended.**
> An AMD adapter on the wgpu path still works but gets a one-shot deprecation
> warning; `gpu_context_from_preinit` only hard-rejects
> (`GPU_ERR_AMD_WGPU_DEPRECATED`) under `-D MABDA_AMD_WGPU_STRICT`. The escape
> hatch stays open during the deprecation window (full retirement is deferred),
> but you should move to `BACKEND_KIND_AMD` native + precompiled GFX9 ISA
> shaders. NVIDIA + Intel keep their wgpu route. Note the native-AMD path does
> not yet cover every wgpu feature — notably instancing (`ic > 1`) and
> odd-dimension render targets are rejected — so confirm before flipping.

## Why three backends

mabda ships three GPU paths against the same public API (as of v4.0.1):

| Backend | Stack | Where it shines |
|---|---|---|
| `BACKEND_KIND_WGPU` (cross-vendor default) | wgpu-native v29 → Vulkan/Metal/DX12 | Cross-platform, every desktop GPU works, mature drivers. Per-vendor-deprecating — AMD-on-wgpu is deprecated at v4.0.1 (warn+allow; `-D MABDA_AMD_WGPU_STRICT` hard-rejects) |
| `BACKEND_KIND_AMD` (native, v3.0) | direct AMDGPU DRM ioctls (no libdrm), GFX9 | Own-the-stack, no wgpu-native runtime, AMD-only |
| `BACKEND_KIND_NVIDIA` (native, v4.0) | direct nouveau DRM ioctls (no libdrm), SM75/Turing | Own-the-stack, no wgpu-native runtime, NVIDIA-only |

The public surface — `gpu_buffer_*`, `gpu_compute_dispatch`,
`gpu_texture_*`, `gpu_render_*`, `gpu_surface_*` — is the same on all
three. The choice is a single compile-time `MABDA_BACKEND_KIND`
selector plus the matching `gpu_context_*` entry at init.

## Pick the backend

```cyrius
# wgpu — exactly as v2.x. No code changes. Takes the C-built preinit ptr.
var ctx = gpu_context_from_preinit(preinit_ptr);

# native AMD — requires /dev/dri/renderD128 readable
var ctx = gpu_context_new_native();

# native NVIDIA (v4.0) — nouveau, requires /dev/dri/renderD128 readable
var ctx = gpu_context_new_native_nvidia();
```

All three return a `GpuContext*` with the same shape
(`GPU_CONTEXT_SIZE` = 176 bytes; see `src/context.cyr`). The extra
slots past the wgpu handles are native surface-stash / RT-VA state
that backend-agnostic code never reads.

## Shaders are byte-polymorphic

`gpu_shader_module_create(ctx, bytes_ptr, n)` is byte-polymorphic at
the backend boundary. The source-kind tags live in `src/backend.cyr`
(`SHADER_SRC_WGSL` = 0, `SHADER_SRC_SPIRV` = 1, `SHADER_SRC_GFX9` = 2,
`SHADER_SRC_SASS` = 3):

| Backend | Accepted forms |
|---|---|
| wgpu | WGSL UTF-8 source (`SHADER_SRC_WGSL`) or SPIR-V binary (`SHADER_SRC_SPIRV`) |
| native AMD | pre-compiled GFX9 ISA (`SHADER_SRC_GFX9`) |
| native NVIDIA | pre-compiled SM75 SASS (`SHADER_SRC_SASS`) |

Consumer ships **multi-form bundles** — a WGSL/SPIR-V blob plus the
per-native binary for whichever native backend it targets. The build
picks which one flows based on the backend selection. Note that mabda
grew an in-tree **SPIR-V → GFX9 compiler** across v3.2.5–v3.2.12
(`_native_shader_compile_spirv` in `src/backend_native.cyr`), so on the
native AMD path you can feed SPIR-V and let mabda lower it rather than
hand-shipping GFX9 ISA. The NVIDIA SPIR-V → SASS front-end is the v4.x
endgame (N9); until it lands, the NVIDIA path takes pre-compiled SASS.

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
libsystemd via its C-shim. **v4.0.1 deprecates the AMD wgpu route**
(AMD-on-wgpu still works with a one-shot warning; `-D MABDA_AMD_WGPU_STRICT`
enforces native-only), and the paired **libsystemd/samvada C-shim
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

Native render targets created via `gpu_render_target_*` must still
have **even** width and height — the fixed native shader's viewport
math uses an integer `rt_width / 2` divide and odd dimensions lose the
half-pixel, so `native_rt_create_2d_rgba8` (`src/backend_native.cyr`)
rejects odd dims with `GPU_ERR_TEXTURE_DIMENSION`. It also caps the
dimension via the shared `validate_dimensions(w, h,
MABDA_MAX_TEXTURE_DIM_2D)` guard before the size math. This constraint
is unchanged; dropping it (f64 viewport math) is still deferred.

What **did** change is RT VA allocation. v3.4.2 replaced the old
single fixed RT VA with a **per-context RT VA sub-allocator**
(`native_ctx_alloc_rt_va`, cursor at ctx +168, 256 MiB bump region at
`_NATIVE_RT_VA_BASE`), so multiple live render targets no longer
collide on one VA; v3.4.3 raised the GEM_VA span alignment to 64 KiB
to avoid an `EINVAL` on 4-KiB-but-not-64-KiB spans.

## Compute / draw caveats (native)

- Single-shader-pair-per-pipeline. The native render path uses a
  fixed FS+VS pair set up at pipeline create time.
- `vc` (vertex count) on `render_pass_draw` is now **honoured** —
  `_backend_native_render_pass_draw` rejects `vc <= 0` and draws the
  caller-supplied count. Instancing is still single-shot: `ic != 1`
  returns `GPU_ERR_NOT_IMPLEMENTED` (multi-instance is future work).
- `gpu_buffer_create` / `_write` / `_read` are **real** on the native
  backend as of v3.2 (Phase X) — see
  `_backend_native_buffer_create` / `_write` / `_read` in
  `src/backend_native.cyr`. Create allocates a 64 KiB-aligned GTT BO
  and VA-maps it; write/read are bounds-checked coherent memcpys into
  the mmap. This unblocks the public transfer-copy API
  (`gpu_buffer_copy`) and portable buffer code across backends.
- `gpu_shader_module_create` accepts pre-compiled GFX9 ISA (AMD) or
  SM75 SASS (NVIDIA), and SPIR-V on the AMD path via the in-tree
  compiler; see "Shaders are byte-polymorphic" above.

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

- **Stay on wgpu** if cross-platform matters, or if Intel is in scope
  (Intel has no native backend). Native now covers both AMD (v3.0) and
  NVIDIA (v4.0); Intel-native is not planned.
- **Consider native** if your consumer is AGNOS-targeted, or if the
  wgpu-native runtime cost / dependency is a problem you want to drop.
  The native path has ~2x lower CPU overhead for the smoke shapes (no
  FFI marshalling, direct ioctl) but a smaller feature surface than
  wgpu.
- **Ship all three** by writing your code against the public API and
  selecting `MABDA_BACKEND_KIND` at consumer-build time. That's the
  architectural sell — same code, three backends, swap at startup.

## Known native limitations

From the [2026-04-30 audit](../audit/2026-04-30-audit.md), these items
remain open backlog (the native `gpu_buffer_*` slot impls (LOW-3) and
`vc` honouring (part of LOW-4) have since shipped — see the caveats
above):

- Two-pass DRM discovery TOCTOU clamping (MED-4)
- `set_master` `-EINVAL` ambiguity (MED-5)
- Bump-allocator leaks in long-running compositors (MED-7, LOW-5)
- Multi-instance draws (`ic > 1`) in native render (remainder of LOW-4)
- Defense-in-depth nits (LOW-1, LOW-2)

None of these block normal consumer integration today; all surface
on either contrived input (caller-supplied silly dimensions) or
long-running production-shape workloads (compositor / game engine).

## Questions / report back

If your consumer hits a wall the audit didn't predict, file under
`docs/development/issues/` with the `native-backend` tag. Phase B/C/D coverage
is bounded by the in-tree test matrix — your real workload is the
real test.
