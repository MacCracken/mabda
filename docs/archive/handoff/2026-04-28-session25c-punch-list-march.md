# Session 25c handoff — Punch-list march toward native graphics path

**Date:** 2026-04-28
**Branch:** `v3`
**Predecessor:** `2026-04-28-session25-b4-verified.md` (Phase B.4 verified, native compute dispatch working end-to-end on Cezanne).

## Where you'll be when you pick this up

The native compute dispatch is solid (B.4). The wgpu-side v3 backend abstraction is **fully wired to the v2 (render) interface** — every public `gpu_render_*` API on the wgpu path routes through `ctx->backend->render_*` slots and hits real wgpu, byte-exact verified across 4 GPU integration programs.

The native graphics path is **gated on shader bytes** (Step 6.2). That's the next bite. Everything else for v3.0 either feeds off it (6.5/6.6/6.8c/6.9b) or is independent and can be parallelized (Phase D surface, Tier 2-6).

## What landed in session 25c (8 commits on `v3`)

| Commit | Step | What |
|--------|------|------|
| `4183a8a` | 6.3 | `native_rt_create_2d_rgba8` + release primitive at VA range `0xFFFF800101000000` |
| `cce4300` | 6.4 | PM4 builders: `native_pm4_set_context_reg / _one / _pair / _draw_index_auto` |
| `4f2eec5` | docs | `docs/proposals/v3-native-render-design.md` (333 lines, GFX9 graphics-pipeline design) |
| `79221df` | 6.7 v0 | Backend interface v2 expansion proposal — 7 new render slots |
| `ae05bd4` | 6.7 v2.1 | Pit-stop research validation; corrected design (clear-color f64×4, GFX9 PM4 3-block split) |
| `b7ae2db` | 6.8a | Pure layout: 7 slot offset constants, `BACKEND_SIZE 120 → 176`, render-range constants |
| `0c3b554` | 6.8b | 7 wgpu render slot wrappers; `backend_wgpu_new` fills all 21 slots |
| `458cf42` | 6.9a | 7 public `gpu_render_*` dispatchers + 8 mock-backend dispatch tests |

Plus vidya field-notes updates (in `../vidya/`):
- `mabda-v3-gpu.cyml`: 6 → 14 entries (added `phase_b5_backend_abstraction`, `phase_c_textures_complete`, `phase_c_render_groundwork`, `native_dispatch_caching`, `gfx9_graphics_shader_abi_research`, `gfx9_pipeline_pm4_three_block_split`, `wgpucolor_is_4_doubles_not_a_packed_u32`, `wgsl_to_gfx9_lowering_research`; rewrote `phase_b4_store_blocker` as `phase_b4_store_blocker_resolved`).
- `language.cyml`: 22 → 24 entries (added `global_init_order_silent_zero`, `var_bracket_size_must_be_literal`).

## Test posture at session end

| Sweep | Count | Result |
|-------|-------|--------|
| `cyrius test tests/tcyr/mabda.tcyr` | 624 assertions | green |
| `cyrius test tests/tcyr/mabda_v3.tcyr` | 484 assertions | green |
| `make build/phase0` + run | 12 GPU asserts | green |
| `make build/compute_e2e` + run | 7 GPU asserts | green |
| `make build/render_e2e` + run | 8 GPU asserts | green |
| `make build/render_graph_e2e` + run | 5 GPU asserts | green |

**Total: 1108 CPU + 32 GPU assertions, all passing on real hardware (Renoir / gfx90c / Mesa 26.0.5).**

## Uncommitted right now (clean follow-up commit)

```
M programs/compute_e2e.cyr
M programs/phase0.cyr
M programs/render_e2e.cyr
```

