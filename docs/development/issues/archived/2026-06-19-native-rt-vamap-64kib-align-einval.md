# Issue: native render target `va_map` fails with EINVAL unless the BO size is 64 KiB-aligned

> **RESOLVED in 3.4.2 (2026-06-19), HW-verified on Cezanne.** Both the 64 KiB
> alignment and the fixed-VA-base limit are fixed:
> - `native_rt_create_2d_rgba8` rounds the BO + VA-map span up to 64 KiB
>   (`_native_align_up(…, _NATIVE_RT_VA_ALIGN)`), threading the same rounded size
>   through create / map / release / the stored `NATIVE_RT_FIELD_SIZE`.
> - The RT VA is no longer fixed: a per-context bump cursor at `ctx+168`
>   (`native_ctx_alloc_rt_va`, `GPU_CONTEXT_SIZE` 168→176) hands out distinct
>   64 KiB-aligned VAs from a 256 MiB region, so multiple live RTs no longer
>   collide. `native_ctx_free_rt_va` rolls back on a create error.
>
> Verified by `programs/native_rt_alloc_e2e.cyr` (a 1260×682 RT + a 2nd live RT,
> distinct VAs, full-span GTT sentinel) and the `native.tcyr` CPU regressions
> `test_native_rt_bo_size_64k_rounding` / `test_native_rt_va_bump_distinct`. The
> `duplicate fn 'color_rgb'` note below was investigated and is a *consumer-side*
> collision, not a mabda bug — see the corrected Secondary section.

**Discovered:** 2026-06-19 (integrating mabda 3.2.11 into the `puka` Wayland terminal — first consumer to allocate a *window-sized* render target)
**Component:** `src/backend_native_amdgpu.cyr` `native_gem_va_map` / `src/backend_native.cyr` `native_rt_create_2d_rgba8`
**Severity:** High for real consumers (any render target whose byte size isn't a multiple of 64 KiB fails; that's most real window sizes). Latent because every in-tree test uses 256×256.
**Workaround in place:** Yes (consumer-side) — `puka` pads the allocated target up to a 256-pixel multiple per axis (always 64 KiB-aligned) and reads back the visible sub-rect. The clean fix belongs here.

## Summary

`gpu_render_target_create_2d_rgba8(ctx, W, H)` returns `0` (→ `GPU_ERR_OTHER`,
99) for ordinary window sizes such as **1260×682**, while "nice" sizes like
256×256, 512×512, 1024×1024, and even 2048×2048 succeed.

The failure is **not** the GTT BO allocation — `native_bo_create_gtt(fd, W*H*4)`
succeeds at every size tested (including the failing ones). The failure is the
**`DRM_IOCTL_AMDGPU_GEM_VA` map** in `native_rt_create_2d_rgba8`, which returns
raw errno **-22 (EINVAL)**. The public wrapper squashes that to `GPU_ERR_OTHER`,
which is why the symptom looks generic.

The discriminating factor is **64 KiB alignment of the BO byte size**:

| dims | bytes | bytes / 64 KiB | GTT BO create | RT va_map |
|---|---|---|---|---|
| 256×256 | 256 KiB | 4.0 ✓ | OK | **OK** |
| 512×512 | 1024 KiB | 16.0 ✓ | OK | **OK** |
| 1024×1024 | 4096 KiB | 64.0 ✓ | OK | **OK** |
| 2048×2048 | 16384 KiB | 256.0 ✓ | OK | **OK** |
| 1280×768 | 3840 KiB | 60.0 ✓ | OK | **OK** |
| **1260×682** | 3360 KiB | **52.45 ✗** (rem 29408 B) | OK | **EINVAL (-22)** |

Note 1280×768 is **not** a power of two yet succeeds — so the constraint is
specifically 64 KiB *byte-size* alignment, not power-of-two dimensions. The BO
size `W*H*4` is always 4 KiB page-aligned (the kernel requires that and it holds
here), but the GPU VA map additionally needs a 64 KiB-aligned size on this part.

Hardware: AMD Cezanne / Renoir APU (gfx90c), Linux 6.x.

## Reproduction

Each probe creates the RT and **releases it before the next** (the RT VA base is
fixed at `_NATIVE_RT_VA_BASE`, so leaving one mapped collides the next — a
separate gotcha worth a doc note for consumers):

