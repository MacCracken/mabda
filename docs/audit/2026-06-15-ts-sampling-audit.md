# Security/Correctness Audit — v3.2.2 (native texture sampling, Phase TS.1–5), 2026-06-15

**Scope:** the v3.2.2 Phase-TS diff vs the `3.2.1` tag — the GFX9 sampler/image
descriptor + sampleable-texture + render-sample path on both backends:
`backend_native_shaders.cyr` (T#/S#/IMG-format builders + textured `image_load`
FS), `backend_native.cyr` (single-BO `create_2d_sampleable`, `bind_for_sample`,
the `render_pass_draw` textured override + 6-entry residency, release),
`context.cyr` (PM4 scratch 1024→2048), `backend.cyr` (2 sample slots, size
264→280, walk), `texture.cyr`/`render_pass.cyr`/`sampler.cyr`/`bind_group.cyr`
(public `gpu_texture_create_2d_sampleable` / `gpu_sampler_create` /
`gpu_render_pass_bind_texture` + the wgpu bind-group/sampler builders),
`wgpu_ffi.cyr`/`deps/wgpu_main.c` (`get_bind_group_layout` FFI).

**Method:** the descriptor/shader bytes were sourced from the authoritative
Mesa `gfx9.json` (S#/T#/IMG-format bit layouts + enums) and assembled/round-
tripped through `llvm-mc -mcpu=gfx90c` (the textured FS), each byte CPU-pinned.
Two P(-1)-style adversarial workflows then reviewed the integration: the wgpu
sample path (`review-wgpu-sample-ts5`, 4 dims) and the native sample path
(`review-native-sample-ts5`, 2 dims), each finding independently re-verified
against source.

**Result:** **0 CRITICAL**, **2 HIGH→fixed**, **several MED/LOW→fixed**, all
before the cut. Both backends' uncompressed-RGBA8 sampling is HW-verified on
Cezanne (`native_texture_sample_e2e` pixel-exact `RT[x,y]==tex[x,y]`;
`wgpu_texture_sample_e2e` RT == sampled color).

## Confirmed findings + resolutions

| # | Sev | Path | Issue | Resolution |
|---|-----|------|-------|------------|
| 1 | HIGH | wgpu draw | Per-draw `WGPUSampler`+`WGPUBindGroup`+`BGL` handle leak (unbounded in a per-frame consumer; the single-shot e2e hid it). | Release all three after `set_bind_group` (wgpu-native retains its own refs once recorded). |
| 2 | LOW | wgpu draw | No null-guards on the sampler/bgl/bg create chain (a non-textured pipeline mis-binds via wgpu abort). | Guard each create → `GPU_ERR_OTHER`, releasing partials. |
| 3 | MED | native release | Sampleable single-BO mapped at `bo_size` but released with `SIZE`=logical → a CPU-mmap page + GPU-VA reservation leaked per release. | `texture_release` recomputes `bo_size` from `t_va` for sampleable; storage/mipped path unchanged. |
| 4 | MED | native release | Release left `t_va != 0` → a released tex still looked sampleable (bind → stale-descriptor use-after-free/TDR). | Release clears `t_va`. |
| 5 | LOW | native draw | `bound_tex` stash never cleared → a same-pass second draw without re-bind would re-emit a stale descriptor (latent until multi-draw-per-pass). | `render_pass_draw` clears `bound_tex` after each draw (per-draw bind). |
| 6 | LOW | native create | `create_2d_sampleable` accepted compressed formats but built a LINEAR descriptor → would sample garbage. | Reject compressed-sampleable on native until TS.7 (tiled path); `gpu_caps_supports_format(BC)` stays 0 to match. |
| 7 | NIT | docs | Stale docstrings (two-BO → single-BO; 1024 → 2048 scratch). | Corrected. |

Each fix landed with a regression assertion or is covered by the HW e2e.

## Dismissed on verification

- wgpu e2e "leaks its own handles" — NOT_A_BUG (single-shot process exit).
- native USER_DATA_PS persistence / no in-pass unbind — NOT_A_BUG / NIT: each
  draw re-emits its full pipeline state (the proven solid-red path self-heals
  `SPI_PS_INPUT`/`RSRC1`), and single-draw-per-pass is the documented contract.

## Limitations carried into the arc (not deferred past 3.2.x)

- **Native COMPRESSED sampling** (BC/ETC2/ASTC) is **TS.7 (3.2.3)** — it needs
  the tiled `SW_MODE` + swizzle; native sampleable is RGBA8/linear at 3.2.2.
- **Bilinear / ETC2 / ASTC** sampling is **TS.8 (3.2.3)**.
- The descriptor bit POSITIONS are gfx9.json-authoritative and now HW-confirmed
  for the RGBA8 `image_load` path; the compressed/tiled fields are TS.7's HW
  cross-check.