These add `include "src/depth.cyr"` and (where needed) `render_target.cyr` / `render_pass.cyr` to the selective-include programs. The fix lands because Step 6.9a (commit `458cf42`) added `render_target.cyr` to compute_e2e and render_e2e — and `render_target.cyr::rtb_build` references `depth_texture_new / depth_texture_view` from `src/depth.cyr`. Cyrius DCE doesn't strip those, so the GCC link sees unresolved refs unless `depth.cyr` is included.

`cyrius build` reports OK for these programs without the fix because Cyrius produces an `.o` file — the unresolved-symbol failure only surfaces at the GCC link step in the Makefile. So the Step 6.9a CI signal was misleading. **Lesson:** when a step touches what the integration programs include, run `make build/<program>` (full link), not just `cyrius build`.

Suggested commit message:
```
fix(programs): add depth.cyr/render_target.cyr/render_pass.cyr to selective-include programs

Step 6.9a's render_target.cyr addition surfaced an existing dependency:
rtb_build references depth_texture_*. Cyrius DCE keeps them in the .o
file, so the GCC link fails without depth.cyr. Phase0 also gained
render_target.cyr + render_pass.cyr since backend_wgpu's render
wrappers reference them.
```

This is a 3-file, ~6-line diff. Commit and continue.

## The pit-stop research validation (the value-add of session 25c)

Step 6.7 was originally written from internal context only — wgpu API names + slot patterns I'd seen elsewhere in the codebase. After the user pushed back ("did you do external research?"), two parallel research agents verified the v2 design against the **actual** webgpu.h v29 header + Mesa radv source. They found two real design bugs:

1. **Clear color was packed as a single u32** (`0xRRGGBBAA`). WGPUColor is actually `{double r,g,b,a;}` — 32 bytes, not 4. A u32 pack loses 56 bits of precision per channel and breaks HDR/wide-gamut clears that already work on the wgpu path. **Corrected** to a pointer-to-32-byte-f64-RGBA-block, matching mabda's existing `Color` shape.

2. **"Encode pipeline state once at pipeline_create"** was rejected. GFX9 graphics state has *three* lifetimes, not one: `pipeline_sh_block` (per-pipeline, shader VAs + RSRC), `pipeline_ctx_block` (per-pipeline, blend/raster/depth-test mode), `pass_target_block` (per-pass, RT-extent-derived: CB_COLOR0_*, scissor, viewport), and a `draw_tail` (per-draw, USER_DATA + DRAW_INDEX_AUTO). This matches Mesa radv's `ctx_cs` / `cs` / dynamic-state cut. Speculative single-block caching would have broken RT-bind, viewport-change, and per-draw uniforms.

Filed in proposal v2.1 (`docs/proposals/v3-backend-interface.md`, "GFX9 PM4 block split" section + "Verification & citations" section) and in two new vidya field-notes (`gfx9_pipeline_pm4_three_block_split`, `wgpucolor_is_4_doubles_not_a_packed_u32`).

**The lesson** — when designing slot interfaces that wrap external APIs, read the relevant struct definition / source before deciding the arg shape. "Internal context only" produces drafts that look right but slip critical detail.

## What's blocking next: Step 6.2 (graphics shader bytes)

The native graphics path has a single hard gate: hand bytes for a minimal vertex+fragment shader pair (full-screen triangle, solid color output). Without that, 6.5 (PM4 stream builder), 6.6 (native render dispatch), 6.8c (native render slot wrappers), and 6.9b (`programs/native_render_e2e.cyr`) are all blocked.

### What we know

A research agent wrote up the spec-derived shape in detail. Summary:

- **VS**: `@builtin(vertex_index)` (in VGPR0 from rasterizer hardware) → compute three positions for full-screen triangle → `exp pos0` with TGT=12, EN=0xF, DONE=1. ~9 instructions, ~104 bytes including 16-dword NOP padding.
- **FS**: 4× `v_mov_b32_e32` for solid red, `exp mrt0` with TGT=0, EN=0xF, VM=1, DONE=1. ~6 instructions, ~96 bytes including padding.
- **SH-reg state** (from Mesa `sid.h` + research agent):
  - `R_SPI_SHADER_PGM_LO_VS = 0x2C48`, `_HI_VS = 0x2C4C`, `_RSRC1_VS = 0x2C50`, `_RSRC2_VS = 0x2C54`
  - `R_SPI_SHADER_PGM_LO_PS = 0x2C08`, `_HI_PS = 0x2C0C`, `_RSRC1_PS = 0x2C10`, `_RSRC2_PS = 0x2C14`
  - `RSRC1_*` minimum: `0x002C0040` (FLOAT_MODE=0xC0, DX10_CLAMP, IEEE_MODE — same MIN as compute)
  - `RSRC2_*` for these shaders: `0x00000000` for VS, `0x00000004`-ish for PS (USER_SGPR=0; PS may need a flag bit set)
  - `SPI_SHADER_POS_FORMAT = 4` (POS_FLOAT_4)
  - `SPI_SHADER_Z_FORMAT = 0` (no depth output)
  - `SPI_SHADER_COL_FORMAT`: ambiguous between FP16_ABGR (0x9), UNORM16_ABGR (0xA), FP32_ABGR (0xE) — needs verification for our RGBA8 RT
- **Pipeline-static context-reg state**: `CB_TARGET_MASK = 0xF`, `CB_SHADER_MASK = 0xF`, `CB_COLOR_CONTROL = 0xC8`, `CB_BLEND0_CONTROL = 0`, `PA_SU_SC_MODE_CNTL = 0x4` (cull none), `VGT_SHADER_STAGES_EN = 0`, `SPI_PS_INPUT_ENA = 0` (or 0x2 if hang-on-zero quirk), `SPI_VS_OUT_CONFIG = 0`. Full table in the research agent's report (preserved in conversation history).

### The Mesa 26 capture problem

I attempted to capture an `AMD_DEBUG=ib` dump on the dev box and Mesa 26.0.5 doesn't expose user-level IB dumping the way Phase B.4's protocol assumed. Tried:

- `RADV_DEBUG=hang` — only emits on actual hang
- `RADV_DEBUG=dumpibs` — silent (the env var exists in the binary's strings but doesn't produce output)
- `RADV_DEBUG=metashaders / shaderstats / trace / info` — only emits device info, not PM4 or shader bytes
- `MESA_VK_TRACE=rgp` — silent without a swapchain frame boundary
- `vkcube` under `xvfb-run` — segfaults because Xvfb lacks DRI3

The dev box is Arch base (no desktop, no Wayland, no DRI3). Vulkan apps that need a presentation surface won't run. Headless Vulkan workloads that DO run (compute_e2e, render_e2e) don't emit IB dumps under any radv debug flag I tried in Mesa 26.0.5.

**Important framing correction:** Phase B.4's working compute shader bytes (`native_gfx9_shader_store_deadbeef` in `src/backend_native.cyr` lines 893-973) document their source as `clang -target amdgcn--amdhsa -mcpu=gfx90c` — i.e., the bytes were derived via clang+AMDHSA, not from a live IB dump. The "byte-exact against radv IB" framing in `feedback_pm4_verify_against_mesa_ib.md` may have applied only to **PM4 packets** (which we DO emit and CAN diff against radv source), not to shader ISA bytes (which are emitted *into* a BO and never appear in PM4). I haven't verified this against the actual Phase B.4 work log.

### Three viable paths for 6.2(b) (decision needed)

1. **Match the compute approach** — hand-encode VS+FS against the GCN5 ISA spec, iterate on hardware once 6.5/6.6/6.8c land. Same workflow that produced the working compute shader. Risk: graphics needs different USER_DATA + SPI conventions than AMDHSA compute; iteration may take a few hardware cycles. **My current recommendation** — it's the pattern that already worked for compute, the user has the iteration loop muscle memory from B.4.