```cyrius
fn probe(ctx, w, h): i64 {
    var fd = native_ctx_fd(ctx);
    var rt[64]; memset(&rt, 0, 64);
    var rc = native_rt_create_2d_rgba8(fd, w, h, &rt);     # full path (BO + va_map)
    # rc == 0 OK; rc == 99 fail
    if (rc != 0) {
        var sz = w * h * 4;
        var bo[16];
        native_bo_create_gtt(fd, sz, &bo);                 # this SUCCEEDS even on failing sizes
        var raw = native_gem_va_map(fd, load64(&bo), _NATIVE_RT_VA_BASE, sz, _NATIVE_PERM_DATA);
        # raw == -22 (EINVAL) for non-64KiB-aligned sz
    }
    # ... release rt / bo ...
}
```

Observed:

```
256x256  -> rt=0 OK
512x512  -> rt=0 OK
1024x1024-> rt=0 OK
1260x682 -> rt=99   va_map_errno=-22
2048x2048-> rt=0 OK
```

## Root cause (hypothesis)

`native_gem_va_map` passes `size = W*H*4` straight to `DRM_IOCTL_AMDGPU_GEM_VA`:

```cyrius
store64(&req + 32, size);     # backend_native_amdgpu.cyr, native_gem_va_map
```

amdgpu's GEM_VA map appears to require the mapped span to be aligned to the GPU
page-table fragment size (64 KiB on GFX9) for this map flag/path; a 4 KiB-aligned
but not 64 KiB-aligned size is rejected with EINVAL. The in-tree
`native_render_e2e` only ever maps 256×256 (256 KiB, trivially 64 KiB-aligned),
so this never surfaced.

## Proposed fix

Round the render-target BO size up to 64 KiB before the VA map (and allocate the
BO at that rounded size so the mapping stays within the BO). One option, in
`native_rt_create_2d_rgba8`:

```cyrius
var size = _native_align_up(native_rt_size_2d_rgba8(width, height), 0x10000);  # 64 KiB
```

`_native_align_up` already exists and is used for the 4 KiB page rounding
elsewhere. The extra padding is at most ~64 KiB per target — negligible. This
keeps the public dimension contract unchanged (callers still pass logical W×H;
the stride/extent registers continue to use `width`).

Worth auditing the same 64 KiB constraint on the other `native_*_create` paths
that `va_map` (textures, staging) — they may pass today only because their sizes
happen to be 64 KiB-aligned.

## Also worth a consumer-facing note — FIXED in 3.4.2

`_NATIVE_RT_VA_BASE` *was* a **fixed** VA — a consumer that created a second render
target without releasing the first got EINVAL on the second `va_map` (VA in use),
also surfacing as `GPU_ERR_OTHER`. 3.4.2 took the recommended path: it bumps the
base per allocation like the texture VA region does (`native_ctx_alloc_rt_va`, a
per-context cursor at `ctx+168` over a 256 MiB region). Multiple live RTs now get
distinct VAs.

## Secondary: `duplicate fn 'color_rgb'` build warning — consumer-side, not a mabda bug (corrected)

Compiling a consumer that `include`s `dist/mabda.cyr` *and also defines its own*
`color_rgb` emits:

```
warning: duplicate fn 'color_rgb' (last definition wins)
```

**Corrected diagnosis (3.4.2 investigation).** The original "amalgam may be
emitting `color_rgb` twice" hypothesis is **disproven**. mabda defines
`color_rgb` exactly once — `src/color.cyr` and the single amalgamated copy in
`dist/mabda.cyr` (`grep -c '^fn color_rgb' dist/mabda.cyr` == 1) — and there is
**no `color_rgb` anywhere in the Cyrius stdlib** (`math`, `sakshi`, etc.) or any
sibling AGNOS package. The duplicate is entirely *consumer-side*: `puka` defines
its own `fn color_rgb` (`puka/src/terminal.cyr` — a terminal-truecolor packer)
alongside `dist/mabda.cyr`, so the Cyrius v5.7.9+ parser sees two definitions and
warns "last definition wins". A consumer that does not define `color_rgb` sees no
warning.

This is **not** the 3.4.1 `F64_HALF`/`F64_TWO`-vs-`math` case (that was a real
collision against a *shared stdlib* symbol). mabda's `color_rgb` is documented
`@public` API; renaming it would be a breaking change in the wrong direction. The
established fix is consumer-side: the squatting package renames its helper (the
patra `json_build → patra_json_build` precedent). **No mabda code change.**