2. **clang + AMDPAL + USER_DATA translation** — `clang -target amdgcn--amdpal -mcpu=gfx90c` against a tiny GLSL→SPIR-V→ISA, then patch the USER_DATA prologue to match radv's convention. AMDPAL is closer to the graphics ABI than AMDHSA. Risk: tooling drift; the bytes still need verification.

3. **Pre-extracted bytes from Mesa source** — radv's `radv_meta_clear.c::build_color_shaders()` is NIR; we'd need to manually trace what NIR→ACO produces. Higher cognitive cost than (1).

**The decision is yours.** I tried not to pre-commit speculative bytes (the trap that burned 12 sessions on B.4 originally). Whichever path you pick, 6.2(a) (the spec-derived register state + offset constants) can land independently and feeds 6.5 / 6.8c.

## What I'd land first when you pick this back up

1. **Commit the include fix** (`programs/{phase0,compute_e2e,render_e2e}.cyr`) as the bug-fix follow-up to 6.9a.
2. **6.2(a) — the safely-derivable parts.** ~30 minutes:
   - `src/backend_native.cyr`: add 12 GFX9 register offset constants + 6 spec-derived field minimums (RSRC1/RSRC2/format/etc.).
   - `tests/tcyr/mabda_v3.tcyr`: CPU value-asserts for each constant (per the lesson from Step 4f.ii — value-assert every computed constant).
   - `docs/proposals/v3-shader-bytes-capture.md`: the protocol doc + the three derivation paths + Mesa-26-specific notes on why the env-var capture didn't work.
3. **Decide 6.2(b) path** (the three options above) and start the work for whichever you pick.
4. **6.5 / 6.6 / 6.8c follow naturally** once 6.2(b) bytes land.

In parallel — if you want a side-quest that doesn't need shader bytes — Phase D (7.x: surface / present via DRM/KMS) is fully unblocked and complements the work nicely.

## Quick-reference paths

- Punch list: `docs/development/3-0-punchlist.md` — refreshed post-6.9a, decision log entries through 2026-04-28 covering the pit-stop and 6.8a/b/9a.
- Backend interface proposal: `docs/proposals/v3-backend-interface.md` — at v2.1, includes the corrected GFX9 PM4 block split + verification citations.
- Render design doc: `docs/proposals/v3-native-render-design.md` — the GFX9 graphics-pipeline state enumeration from Step 6.1.
- Vidya field-notes: `../vidya/content/cyrius/field_notes/mabda-v3-gpu.cyml` (14 entries, latest is 2026-04-28).
- Open issues filed this arc:
  - `docs/development/issues/2026-04-28-cyrius-global-init-order.md` — slotted for fix in cyrius 5.7.32
  - `docs/development/issues/2026-04-28-cyrlint-multi-line-assert.md` — file-size threshold bug, workaround in place
  - `docs/development/issues/2026-04-28-cyim-regex-pattern-error.md` — workaround documented

## What you'll thank past-you for in 12 hours

The pit-stop research validation. The temptation when picking back up will be to assume "the v2 design was sound, let me just code it" — but the v2.0 design had two real bugs that v2.1 caught. **The 3-block PM4 split is load-bearing for 6.8c.** If you find yourself writing a single PM4 buffer per-pipeline in the native render code, stop and re-read the "GFX9 PM4 block split" section of `docs/proposals/v3-backend-interface.md`.

Also: the 1108 + 32 passing tests are real. The mock-backend dispatch tests in `mabda_v3.tcyr` are particularly valuable — they verify slot threading without GPU, so any v2 dispatcher bug surfaces at CPU-test time. Pattern is: `var b = backend_new(); backend_set_slot(b, BACKEND_SLOT_*, &capture_fn); var ctx = alloc(96); store64(ctx + 32, b); call_public_fn(...); assert capture got the right args.` Use this for every new dispatcher.

Sleep well. The native graphics path is one decision and one bite away from unblocking.
