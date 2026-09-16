# Native render owned-state audit (mabda 4.1.3)

Reference record for the 4.1.3 render-state fix: which GPU registers mabda's
native AMD render streams must write themselves, why, and the hardware evidence.
The register tables at the end are generated (and checked) by
[`../state_poison/owned_state_audit.py`](../state_poison/owned_state_audit.py);
the harness that produced the hardware evidence is
[`../state_poison/`](../state_poison/README.md).

## 1. The problem

The GPU does not hand a new submission a known render state. Context registers
hold whatever was last loaded — the kernel clear state on a fresh boot or after
idle power-gating, or the previous client's values (after a MODE2 reset this
Cezanne carries them across processes). radv and radeonsi never inherit: on
GFX9 they emit `CONTEXT_CONTROL` + `CLEAR_STATE` and then their full state in
every submission. mabda emits no `CLEAR_STATE`, so **every register a draw
depends on must be written in every render stream**.

The 4.1.3 root cause was `PA_CL_VTE_CNTL`: never written, so a fresh-boot draw
ran with the viewport transform off and the fullscreen triangle covered only
pixel (0,0) — every native texture-sample gate failed. Round 1 of the fix added
VTE and the audited owned set; round 2 (this revision) completed the audit
across the whole GFX9 register database.

## 2. Sources

| input | version / provenance | sha256 |
|---|---|---|
| GPU | AMD Cezanne, PCI 0x1638, 1 shader engine x 1 SH array, 8 compute units (`cu_bitmap[0][0]` = 0xFF), 2 render backends (DRM_AMDGPU_INFO probe) | — |
| kernel | 7.2.3-arch1-3 | — |
| `clearstate_gfx9.h` | stable `v7.2.3` (kernel.org), identical at `v7.2` and in archlinux/linux `v7.2.3-arch1`; round 1 used a torvalds master copy — byte-identical | `e885c7f6475efd74e7729688a70992ff9543979363eb029c628bd95627fb34b5` |
| `gfx9.json` | Mesa `mesa-26.2.2` (identical to the main-branch copy used in round 1) | `081fbfd1251fb5c6f4b7b6a218ba217e21346e824a04ec8146ab0d1d389ddb06` |
| Mesa driver source | `mesa-26.2.2` release tarball (radv, radeonsi, `src/amd/common`), the installed Mesa | — |
| radv capture | `make snoop` in this directory: gfx_init IB (416 B, `c0-ib0`) + main IB (1856 B, `c1`); re-captured in round 2 register-for-register identical | `5740ee70f421a0b89b7969a6db74d4b46ad3c4a028ad3e8cbef07edc22b01d5a` / `4e486742a435992b1d463b65daeca0b7cf3ab6188fc376aa331f355799807211` |
| mabda streams | `state_poison/dump_streams`: clear-triangle 1364 B, untextured pass_draw 1388 B, textured 1448 B | — |

`gfx940.json` (also in Mesa) only adds compute / debug registers (e.g.
`SQ_THREAD_TRACE_*`, `COMPUTE_PGM_RSRC3`); no draw state.

## 3. Method

1. Decode radv's full submission (gfx_init + main IB) and mabda's three streams
   against gfx9.json; take the last value written before the draw.
2. Parse the kernel clear state.
3. Classify **every** gfx9.json register (1652): written by mabda (table A, with
   clear / radv / mabda values and a verdict), or a family verdict saying which
   owned register makes the family unreachable for mabda's draws (tables B-E).
   The generator fails on any unclassified register.
4. Hardware: register-only poison IBs (`state_poison`) before each gate; a
   liveness canary proves the poison reaches the next process; negative controls
   against libraries that inherit the state.

A register is owned when it (a) sits on the path of mabda's draws (VS + PS,
auto-index TRIANGLELIST, 1 instance, MRT0 RGBA8, 1x, no depth / stencil / blend
/ varyings) and (b) can hold a non-default value from another client or is
omitted by the clear state. Mesa's own no-`CLEAR_STATE` paths (GFX11 context
registers in `ac_cmdbuf.c` / `si_state.c`; the GFX9 `!has_clear_state` block)
are the reference list for (b).

## 4. Round 2 changes

| change | registers | evidence |
|---|---|---|
| value corrected | `VGT_OUT_DEALLOC_CNTL` 16 -> 32 | Kernel clear pair is REUSE_DEPTH 30 / DEALLOC_DIST 32, which radv and radeonsi run with here (both emit CLEAR_STATE; radv re-writes 30 and never writes the distance). Mesa writes 14 / 16 only for GFX7- or `!has_clear_state`. mabda's 30 / 16 matched neither pairing. Pixel-neutral for auto-index draws (all gates pass before and after). |
| added (context) | `VGT_OUTPUT_PATH_CNTL`, `VGT_HOS_CNTL`, `VGT_ENHANCE`, `IA_ENHANCE`, `WD_ENHANCE`, `VGT_VTX_CNT_EN`, `VGT_DRAW_PAYLOAD_CNTL`, `PA_CL_NANINF_CNTL`, `PA_SU_SMALL_PRIM_FILTER_CNTL`, `PA_SU_OVER_RASTERIZATION_CNTL`, `PA_STEREO_CNTL`, `PA_SC_SCREEN_EXTENT_CONTROL`, `PA_SC_TILE_STEERING_OVERRIDE` — all 0 (clear state) | Each selects or tweaks how mabda's primitives are assembled, tessellated, routed or rasterized: VGT output path, legacy tessellation, VGT / IA / work-distributor tweak bits, vertex counting, payload / RT-index routing, NaN/Inf or zero-area discard, small-primitive culling, stereo viewport / RT-slice routing, screen slicing, tile steering. Mesa writes most of them when it skips CLEAR_STATE (GFX11 / GFX12) or on GFX9 `!has_clear_state` (`VGT_VTX_CNT_EN`, `PA_CL_NANINF_CNTL`), radv writes `PA_SU_SMALL_PRIM_FILTER_CNTL`; `VGT_OUTPUT_PATH_CNTL`, `VGT_HOS_CNTL`, `IA_ENHANCE` and `WD_ENHANCE` have no Mesa writer — CLEAR_STATE is their only source, which mabda does not emit. `PA_STEREO_CNTL` is not in the GFX9 clear state at all. |
| added (UConfig) | `VGT_INSTANCE_BASE_ID` = 0 | radv writes 0 in every GFX9 gfx_init; UConfig is never cleared. |
| derived per device | `SPI_SHADER_PGM_RSRC3_VS`, `SPI_SHADER_LATE_ALLOC_VS` | See section 5. |
| no negative control | all of the above | For mabda's draws a wrong value is either pixel-neutral (no NaN/Inf vertices, no zero-area primitives, no instance-id or vertex-count inputs, a single SE) or has no known-safe non-default value (output path, legacy tessellation, stereo, steering could wedge the VGT / rasterizer). They are written at clear values, verified by `test_native_render_stream_owns_inherited_state`, and the poison matrices run with them at clear values. |
| round 1 verdicts kept | VTE, SX_MRT*_BLEND_OPT, DB_ALPHA_TO_MASK, VGT index clamp / offset (HW negative controls); CB_COLOR1..7_INFO, stream-out, RSRC3_PS, DFSM, EQAA, EDGERULE, conservative raster, IA_MULTI_VGT_PARAM, RESET_EN (semantic) | round 1 audit; re-checked in table A |

Not added, with reasons in tables B-D: every other context register (depth /
stencil, MRT1-7 surfaces, viewports 1-15, clip planes, varyings, points / lines,
GS / tessellation, MSAA, stream-out buffers, per-draw CP-loaded registers), and
UConfig / SH / CONFIG registers outside the draw path.

## 5. SPI_SHADER_PGM_RSRC3 CU_EN and late alloc

- `SPI_SHADER_PGM_RSRC3_PS` = 0x003FFFFF is **not** device-derived in Mesa 26.2.2:
  `ac_cmdbuf.c` writes `ac_apply_cu_en(CU_EN(~0) | WAVE_LIMIT(0x3F))`, and
  `ac_apply_cu_en` masks CU_EN only with `info->spi_cu_en`, which is `~0` unless
  the `AMD_CU_MASK` debug variable is set (`ac_gpu_info.c`
  `set_custom_cu_en_mask`). The kernel CU bitmap never enters it, so the
  constant is Mesa's value on every GFX9 part.
- `SPI_SHADER_PGM_RSRC3_VS` (CU_EN 0xFFFE) and `SPI_SHADER_LATE_ALLOC_VS` (24)
  **are** derived: `ac_compute_late_alloc` (GFX9, non-NGG, VS without scratch)
  uses `min_good_cu_per_sa = popcount(kernel cu_bitmap) / (SEs x SAs)` — <= 2:
  late 0, all CUs; <= 4: late 2, all CUs; else `(n - 2) * 4` with CU0 masked,
  clamped to `LIMIT[0:5]`.
- Round 2 implements that: `native_amdgpu_query_dev_info` (read-only
  `DRM_IOCTL_AMDGPU_INFO` / `AMDGPU_INFO_DEV_INFO`) at pipeline creation and
  `native_gfx9_vs_late_alloc_derive`; a failed query or impossible topology
  fails pipeline creation. On this Cezanne the probe reports 1 SE x 1 SA with
  8 CUs -> 0x003FFFFE / 24, byte-identical to the radv capture, and the stream
  the GPU received (`coverage_scan all <dir>`) matched the CPU dump.
- Compute's `COMPUTE_STATIC_THREAD_MGMT_SE0` = 0xFFFFFFFF matches Mesa's
  `SH0_CU_EN(spi_cu_en) | SH1_CU_EN(spi_cu_en)` with the default mask.

## 6. Pre-4.1.3 hardware claims re-tested

These comments were written while `PA_CL_VTE_CNTL` was itself inherited. Each
was re-run as a one-constant variant of the final round-2 library, with the
`clear` poison immediately before every gate (render e2e 256x256 full coverage,
texture-sample, 2D-array, `coverage_scan all` = 64x64 red, 64x64 textured,
1920x1080 red). Kernel log clean throughout. (A variant is a scratch copy with
one constant changed and the four gate programs rebuilt; run each gate with
`state_poison/run_gate.sh clear "run:<gate>"`.)

| variant | old claim | result | verdict |
|---|---|---|---|
| control (library as shipped) | — | all pass | — |
| `PA_SU_SC_MODE_CNTL` = 0x240 (radv) | "produced black pixels" | all pass, exact | retracted; culling off makes FACE / poly-mode types neutral |
| `PA_CL_CLIP_CNTL` = 0x01080000 (radv, clipping on) | "produced black pixels" | all pass, exact | retracted |
| `SPI_PS_INPUT_ENA/ADDR` = 0x80 (radv; mislabelled "LINEAR_CENTER" — it is `LINE_STIPPLE_TEX_ENA`) | "produced black pixels" | all pass, exact | retracted, label corrected |
| `SPI_PS_INPUT_ENA/ADDR` = 0x20 (real `LINEAR_CENTER_ENA`) | same claim | all pass, exact | retracted |
| `CB_COLOR0_INFO` = 0x04000028 (mabda's older value) | "the CB wrote zeros" | all pass, exact | retracted (0x00028028 = FORMAT + BLEND_CLAMP + SIMPLE_FLOAT, kept) |
| `PA_SC_AA_MASK_*` = 0 | "0 = no pixels written" (also claimed 0 was the default) | every gate reads back only the sentinel (render exit 8, texture 11, array 9, scan 20) | semantic confirmed; default is 0xFFFFFFFF (clear state), comment corrected |
| `PA_SC_CLIPRECT_RULE` = 0 | "0 = reject everything" (claimed default) | no pixel in any gate | semantic confirmed; default is 0xFFFF (clear state), comment corrected |

Field decodes corrected against gfx9.json in the same pass: `VGT_SHADER_STAGES_EN`
0x00010000 is `MAX_PRIMGRP_IN_WAVE` = 2 (not "VS_EN"); `PA_SU_VTX_CNTL`
ROUND_MODE 2 is `X_ROUND_TO_EVEN` (not truncate); `SPI_VS_OUT_CONFIG`
VS_HALF_PACK is bit 6 (not 0); `PA_SU_PRIM_FILTER_CNTL` bits 30-31 are
XMAX_RIGHT / YMAX_BOTTOM exclusion (was "likely"); `PA_SC_MODE_CNTL_0` 0x22 is
VPORT_SCISSOR_ENABLE + ALTERNATE_RBS_PER_TILE; the RSRC1 minimum has IEEE_MODE 0.

Claims **not** re-tested because testing them risks a GPU hang (left marked
as unverified in the source): SPI_PS_INPUT_ENA = 0 hangs; PA_SU_VTX_CNTL,
PA_SC_MODE_CNTL_0/1, VGT_SHADER_STAGES_EN, SX / binner / guard-band = 1.0,
CB_COLOR0_ATTRIB2 and CMASK/FMASK "without it ... TDR / VM fault" notes;
`SPI_SHADER_COL_FORMAT` 0xE (a reserved enum); RSRC2_VS "SPI requires a
non-zero USER_SGPR".

## 7. Round 1 anomalies explained

Round 1 saw two results it could not explain: the first `VGT_INDX_OFFSET = 1`
poison left the (unfixed) render gate passing, and the `SX_MRT0_BLEND_OPT`
poison broke the render and 2D-array gates but not texture-sample.

**Cause: another GPU client rewrote the poisoned registers before the gate
drew.** The only other clients on this GPU are Xorg and `sddm-greeter-qt6`,
both radeonsi (Mesa 26.2.2), which on GFX9 emits `CONTEXT_CONTROL` +
`CLEAR_STATE` and then writes its own `PA_CL_VTE_CNTL`, SX blend-opt hints and
the VGT index clamp / offset (`ac_cmdbuf.c`) in every submission. (Their DRM
fds / fdinfo are not readable without root, so the attribution is by
elimination; `gpu_busy_percent` read a constant 99 during the session and is
not usable as an activity signal.)

Evidence (all kernel-log clean):

`vte0` poison -> sleep D -> canary (inherits only `PA_CL_VTE_CNTL`), 6 runs each:

| D | poison survived (1 px) | rewritten (full coverage) |
|---|---|---|
| 0 ms | 6 | 0 |
| 100 ms | 5 | 1 |
| 500 ms | 3 | 3 |
| 1000 ms | 0 | 6 |
| 2000 ms | 0 | 6 |
| 5000 ms | 0 | 6 |

Round-1 negative controls re-run against the pre-4.1.3 library binaries:

| poison | delay | gate | result |
|---|---|---|---|
| `n1-indxoff` | 0 | render e2e | 6/6 fail (exit 8) |
| `n1-indxoff` | 1.5 s | render e2e | 6/6 **pass** — the round-1 lone pass |
| `n3-sxopt` | 0 | texture-sample | 6/6 fail (exit 11) — texture-sample is **not** immune |
| `n3-sxopt` | 0 | render / 2D-array | 6/6 fail each |
| `n3-sxopt` | 1.5 s | render e2e | 4/4 pass |
| restores | 0 | all three | pass |

Texture-sample does the most setup (texture create + upload) between process
start and its draw, which widened its window in round 1.

Consequence for the harness: run gate binaries immediately after the poison,
repeat negative controls (`REPS=3`), and check liveness with the canary first.
Reproduce the sweep with `state_poison/run_gate.sh vte0 "run:coverage_scan
canary" <delay_ms>` (exit 30 = poison survived, 0 = rewritten).

## 8. Hardware verification (round 2)

All on the Cezanne on 2026-09-16 with the final round-2 source; the kernel log
shows no amdgpu / drm line from session start (13:56) through the last run
(15:06).

| run | library | result |
|---|---|---|
| canary `vte43f` / `vte0` (before and after) | round 2 | exit 0 / exit 30 — harness live |
| `matrix.sh clear`, `wrong` (x2), `good` | round 2 | 9 draw gates + coverage scan (64x64 red, 64x64 textured, 1920x1080 red) all pass, exact coverage |
| `matrix.sh clear`, `wrong` (x3) | pre-4.1.3 | every gate fails every repetition (texture 11, array 9, BC array 9, bilinear 10, compressed 11, cube 9, load-PNG 11, render 12 / 8, render-graph MQ 9 / 8; render coverage 1/65536 under `clear`) |
| `matrix.sh clear`, `wrong` | 4.1.3 round 1 | all pass — round 2's additions have no benign negative control (section 4) |
| `make test-native-all` | round 2 | 71 passed, 0 failed, 2 skipped (DRM master) |
| `native_all_poison.sh clear` / `wrong` | round 2 | 71 passed, 0 failed, 2 skipped; poison before all 9 draw gates |
| `radv_capture_triangle` (full-coverage check) | radv | 65536/65536; a half-screen triangle covering pixel (0,0) now fails (32640/65536) where the old pixel-(0,0) binary passed it |
| `coverage_scan all <dir>` stream dump vs `dump_streams` | round 2 | untextured stream byte-identical (so the device-derived CU pair on this GPU equals the capture); textured differs only in the 3 VA dwords (FS, RT, descriptor) |

CPU gates for the same changes (`tests/tcyr/native.tcyr`): the owned-state
walker asserts every register above in all three streams, the capacity check
rejects an undersized buffer before writing, the size tables match the
composers, and the CU derivation is pinned for 2 / 3 / 4 / 5 / 6 / 8 / 16 / 20
CUs per SA and invalid topologies. Each was shown to fail on a mutant that
re-introduces the old behaviour (capacity check removed, round-2 writes removed,
dealloc 16, pad re-added, derived pair ignored, derivation boundaries moved).
`owned_state_audit.py --check` fails on stale tables, an unclassified register
or an unjustified owned value.

## 9. Register tables

<!-- BEGIN GENERATED: owned_state_audit.py -->
### A. Registers the render streams write (137)

`clear` = kernel clear state (— = not in it); `radv` = radv's value on this GPU (explicit write in its gfx_init or main IB, else the clear state it relies on); `mabda` = the untextured pass_draw stream (textured-only writes marked *); `before` = the earlier library. Fields decode `mabda` with gfx9.json.

| MMIO | register | clear | radv | mabda | fields | verdict |
|---|---|---|---|---|---|---|
| 0B01C | SPI_SHADER_PGM_RSRC3_PS | — | 003fffff | 003fffff | CU_EN=0xffff WAVE_LIMIT=0x3f | = radv; constant on every GFX9 part (Mesa masks CU_EN only by AMD_CU_MASK). |
| 0B020 | SPI_SHADER_PGM_LO_PS | — | 00000001 | 01000010 |  | RT extent / shader or descriptor VA dependent. |
| 0B024 | SPI_SHADER_PGM_HI_PS | — | 00000080 | 00000080 | MEM_BASE=0x80 | RT extent / shader or descriptor VA dependent. |
| 0B028 | SPI_SHADER_PGM_RSRC1_PS | — | 002c0040 | 002c0040 | SGPRS=0x1 FLOAT_MODE=0xc0 DX10_CLAMP=0x1 | = radv |
| 0B02C | SPI_SHADER_PGM_RSRC2_PS | — | 00000004 | 00000004 | USER_SGPR=0x2 | = radv |
| 0B030 | SPI_SHADER_USER_DATA_PS_0 * | — | — | 00404000 |  | RT extent / shader or descriptor VA dependent. |
| 0B034 | SPI_SHADER_USER_DATA_PS_1 * | — | — | ffff8001 |  | RT extent / shader or descriptor VA dependent. |
| 0B118 | SPI_SHADER_PGM_RSRC3_VS | — | 003ffffe | 003ffffe | CU_EN=0xfffe WAVE_LIMIT=0x3f | = radv. Round 2: derived per device from the DRM_AMDGPU_INFO CU topology (native_gfx9_vs_late_alloc_derive); this Cezanne yields the capture value. |
| 0B11C | SPI_SHADER_LATE_ALLOC_VS | — | 00000018 | 00000018 | LIMIT=0x18 | as SPI_SHADER_PGM_RSRC3_VS. |
| 0B120 | SPI_SHADER_PGM_LO_VS | — | 00000000 | 01000000 |  | RT extent / shader or descriptor VA dependent. |
| 0B124 | SPI_SHADER_PGM_HI_VS | — | 00000080 | 00000080 | MEM_BASE=0x80 | RT extent / shader or descriptor VA dependent. |
| 0B128 | SPI_SHADER_PGM_RSRC1_VS | — | 002c0040 | 002c0040 | SGPRS=0x1 FLOAT_MODE=0xc0 DX10_CLAMP=0x1 | = radv |
| 0B12C | SPI_SHADER_PGM_RSRC2_VS | — | 00000006 | 00000006 | USER_SGPR=0x3 | = radv |
| 28000 | DB_RENDER_CONTROL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 2800C | DB_RENDER_OVERRIDE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28010 | DB_RENDER_OVERRIDE2 | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28030 | PA_SC_SCREEN_SCISSOR_TL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28034 | PA_SC_SCREEN_SCISSOR_BR | 40004000 | 01000100 | 00400040 | BR_X=0x40 BR_Y=0x40 | RT extent / shader or descriptor VA dependent. |
| 28038 | DB_Z_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 2803C | DB_STENCIL_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28060 | DB_DFSM_CONTROL | 00000000 | 00000002 | 00000002 | PUNCHOUT_MODE=0x2 | = radv |
| 28204 | PA_SC_WINDOW_SCISSOR_TL | 80000000 | 00000000 | 80000000 | WINDOW_OFFSET_DISABLE=0x1 | = clear state (radv 00000000) |
| 28208 | PA_SC_WINDOW_SCISSOR_BR | 40004000 | 01000100 | 40004000 | BR_X=0x4000 BR_Y=0x4000 | = clear state (radv 01000100) |
| 2820C | PA_SC_CLIPRECT_RULE | 0000ffff | 0000ffff | 0000ffff | CLIP_RULE=0xffff | = radv = clear state. Round 2 HW: a rule of 0 drew no pixel in any gate. |
| 28230 | PA_SC_EDGERULE | aa99aaaa | aa99aaaa | aa99aaaa | ER_TRI=0xa ER_POINT=0xa ER_RECT=0xa ER_LINE_LR=0x1a ER_LINE_RL=0x26 ER_LINE_TB=0xa ER_LINE_BT=0xa | = radv = clear state |
| 28234 | PA_SU_HARDWARE_SCREEN_OFFSET | 00000000 | 00080008 | 00000000 |  | = clear state (radv 00080008) |
| 28238 | CB_TARGET_MASK | ffffffff | 0000000f | 0000000f | TARGET0_ENABLE=0xf | = radv |
| 2823C | CB_SHADER_MASK | ffffffff | 0000000f | 0000000f | OUTPUT0_ENABLE=0xf | = radv |
| 28240 | PA_SC_GENERIC_SCISSOR_TL | 80000000 | 80000000 | 80000000 | WINDOW_OFFSET_DISABLE=0x1 | = radv = clear state |
| 28244 | PA_SC_GENERIC_SCISSOR_BR | 40004000 | 40004000 | 00400040 | BR_X=0x40 BR_Y=0x40 | RT extent / shader or descriptor VA dependent. |
| 28250 | PA_SC_VPORT_SCISSOR_0_TL | 80000000 | 80000000 | 80000000 | WINDOW_OFFSET_DISABLE=0x1 | = radv = clear state |
| 28254 | PA_SC_VPORT_SCISSOR_0_BR | 40004000 | 01000100 | 40004000 | BR_X=0x4000 BR_Y=0x4000 | = clear state (radv 01000100) |
| 282D0 | PA_SC_VPORT_ZMIN_0 | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 282D4 | PA_SC_VPORT_ZMAX_0 | 3f800000 | 3f800000 | 3f800000 |  | = radv = clear state |
| 28350 | PA_SC_RASTER_CONFIG | — | — | 00000000 |  | not in the clear state; Mesa writes it only for GFX8 and older (ac_emit_raster_config) and linux v7.2.3 gfx_v9_0.c does not program it. Pre-4.1.3 write, kept: every gate incl. the 1920x1080 full-coverage scan passes with it (this GPU reports 2 RBs). |
| 28358 | PA_SC_SCREEN_EXTENT_CONTROL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 2835C | PA_SC_TILE_STEERING_OVERRIDE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 2843C | PA_CL_VPORT_XSCALE | 00000000 | 43000000 | 42000000 |  | RT extent / shader or descriptor VA dependent. |
| 28440 | PA_CL_VPORT_XOFFSET | 00000000 | 43000000 | 42000000 |  | RT extent / shader or descriptor VA dependent. |
| 28444 | PA_CL_VPORT_YSCALE | 00000000 | 43000000 | c2000000 |  | RT extent / shader or descriptor VA dependent. |
| 28448 | PA_CL_VPORT_YOFFSET | 00000000 | 43000000 | 42000000 |  | RT extent / shader or descriptor VA dependent. |
| 2844C | PA_CL_VPORT_ZSCALE | 00000000 | 3f800000 | 3f800000 |  | = radv |
| 28450 | PA_CL_VPORT_ZOFFSET | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 286C4 | SPI_VS_OUT_CONFIG | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 286CC | SPI_PS_INPUT_ENA | 00000000 | 00000080 | 00000002 | PERSP_CENTER_ENA=0x1 | PERSP_CENTER_ENA. Round 2 HW re-test: radv's 0x80 and LINEAR_CENTER 0x20 draw identically (old "black pixels" note retracted). Textured draws override to 0x302. |
| 286D0 | SPI_PS_INPUT_ADDR | 00000000 | 00000080 | 00000002 | PERSP_CENTER_ENA=0x1 | as SPI_PS_INPUT_ENA. |
| 286D8 | SPI_PS_IN_CONTROL | 00000002 | 00000000 | 00000000 |  | = radv |
| 286E0 | SPI_BARYC_CNTL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 286E8 | SPI_TMPRING_SIZE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 2870C | SPI_SHADER_POS_FORMAT | 00000000 | 00000004 | 00000004 | POS0_EXPORT_FORMAT=0x4 | = radv |
| 28710 | SPI_SHADER_Z_FORMAT | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28714 | SPI_SHADER_COL_FORMAT | 00000000 | 00000004 | 00000009 | COL0_EXPORT_FORMAT=0x9 | SPI_SHADER_32_ABGR (mabda's FS exports f32); radv exports FP16_ABGR. Both valid export formats; gates read back exact values. |
| 28754 | SX_PS_DOWNCONVERT | 00000000 | 00000005 | 00000000 |  | = clear state (radv 00000005) |
| 28758 | SX_BLEND_OPT_EPSILON | 00000000 | 00000006 | 00000000 |  | = clear state (radv 00000006) |
| 2875C | SX_BLEND_OPT_CONTROL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28760 | SX_MRT0_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 28764 | SX_MRT1_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 28768 | SX_MRT2_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 2876C | SX_MRT3_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 28770 | SX_MRT4_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 28774 | SX_MRT5_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 28778 | SX_MRT6_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 2877C | SX_MRT7_BLEND_OPT | 00000000 | 06000600 | 06000600 | COLOR_COMB_FCN=0x6 ALPHA_COMB_FCN=0x6 | = radv |
| 28780 | CB_BLEND0_CONTROL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 287A0 | CB_MRT0_EPITCH | 00000000 | 000000ff | 0000003f | EPITCH=0x3f | RT extent / shader or descriptor VA dependent. |
| 28800 | DB_DEPTH_CONTROL | 00000000 | 00700770 | 00000000 |  | = clear state (radv 00700770) |
| 28804 | DB_EQAA | 00000000 | 00130000 | 00000000 |  | = clear state (radv 00130000) |
| 28808 | CB_COLOR_CONTROL | 00000000 | 00cc0010 | 00cc0010 | MODE=0x1 ROP3=0xcc | = radv |
| 2880C | DB_SHADER_CONTROL | 00000000 | 00000010 | 00000010 | Z_ORDER=0x1 | = radv |
| 28810 | PA_CL_CLIP_CNTL | 00090000 | 01080000 | 00090000 | CLIP_DISABLE=0x1 DX_CLIP_SPACE_DEF=0x1 | = clear state (clipping disabled). Round 2 HW re-test: radv's 0x01080000 (clipping enabled) draws identically (old "black pixels" note retracted). |
| 28814 | PA_SU_SC_MODE_CNTL | 00000004 | 00000240 | 00000004 | FACE=0x1 | = clear state. Round 2 HW re-test under the clear poison: radv's 0x240 draws identically (old "black pixels" note retracted); culling off makes FACE / poly-mode types neutral. |
| 28818 | PA_CL_VTE_CNTL | 00000000 | 0000043f | 0000043f | VPORT_X_SCALE_ENA=0x1 VPORT_X_OFFSET_ENA=0x1 VPORT_Y_SCALE_ENA=0x1 VPORT_Y_OFFSET_ENA=0x1 VPORT_Z_SCALE_ENA=0x1 VPORT_Z_OFFSET_ENA=0x1 VTX_W0_FMT=0x1 | = radv |
| 2881C | PA_CL_VS_OUT_CNTL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28820 | PA_CL_NANINF_CNTL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 2882C | PA_SU_PRIM_FILTER_CNTL | 00000000 | c0000000 | c0000000 | XMAX_RIGHT_EXCLUSION=0x1 YMAX_BOTTOM_EXCLUSION=0x1 | = radv |
| 28830 | PA_SU_SMALL_PRIM_FILTER_CNTL | 00000000 | 00000001 | 00000000 |  | = clear state (radv 00000001) |
| 2883C | PA_SU_OVER_RASTERIZATION_CNTL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28840 | PA_STEREO_CNTL | — | — | 00000000 |  | round 2: not in the GFX9 clear state and never written by radv on GFX9, so no CLEAR_STATE ever resets it; Mesa writes 0 on GFX11 (no CLEAR_STATE). |
| 28A10 | VGT_OUTPUT_PATH_CNTL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28A14 | VGT_HOS_CNTL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28A40 | VGT_GS_MODE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28A48 | PA_SC_MODE_CNTL_0 | 00000000 | 00000022 | 00000022 | VPORT_SCISSOR_ENABLE=0x1 ALTERNATE_RBS_PER_TILE=0x1 | = radv |
| 28A4C | PA_SC_MODE_CNTL_1 | 00000000 | 760201bc | 760201bc | WALK_ALIGN8_PRIM_FITS_ST=0x1 WALK_FENCE_ENABLE=0x1 WALK_FENCE_SIZE=0x3 SUPERTILE_WALK_ORDER_ENABLE=0x1 TILE_WALK_ORDER_ENABLE=0x1 MULTI_SHADER_ENGINE_PRIM_DISCARD_ENABLE=0x1 FORCE_EOV_CNTDWN_ENABLE=0x1 FORCE_EOV_REZ_ENABLE=0x1 OUT_OF_ORDER_WATER_MARK=0x7 | = radv |
| 28A50 | VGT_ENHANCE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28A6C | VGT_GS_OUT_PRIM_TYPE | 00000000 | 00000002 | 00000002 | OUTPRIM_TYPE=0x2 | = radv |
| 28A70 | IA_ENHANCE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28A80 | WD_ENHANCE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28A84 | VGT_PRIMITIVEID_EN | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28A98 | VGT_DRAW_PAYLOAD_CNTL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28AB4 | VGT_REUSE_OFF | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28AB8 | VGT_VTX_CNT_EN | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28ABC | DB_HTILE_SURFACE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28B54 | VGT_SHADER_STAGES_EN | 00000000 | 00010000 | 00010000 | MAX_PRIMGRP_IN_WAVE=0x2 | = radv |
| 28B70 | DB_ALPHA_TO_MASK | 00000000 | 00018700 | 00000000 |  | = clear state (radv 00018700) |
| 28B94 | VGT_STRMOUT_CONFIG | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28B98 | VGT_STRMOUT_BUFFER_CONFIG | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28BE0 | PA_SC_AA_CONFIG | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28BE4 | PA_SU_VTX_CNTL | 00000005 | 0000002d | 00000005 | PIX_CENTER=0x1 ROUND_MODE=0x2 | = clear state (radv 0000002d) |
| 28BE8 | PA_CL_GB_VERT_CLIP_ADJ | 3f800000 | 43800000 | 437efe00 |  | guard band ~254.99 vs radv 256.0 (GFX9_PA_CL_GB_CLIP_ADJ_F32; must not be 1.0). |
| 28BEC | PA_CL_GB_VERT_DISC_ADJ | 3f800000 | 3f800000 | 3f800000 |  | = radv = clear state |
| 28BF0 | PA_CL_GB_HORZ_CLIP_ADJ | 3f800000 | 43800000 | 437efe00 |  | as PA_CL_GB_VERT_CLIP_ADJ. |
| 28BF4 | PA_CL_GB_HORZ_DISC_ADJ | 3f800000 | 3f800000 | 3f800000 |  | = radv = clear state |
| 28C38 | PA_SC_AA_MASK_X0Y0_X1Y0 | ffffffff | ffffffff | ffffffff | AA_MASK_X0Y0=0xffff AA_MASK_X1Y0=0xffff | = radv = clear state. Round 2 HW: a mask of 0 drew no pixel in any gate. |
| 28C3C | PA_SC_AA_MASK_X0Y1_X1Y1 | ffffffff | ffffffff | ffffffff | AA_MASK_X0Y1=0xffff AA_MASK_X1Y1=0xffff | as PA_SC_AA_MASK_X0Y0_X1Y0. |
| 28C40 | PA_SC_SHADER_CONTROL | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28C44 | PA_SC_BINNER_CNTL_0 | 00000003 | 19fc0900 | 10040003 | BINNING_MODE=0x3 DISABLE_START_OF_PRIM=0x1 FLUSH_ON_BINNING_TRANSITION=0x1 | binning disabled (DISABLE_BINNING_USE_LEGACY_SC + Mesa's GFX9 disable pair); radv runs BINNING_ALLOWED here. DB_DFSM_CONTROL FORCE_OFF pairs with it. |
| 28C48 | PA_SC_BINNER_CNTL_1 | 00000000 | 03ff007f | 03ff001f | MAX_ALLOC_COUNT=0x1f MAX_PRIM_PER_BATCH=0x3ff | binner batch limits (only consulted with binning allowed). |
| 28C4C | PA_SC_CONSERVATIVE_RASTERIZATION_CNTL | 00000000 | 00100000 | 00000000 |  | = clear state (radv 00100000) |
| 28C58 | VGT_VERTEX_REUSE_BLOCK_CNTL | 0000001e | 0000001e | 0000001e | VTX_REUSE_DEPTH=0x1e | = radv = clear state |
| 28C5C | VGT_OUT_DEALLOC_CNTL | 00000020 | 00000020 | 00000020 | DEALLOC_DIST=0x20 | = radv = clear state; round 2 corrected 16 -> 32 (the 30 / 32 pair radv and radeonsi run with on this GPU). |
| 28C60 | CB_COLOR0_BASE | 00000000 | 01000200 | 01010000 |  | RT extent / shader or descriptor VA dependent. |
| 28C64 | CB_COLOR0_BASE_EXT | 00000000 | 00000080 | 00000080 | BASE_256B=0x80 | RT extent / shader or descriptor VA dependent. |
| 28C68 | CB_COLOR0_ATTRIB2 | 00000000 | 003fc0ff | 000fc03f | MIP0_HEIGHT=0x3f MIP0_WIDTH=0x3f | RT extent / shader or descriptor VA dependent. |
| 28C6C | CB_COLOR0_VIEW | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28C70 | CB_COLOR0_INFO | 00000000 | 00028028 | 00028028 | FORMAT=0xa BLEND_CLAMP=0x1 SIMPLE_FLOAT=0x1 | = radv. Round 2 HW re-test: mabda's earlier 0x04000028 also draws identically (old "CB wrote zeros" note retracted). |
| 28C74 | CB_COLOR0_ATTRIB | 00000000 | d0000000 | 10000000 | RESOURCE_TYPE=0x1 | RESOURCE_TYPE 1; radv additionally sets RB_ALIGNED / PIPE_ALIGNED for its aligned surface, mabda's linear RT is not. |
| 28C78 | CB_COLOR0_DCC_CONTROL | 00000000 | 00000218 | 00000000 |  | = clear state (radv 00000218) |
| 28C7C | CB_COLOR0_CMASK | 00000000 | 01000200 | 00000000 |  | = clear state (radv 01000200) |
| 28C80 | CB_COLOR0_CMASK_BASE_EXT | 00000000 | 00000080 | 00000000 |  | = clear state (radv 00000080) |
| 28C84 | CB_COLOR0_FMASK | 00000000 | 01000200 | 00000000 |  | = clear state (radv 01000200) |
| 28C88 | CB_COLOR0_FMASK_BASE_EXT | 00000000 | 00000080 | 00000000 |  | = clear state (radv 00000080) |
| 28C94 | CB_COLOR0_DCC_BASE | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28C98 | CB_COLOR0_DCC_BASE_EXT | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28CAC | CB_COLOR1_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28CE8 | CB_COLOR2_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28D24 | CB_COLOR3_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28D60 | CB_COLOR4_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28D9C | CB_COLOR5_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28DD8 | CB_COLOR6_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 28E14 | CB_COLOR7_INFO | 00000000 | 00000000 | 00000000 |  | = radv = clear state |
| 30908 | VGT_PRIMITIVE_TYPE | — | 00000004 | 00000004 | PRIM_TYPE=0x4 | = radv |
| 30920 | VGT_MAX_VTX_INDX | — | ffffffff | ffffffff |  | = radv |
| 30924 | VGT_MIN_VTX_INDX | — | 00000000 | 00000000 |  | = radv |
| 30928 | VGT_INDX_OFFSET | — | 00000000 | 00000000 |  | = radv |
| 3092C | VGT_MULTI_PRIM_IB_RESET_EN | — | 00000000 | 00000000 |  | = radv |
| 30960 | IA_MULTI_VGT_PARAM | — | 0070007f | 0070007f | PRIMGROUP_SIZE=0x7f WD_SWITCH_ON_EOP=0x1 EN_INST_OPT_BASIC=0x1 EN_INST_OPT_ADV=0x1 | = radv |
| 30968 | VGT_INSTANCE_BASE_ID | — | 00000000 | 00000000 |  | = radv |

### B. Context registers not written (478)

| family | registers | clear-state values | radv writes | verdict |
|---|---|---|---|---|
| depth / stencil buffer | DB_COUNT_CONTROL (28004) DB_DEPTH_VIEW (28008) DB_HTILE_DATA_BASE (28014) DB_HTILE_DATA_BASE_HI (28018) DB_DEPTH_SIZE (2801C) DB_DEPTH_BOUNDS_MIN (28020) DB_DEPTH_BOUNDS_MAX (28024) DB_STENCIL_CLEAR (28028) DB_DEPTH_CLEAR (2802C) DB_Z_READ_BASE (28040) DB_Z_READ_BASE_HI (28044) DB_STENCIL_READ_BASE (28048) DB_STENCIL_READ_BASE_HI (2804C) DB_Z_WRITE_BASE (28050) DB_Z_WRITE_BASE_HI (28054) DB_STENCIL_WRITE_BASE (28058) DB_STENCIL_WRITE_BASE_HI (2805C) DB_Z_INFO2 (28068) DB_STENCIL_INFO2 (2806C) DB_STENCIL_CONTROL (2842C) DB_STENCILREFMASK (28430) DB_STENCILREFMASK_BF (28434) DB_SRESULTS_COMPARE_STATE0 (28AC0) DB_SRESULTS_COMPARE_STATE1 (28AC4) DB_PRELOAD_CONTROL (28AC8) | 00000000, 01000000 | DB_COUNT_CONTROL=00000001 | No depth-stencil attachment: DB_Z_INFO / DB_STENCIL_INFO FORMAT = INVALID, DB_DEPTH_CONTROL = 0 (no test, no write), DB_RENDER_CONTROL / _OVERRIDE / _OVERRIDE2 = 0, DB_SHADER_CONTROL written. The DB never reads these; DB_COUNT_CONTROL only feeds occlusion counters. |
| border colour table | TA_BC_BASE_ADDR (28080) TA_BC_BASE_ADDR_HI (28084) | all 00000000 | TA_BC_BASE_ADDR=00000000 TA_BC_BASE_ADDR_HI=00000000 | Only for CLAMP_TO_BORDER addressing; mabda samplers use point/bilinear with clamp/repeat. |
| coherent-copy destinations | COHER_DEST_BASE_HI_0 (281E8) COHER_DEST_BASE_HI_1 (281EC) COHER_DEST_BASE_HI_2 (281F0) COHER_DEST_BASE_HI_3 (281F4) COHER_DEST_BASE_2 (281F8) COHER_DEST_BASE_3 (281FC) COHER_DEST_BASE_0 (28248) COHER_DEST_BASE_1 (2824C) | all 00000000 | — | CP state-copy destinations; Mesa only lists them in its register-shadowing tables (ac_shadowed_regs.c), never writes them for draws. |
| window offset | PA_SC_WINDOW_OFFSET (28200) | all 00000000 | — | Applied only with PA_SU_SC_MODE_CNTL.VTX_WINDOW_OFFSET_ENABLE (written 0) and to scissors without WINDOW_OFFSET_DISABLE (window / generic / vport scissor TL written with it set). |
| clip rectangles | PA_SC_CLIPRECT_0_TL (28210) PA_SC_CLIPRECT_0_BR (28214) PA_SC_CLIPRECT_1_TL (28218) PA_SC_CLIPRECT_1_BR (2821C) PA_SC_CLIPRECT_2_TL (28220) PA_SC_CLIPRECT_2_BR (28224) PA_SC_CLIPRECT_3_TL (28228) PA_SC_CLIPRECT_3_BR (2822C) | 00000000, 40004000 | — | PA_SC_CLIPRECT_RULE written 0xFFFF: every inside/outside combination passes (round 2 HW: a rule of 0 drew nothing), so the rectangles never reject a pixel. |
| viewports 1..15 | PA_SC_VPORT_SCISSOR_1_TL (28258) PA_SC_VPORT_SCISSOR_1_BR (2825C) PA_SC_VPORT_SCISSOR_2_TL (28260) PA_SC_VPORT_SCISSOR_2_BR (28264) PA_SC_VPORT_SCISSOR_3_TL (28268) PA_SC_VPORT_SCISSOR_3_BR (2826C) PA_SC_VPORT_SCISSOR_4_TL (28270) PA_SC_VPORT_SCISSOR_4_BR (28274) PA_SC_VPORT_SCISSOR_5_TL (28278) PA_SC_VPORT_SCISSOR_5_BR (2827C) PA_SC_VPORT_SCISSOR_6_TL (28280) PA_SC_VPORT_SCISSOR_6_BR (28284) PA_SC_VPORT_SCISSOR_7_TL (28288) PA_SC_VPORT_SCISSOR_7_BR (2828C) PA_SC_VPORT_SCISSOR_8_TL (28290) PA_SC_VPORT_SCISSOR_8_BR (28294) PA_SC_VPORT_SCISSOR_9_TL (28298) PA_SC_VPORT_SCISSOR_9_BR (2829C) PA_SC_VPORT_SCISSOR_10_TL (282A0) PA_SC_VPORT_SCISSOR_10_BR (282A4) PA_SC_VPORT_SCISSOR_11_TL (282A8) PA_SC_VPORT_SCISSOR_11_BR (282AC) PA_SC_VPORT_SCISSOR_12_TL (282B0) PA_SC_VPORT_SCISSOR_12_BR (282B4) PA_SC_VPORT_SCISSOR_13_TL (282B8) PA_SC_VPORT_SCISSOR_13_BR (282BC) PA_SC_VPORT_SCISSOR_14_TL (282C0) PA_SC_VPORT_SCISSOR_14_BR (282C4) PA_SC_VPORT_SCISSOR_15_TL (282C8) PA_SC_VPORT_SCISSOR_15_BR (282CC) PA_SC_VPORT_ZMIN_1 (282D8) PA_SC_VPORT_ZMAX_1 (282DC) PA_SC_VPORT_ZMIN_2 (282E0) PA_SC_VPORT_ZMAX_2 (282E4) PA_SC_VPORT_ZMIN_3 (282E8) PA_SC_VPORT_ZMAX_3 (282EC) PA_SC_VPORT_ZMIN_4 (282F0) PA_SC_VPORT_ZMAX_4 (282F4) PA_SC_VPORT_ZMIN_5 (282F8) PA_SC_VPORT_ZMAX_5 (282FC) PA_SC_VPORT_ZMIN_6 (28300) PA_SC_VPORT_ZMAX_6 (28304) PA_SC_VPORT_ZMIN_7 (28308) PA_SC_VPORT_ZMAX_7 (2830C) PA_SC_VPORT_ZMIN_8 (28310) PA_SC_VPORT_ZMAX_8 (28314) PA_SC_VPORT_ZMIN_9 (28318) PA_SC_VPORT_ZMAX_9 (2831C) PA_SC_VPORT_ZMIN_10 (28320) PA_SC_VPORT_ZMAX_10 (28324) PA_SC_VPORT_ZMIN_11 (28328) PA_SC_VPORT_ZMAX_11 (2832C) PA_SC_VPORT_ZMIN_12 (28330) PA_SC_VPORT_ZMAX_12 (28334) PA_SC_VPORT_ZMIN_13 (28338) PA_SC_VPORT_ZMAX_13 (2833C) PA_SC_VPORT_ZMIN_14 (28340) PA_SC_VPORT_ZMAX_14 (28344) PA_SC_VPORT_ZMIN_15 (28348) PA_SC_VPORT_ZMAX_15 (2834C) PA_CL_VPORT_XSCALE_1 (28454) PA_CL_VPORT_XOFFSET_1 (28458) PA_CL_VPORT_YSCALE_1 (2845C) PA_CL_VPORT_YOFFSET_1 (28460) PA_CL_VPORT_ZSCALE_1 (28464) PA_CL_VPORT_ZOFFSET_1 (28468) PA_CL_VPORT_XSCALE_2 (2846C) PA_CL_VPORT_XOFFSET_2 (28470) PA_CL_VPORT_YSCALE_2 (28474) PA_CL_VPORT_YOFFSET_2 (28478) PA_CL_VPORT_ZSCALE_2 (2847C) PA_CL_VPORT_ZOFFSET_2 (28480) PA_CL_VPORT_XSCALE_3 (28484) PA_CL_VPORT_XOFFSET_3 (28488) PA_CL_VPORT_YSCALE_3 (2848C) PA_CL_VPORT_YOFFSET_3 (28490) PA_CL_VPORT_ZSCALE_3 (28494) PA_CL_VPORT_ZOFFSET_3 (28498) PA_CL_VPORT_XSCALE_4 (2849C) PA_CL_VPORT_XOFFSET_4 (284A0) PA_CL_VPORT_YSCALE_4 (284A4) PA_CL_VPORT_YOFFSET_4 (284A8) PA_CL_VPORT_ZSCALE_4 (284AC) PA_CL_VPORT_ZOFFSET_4 (284B0) PA_CL_VPORT_XSCALE_5 (284B4) PA_CL_VPORT_XOFFSET_5 (284B8) PA_CL_VPORT_YSCALE_5 (284BC) PA_CL_VPORT_YOFFSET_5 (284C0) PA_CL_VPORT_ZSCALE_5 (284C4) PA_CL_VPORT_ZOFFSET_5 (284C8) PA_CL_VPORT_XSCALE_6 (284CC) PA_CL_VPORT_XOFFSET_6 (284D0) PA_CL_VPORT_YSCALE_6 (284D4) PA_CL_VPORT_YOFFSET_6 (284D8) PA_CL_VPORT_ZSCALE_6 (284DC) PA_CL_VPORT_ZOFFSET_6 (284E0) PA_CL_VPORT_XSCALE_7 (284E4) PA_CL_VPORT_XOFFSET_7 (284E8) PA_CL_VPORT_YSCALE_7 (284EC) PA_CL_VPORT_YOFFSET_7 (284F0) PA_CL_VPORT_ZSCALE_7 (284F4) PA_CL_VPORT_ZOFFSET_7 (284F8) PA_CL_VPORT_XSCALE_8 (284FC) PA_CL_VPORT_XOFFSET_8 (28500) PA_CL_VPORT_YSCALE_8 (28504) PA_CL_VPORT_YOFFSET_8 (28508) PA_CL_VPORT_ZSCALE_8 (2850C) PA_CL_VPORT_ZOFFSET_8 (28510) PA_CL_VPORT_XSCALE_9 (28514) PA_CL_VPORT_XOFFSET_9 (28518) PA_CL_VPORT_YSCALE_9 (2851C) PA_CL_VPORT_YOFFSET_9 (28520) PA_CL_VPORT_ZSCALE_9 (28524) PA_CL_VPORT_ZOFFSET_9 (28528) PA_CL_VPORT_XSCALE_10 (2852C) PA_CL_VPORT_XOFFSET_10 (28530) PA_CL_VPORT_YSCALE_10 (28534) PA_CL_VPORT_YOFFSET_10 (28538) PA_CL_VPORT_ZSCALE_10 (2853C) PA_CL_VPORT_ZOFFSET_10 (28540) PA_CL_VPORT_XSCALE_11 (28544) PA_CL_VPORT_XOFFSET_11 (28548) PA_CL_VPORT_YSCALE_11 (2854C) PA_CL_VPORT_YOFFSET_11 (28550) PA_CL_VPORT_ZSCALE_11 (28554) PA_CL_VPORT_ZOFFSET_11 (28558) PA_CL_VPORT_XSCALE_12 (2855C) PA_CL_VPORT_XOFFSET_12 (28560) PA_CL_VPORT_YSCALE_12 (28564) PA_CL_VPORT_YOFFSET_12 (28568) PA_CL_VPORT_ZSCALE_12 (2856C) PA_CL_VPORT_ZOFFSET_12 (28570) PA_CL_VPORT_XSCALE_13 (28574) PA_CL_VPORT_XOFFSET_13 (28578) PA_CL_VPORT_YSCALE_13 (2857C) PA_CL_VPORT_YOFFSET_13 (28580) PA_CL_VPORT_ZSCALE_13 (28584) PA_CL_VPORT_ZOFFSET_13 (28588) PA_CL_VPORT_XSCALE_14 (2858C) PA_CL_VPORT_XOFFSET_14 (28590) PA_CL_VPORT_YSCALE_14 (28594) PA_CL_VPORT_YOFFSET_14 (28598) PA_CL_VPORT_ZSCALE_14 (2859C) PA_CL_VPORT_ZOFFSET_14 (285A0) PA_CL_VPORT_XSCALE_15 (285A4) PA_CL_VPORT_XOFFSET_15 (285A8) PA_CL_VPORT_YSCALE_15 (285AC) PA_CL_VPORT_YOFFSET_15 (285B0) PA_CL_VPORT_ZSCALE_15 (285B4) PA_CL_VPORT_ZOFFSET_15 (285B8) | 00000000, 3f800000, 40004000, 80000000 | — | Viewport index is always 0: PA_CL_VS_OUT_CNTL = 0 (no USE_VTX_VIEWPORT_INDX), PA_STEREO_CNTL = 0 (no VP_ID_MODE). Viewport 0 is written in full. |
| multi-SE raster config | PA_SC_RASTER_CONFIG_1 (28354) | not in clear state | — | Shader-engine pairing for multi-SE parts; this GPU has 1 SE and Mesa writes it only for GFX8 and older. |
| CP bookkeeping | CP_PERFMON_CNTX_CNTL (28360) CP_PIPEID (28364) CP_VMID (28368) | all 00000000 | — | Per-context CP perfmon / pipe / VMID bookkeeping maintained by the CP and kernel, not draw state. |
| MSAA sample grid | PA_SC_RIGHT_VERT_GRID (283A0) PA_SC_LEFT_VERT_GRID (283A4) PA_SC_HORIZ_GRID (283A8) | all 00000000 | — | MSAA grid; PA_SC_AA_CONFIG = 0 (1x). Round 1 HW: moving 1x sample locations changed no pixel. |
| primitive restart index | VGT_MULTI_PRIM_IB_RESET_INDX (2840C) | all 00000000 | — | VGT_MULTI_PRIM_IB_RESET_EN written 0. |
| blend constant | CB_BLEND_RED (28414) CB_BLEND_GREEN (28418) CB_BLEND_BLUE (2841C) CB_BLEND_ALPHA (28420) | all 00000000 | — | Blending off: CB_BLEND0_CONTROL written 0. |
| DCC / overwrite combiner | CB_DCC_CONTROL (28424) | all 00000000 | CB_DCC_CONTROL=00000412 | Compressed-surface (DCC) encode controls; CB_COLOR0_INFO DCC_ENABLE = 0, no DCC surface bound. |
| user clip planes | PA_CL_UCP_0_X (285BC) PA_CL_UCP_0_Y (285C0) PA_CL_UCP_0_Z (285C4) PA_CL_UCP_0_W (285C8) PA_CL_UCP_1_X (285CC) PA_CL_UCP_1_Y (285D0) PA_CL_UCP_1_Z (285D4) PA_CL_UCP_1_W (285D8) PA_CL_UCP_2_X (285DC) PA_CL_UCP_2_Y (285E0) PA_CL_UCP_2_Z (285E4) PA_CL_UCP_2_W (285E8) PA_CL_UCP_3_X (285EC) PA_CL_UCP_3_Y (285F0) PA_CL_UCP_3_Z (285F4) PA_CL_UCP_3_W (285F8) PA_CL_UCP_4_X (285FC) PA_CL_UCP_4_Y (28600) PA_CL_UCP_4_Z (28604) PA_CL_UCP_4_W (28608) PA_CL_UCP_5_X (2860C) PA_CL_UCP_5_Y (28610) PA_CL_UCP_5_Z (28614) PA_CL_UCP_5_W (28618) PA_CL_PROG_NEAR_CLIP_Z (2861C) | all 00000000 (24 of 25 in it) | — | PA_CL_CLIP_CNTL written with UCP_ENA_0..5 = 0, ZCLIP_PROG_NEAR_ENA = 0 and CLIP_DISABLE = 1. |
| PS varying inputs | SPI_PS_INPUT_CNTL_0 (28644) SPI_PS_INPUT_CNTL_1 (28648) SPI_PS_INPUT_CNTL_2 (2864C) SPI_PS_INPUT_CNTL_3 (28650) SPI_PS_INPUT_CNTL_4 (28654) SPI_PS_INPUT_CNTL_5 (28658) SPI_PS_INPUT_CNTL_6 (2865C) SPI_PS_INPUT_CNTL_7 (28660) SPI_PS_INPUT_CNTL_8 (28664) SPI_PS_INPUT_CNTL_9 (28668) SPI_PS_INPUT_CNTL_10 (2866C) SPI_PS_INPUT_CNTL_11 (28670) SPI_PS_INPUT_CNTL_12 (28674) SPI_PS_INPUT_CNTL_13 (28678) SPI_PS_INPUT_CNTL_14 (2867C) SPI_PS_INPUT_CNTL_15 (28680) SPI_PS_INPUT_CNTL_16 (28684) SPI_PS_INPUT_CNTL_17 (28688) SPI_PS_INPUT_CNTL_18 (2868C) SPI_PS_INPUT_CNTL_19 (28690) SPI_PS_INPUT_CNTL_20 (28694) SPI_PS_INPUT_CNTL_21 (28698) SPI_PS_INPUT_CNTL_22 (2869C) SPI_PS_INPUT_CNTL_23 (286A0) SPI_PS_INPUT_CNTL_24 (286A4) SPI_PS_INPUT_CNTL_25 (286A8) SPI_PS_INPUT_CNTL_26 (286AC) SPI_PS_INPUT_CNTL_27 (286B0) SPI_PS_INPUT_CNTL_28 (286B4) SPI_PS_INPUT_CNTL_29 (286B8) SPI_PS_INPUT_CNTL_30 (286BC) SPI_PS_INPUT_CNTL_31 (286C0) SPI_INTERP_CONTROL_0 (286D4) | all 00000000 | SPI_INTERP_CONTROL_0=0000086b | No interpolated varyings: SPI_VS_OUT_CONFIG VS_EXPORT_COUNT = 0 and SPI_PS_IN_CONTROL NUM_INTERP = 0 (written); screen position arrives via SPI_PS_INPUT_ENA POS_*_FLOAT (written). |
| MRT1..7 | CB_BLEND1_CONTROL (28784) CB_BLEND2_CONTROL (28788) CB_BLEND3_CONTROL (2878C) CB_BLEND4_CONTROL (28790) CB_BLEND5_CONTROL (28794) CB_BLEND6_CONTROL (28798) CB_BLEND7_CONTROL (2879C) CB_MRT1_EPITCH (287A4) CB_MRT2_EPITCH (287A8) CB_MRT3_EPITCH (287AC) CB_MRT4_EPITCH (287B0) CB_MRT5_EPITCH (287B4) CB_MRT6_EPITCH (287B8) CB_MRT7_EPITCH (287BC) CB_COLOR1_BASE (28C9C) CB_COLOR1_BASE_EXT (28CA0) CB_COLOR1_ATTRIB2 (28CA4) CB_COLOR1_VIEW (28CA8) CB_COLOR1_ATTRIB (28CB0) CB_COLOR1_DCC_CONTROL (28CB4) CB_COLOR1_CMASK (28CB8) CB_COLOR1_CMASK_BASE_EXT (28CBC) CB_COLOR1_FMASK (28CC0) CB_COLOR1_FMASK_BASE_EXT (28CC4) CB_COLOR1_CLEAR_WORD0 (28CC8) CB_COLOR1_CLEAR_WORD1 (28CCC) CB_COLOR1_DCC_BASE (28CD0) CB_COLOR1_DCC_BASE_EXT (28CD4) CB_COLOR2_BASE (28CD8) CB_COLOR2_BASE_EXT (28CDC) CB_COLOR2_ATTRIB2 (28CE0) CB_COLOR2_VIEW (28CE4) CB_COLOR2_ATTRIB (28CEC) CB_COLOR2_DCC_CONTROL (28CF0) CB_COLOR2_CMASK (28CF4) CB_COLOR2_CMASK_BASE_EXT (28CF8) CB_COLOR2_FMASK (28CFC) CB_COLOR2_FMASK_BASE_EXT (28D00) CB_COLOR2_CLEAR_WORD0 (28D04) CB_COLOR2_CLEAR_WORD1 (28D08) CB_COLOR2_DCC_BASE (28D0C) CB_COLOR2_DCC_BASE_EXT (28D10) CB_COLOR3_BASE (28D14) CB_COLOR3_BASE_EXT (28D18) CB_COLOR3_ATTRIB2 (28D1C) CB_COLOR3_VIEW (28D20) CB_COLOR3_ATTRIB (28D28) CB_COLOR3_DCC_CONTROL (28D2C) CB_COLOR3_CMASK (28D30) CB_COLOR3_CMASK_BASE_EXT (28D34) CB_COLOR3_FMASK (28D38) CB_COLOR3_FMASK_BASE_EXT (28D3C) CB_COLOR3_CLEAR_WORD0 (28D40) CB_COLOR3_CLEAR_WORD1 (28D44) CB_COLOR3_DCC_BASE (28D48) CB_COLOR3_DCC_BASE_EXT (28D4C) CB_COLOR4_BASE (28D50) CB_COLOR4_BASE_EXT (28D54) CB_COLOR4_ATTRIB2 (28D58) CB_COLOR4_VIEW (28D5C) CB_COLOR4_ATTRIB (28D64) CB_COLOR4_DCC_CONTROL (28D68) CB_COLOR4_CMASK (28D6C) CB_COLOR4_CMASK_BASE_EXT (28D70) CB_COLOR4_FMASK (28D74) CB_COLOR4_FMASK_BASE_EXT (28D78) CB_COLOR4_CLEAR_WORD0 (28D7C) CB_COLOR4_CLEAR_WORD1 (28D80) CB_COLOR4_DCC_BASE (28D84) CB_COLOR4_DCC_BASE_EXT (28D88) CB_COLOR5_BASE (28D8C) CB_COLOR5_BASE_EXT (28D90) CB_COLOR5_ATTRIB2 (28D94) CB_COLOR5_VIEW (28D98) CB_COLOR5_ATTRIB (28DA0) CB_COLOR5_DCC_CONTROL (28DA4) CB_COLOR5_CMASK (28DA8) CB_COLOR5_CMASK_BASE_EXT (28DAC) CB_COLOR5_FMASK (28DB0) CB_COLOR5_FMASK_BASE_EXT (28DB4) CB_COLOR5_CLEAR_WORD0 (28DB8) CB_COLOR5_CLEAR_WORD1 (28DBC) CB_COLOR5_DCC_BASE (28DC0) CB_COLOR5_DCC_BASE_EXT (28DC4) CB_COLOR6_BASE (28DC8) CB_COLOR6_BASE_EXT (28DCC) CB_COLOR6_ATTRIB2 (28DD0) CB_COLOR6_VIEW (28DD4) CB_COLOR6_ATTRIB (28DDC) CB_COLOR6_DCC_CONTROL (28DE0) CB_COLOR6_CMASK (28DE4) CB_COLOR6_CMASK_BASE_EXT (28DE8) CB_COLOR6_FMASK (28DEC) CB_COLOR6_FMASK_BASE_EXT (28DF0) CB_COLOR6_CLEAR_WORD0 (28DF4) CB_COLOR6_CLEAR_WORD1 (28DF8) CB_COLOR6_DCC_BASE (28DFC) CB_COLOR6_DCC_BASE_EXT (28E00) CB_COLOR7_BASE (28E04) CB_COLOR7_BASE_EXT (28E08) CB_COLOR7_ATTRIB2 (28E0C) CB_COLOR7_VIEW (28E10) CB_COLOR7_ATTRIB (28E18) CB_COLOR7_DCC_CONTROL (28E1C) CB_COLOR7_CMASK (28E20) CB_COLOR7_CMASK_BASE_EXT (28E24) CB_COLOR7_FMASK (28E28) CB_COLOR7_FMASK_BASE_EXT (28E2C) CB_COLOR7_CLEAR_WORD0 (28E30) CB_COLOR7_CLEAR_WORD1 (28E34) CB_COLOR7_DCC_BASE (28E38) CB_COLOR7_DCC_BASE_EXT (28E3C) | all 00000000 | CB_BLEND1_CONTROL=00000000 CB_BLEND2_CONTROL=00000000 CB_BLEND3_CONTROL=00000000 CB_BLEND4_CONTROL=00000000 CB_BLEND5_CONTROL=00000000 CB_BLEND6_CONTROL=00000000 CB_BLEND7_CONTROL=00000000 | Unbound: CB_COLOR1..7_INFO FORMAT = COLOR_INVALID (owned) and CB_TARGET_MASK / CB_SHADER_MASK enable MRT0 only (written). |
| PM4 COPY_STATE ids | CS_COPY_STATE (287CC) GFX_COPY_STATE (287D0) | not in clear state | — | Source ids for the COPY_STATE packet, not rasterizer state. |
| points | PA_CL_POINT_X_RAD (287D4) PA_CL_POINT_Y_RAD (287D8) PA_CL_POINT_SIZE (287DC) PA_CL_POINT_CULL_RAD (287E0) PA_SU_POINT_SIZE (28A00) PA_SU_POINT_MINMAX (28A04) | all 00000000 | PA_SU_POINT_SIZE=00080008 PA_SU_POINT_MINMAX=ffff0000 | Point primitives only; VGT_PRIMITIVE_TYPE = TRIANGLELIST (owned). |
| per-draw CP-loaded | VGT_DMA_BASE_HI (287E4) VGT_DMA_BASE (287E8) VGT_DRAW_INITIATOR (287F0) VGT_IMMED_DATA (287F4) VGT_EVENT_ADDRESS_REG (287F8) VGT_DMA_SIZE (28A74) VGT_DMA_MAX_SIZE (28A78) VGT_DMA_INDEX_TYPE (28A7C) VGT_DMA_NUM_INSTANCES (28A88) VGT_EVENT_INITIATOR (28A90) VGT_DISPATCH_DRAW_INDEX (28B74) VGT_DMA_EVENT_INITIATOR (28B9C) | all 00000000 (1 of 12 in it) | — | Loaded by the CP from each draw / event packet (DRAW_INDEX_AUTO, EVENT_WRITE), not persistent state. |
| lines / stipple | PA_SU_LINE_STIPPLE_CNTL (28824) PA_SU_LINE_STIPPLE_SCALE (28828) PA_SU_LINE_CNTL (28A08) PA_SC_LINE_STIPPLE (28A0C) PA_SC_LINE_CNTL (28BDC) | 00000000, 00001000 | PA_SU_LINE_STIPPLE_SCALE=3f800000 PA_SU_LINE_CNTL=00000008 PA_SC_LINE_STIPPLE=00ff0000 PA_SC_LINE_CNTL=00000000 | Line primitives only; VGT_PRIMITIVE_TYPE = TRIANGLELIST. |
| object / primitive id | PA_CL_OBJPRIM_ID_CNTL (28834) | all 00000000 | — | Primitive-id generation for PS inputs; no FS reads it and VGT_PRIMITIVEID_EN = 0 (written). |
| NGG primitive shaders | PA_CL_NGG_CNTL (28838) PA_SC_NGG_MODE_CNTL (28C50) | all 00000000 | — | NGG path off: VGT_SHADER_STAGES_EN PRIMGEN_EN = 0 (written). |
| tessellation | VGT_HOS_MAX_TESS_LEVEL (28A18) VGT_HOS_MIN_TESS_LEVEL (28A1C) VGT_HOS_REUSE_DEPTH (28A20) VGT_TESS_DISTRIBUTION (28B50) VGT_LS_HS_CONFIG (28B58) VGT_TF_PARAM (28B6C) | all 00000000 | VGT_HOS_MAX_TESS_LEVEL=42800000 VGT_TESS_DISTRIBUTION=d8181e0c | LS/HS stages off (VGT_SHADER_STAGES_EN) and legacy tessellation off (VGT_HOS_CNTL TESS_MODE owned 0). |
| vertex grouping path | VGT_GROUP_PRIM_TYPE (28A24) VGT_GROUP_FIRST_DECR (28A28) VGT_GROUP_DECR (28A2C) VGT_GROUP_VECT_0_CNTL (28A30) VGT_GROUP_VECT_1_CNTL (28A34) VGT_GROUP_VECT_0_FMT_CNTL (28A38) VGT_GROUP_VECT_1_FMT_CNTL (28A3C) | all 00000000 | — | VGT_OUTPUT_PATH_CNTL PATH_SELECT owned 0 (vertex-reuse path). |
| geometry shader | VGT_GS_ONCHIP_CNTL (28A44) VGT_GS_PER_ES (28A54) VGT_ES_PER_GS (28A58) VGT_GS_PER_VS (28A5C) VGT_GSVS_RING_OFFSET_1 (28A60) VGT_GSVS_RING_OFFSET_2 (28A64) VGT_GSVS_RING_OFFSET_3 (28A68) VGT_GS_MAX_PRIMS_PER_SUBGROUP (28A94) VGT_ESGS_RING_ITEMSIZE (28AAC) VGT_GSVS_RING_ITEMSIZE (28AB0) VGT_GS_MAX_VERT_OUT (28B38) VGT_GS_VERT_ITEMSIZE (28B5C) VGT_GS_VERT_ITEMSIZE_1 (28B60) VGT_GS_VERT_ITEMSIZE_2 (28B64) VGT_GS_VERT_ITEMSIZE_3 (28B68) VGT_GS_INSTANCE_CNT (28B90) | 00000000, 00000002, 00000080, 00000100 | VGT_ESGS_RING_ITEMSIZE=00000001 | GS / ES stages off: VGT_GS_MODE = 0 and VGT_SHADER_STAGES_EN (written). |
| primitive id reset | VGT_PRIMITIVEID_RESET (28A8C) | all 00000000 | — | VGT_PRIMITIVEID_EN written 0. |
| instanced attribute step | VGT_INSTANCE_STEP_RATE_0 (28AA0) VGT_INSTANCE_STEP_RATE_1 (28AA4) | all 00000000 | VGT_INSTANCE_STEP_RATE_0=00000001 | Per-instance vertex-fetch step rates; mabda has no vertex attributes (auto-index VS) and 1 instance. |
| stream-out buffers | VGT_STRMOUT_BUFFER_SIZE_0 (28AD0) VGT_STRMOUT_VTX_STRIDE_0 (28AD4) VGT_STRMOUT_BUFFER_OFFSET_0 (28ADC) VGT_STRMOUT_BUFFER_SIZE_1 (28AE0) VGT_STRMOUT_VTX_STRIDE_1 (28AE4) VGT_STRMOUT_BUFFER_OFFSET_1 (28AEC) VGT_STRMOUT_BUFFER_SIZE_2 (28AF0) VGT_STRMOUT_VTX_STRIDE_2 (28AF4) VGT_STRMOUT_BUFFER_OFFSET_2 (28AFC) VGT_STRMOUT_BUFFER_SIZE_3 (28B00) VGT_STRMOUT_VTX_STRIDE_3 (28B04) VGT_STRMOUT_BUFFER_OFFSET_3 (28B0C) VGT_STRMOUT_DRAW_OPAQUE_OFFSET (28B28) VGT_STRMOUT_DRAW_OPAQUE_BUFFER_FILLED_SIZE (28B2C) VGT_STRMOUT_DRAW_OPAQUE_VERTEX_STRIDE (28B30) | all 00000000 | — | VGT_STRMOUT_CONFIG / _BUFFER_CONFIG owned 0. |
| polygon offset | PA_SU_POLY_OFFSET_DB_FMT_CNTL (28B78) PA_SU_POLY_OFFSET_CLAMP (28B7C) PA_SU_POLY_OFFSET_FRONT_SCALE (28B80) PA_SU_POLY_OFFSET_FRONT_OFFSET (28B84) PA_SU_POLY_OFFSET_BACK_SCALE (28B88) PA_SU_POLY_OFFSET_BACK_OFFSET (28B8C) | all 00000000 | PA_SU_POLY_OFFSET_DB_FMT_CNTL=00000000 PA_SU_POLY_OFFSET_CLAMP=00000000 PA_SU_POLY_OFFSET_FRONT_SCALE=00000000 PA_SU_POLY_OFFSET_FRONT_OFFSET=00000000 PA_SU_POLY_OFFSET_BACK_SCALE=00000000 PA_SU_POLY_OFFSET_BACK_OFFSET=00000000 | PA_SU_SC_MODE_CNTL POLY_OFFSET_*_ENABLE = 0 (written) and no depth buffer. |
| centroid order | PA_SC_CENTROID_PRIORITY_0 (28BD4) PA_SC_CENTROID_PRIORITY_1 (28BD8) | all 00000000 | PA_SC_CENTROID_PRIORITY_0=00000000 PA_SC_CENTROID_PRIORITY_1=00000000 | MSAA centroid only; 1x and no centroid inputs. |
| 1x sample locations | PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y0_0 (28BF8) PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y0_1 (28BFC) PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y0_2 (28C00) PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y0_3 (28C04) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y0_0 (28C08) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y0_1 (28C0C) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y0_2 (28C10) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y0_3 (28C14) PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y1_0 (28C18) PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y1_1 (28C1C) PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y1_2 (28C20) PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y1_3 (28C24) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y1_0 (28C28) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y1_1 (28C2C) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y1_2 (28C30) PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y1_3 (28C34) | all 00000000 | PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y0_0=00000000 PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y0_0=00000000 PA_SC_AA_SAMPLE_LOCS_PIXEL_X0Y1_0=00000000 PA_SC_AA_SAMPLE_LOCS_PIXEL_X1Y1_0=00000000 | PA_SC_AA_CONFIG = 0 (1x). Round 1 HW negative control N5: S0 at (-8,-8)/16 changed no gate pixel. |
| fast-clear colour | CB_COLOR0_CLEAR_WORD0 (28C8C) CB_COLOR0_CLEAR_WORD1 (28C90) | all 00000000 | — | CB_COLOR0_INFO FAST_CLEAR = 0 (written). |

### C. SH registers not written (235)

| family | registers | clear-state values | radv writes | verdict |
|---|---|---|---|---|
| PS / VS user data | SPI_SHADER_USER_DATA_PS_2 (0B038) SPI_SHADER_USER_DATA_PS_3 (0B03C) SPI_SHADER_USER_DATA_PS_4 (0B040) SPI_SHADER_USER_DATA_PS_5 (0B044) SPI_SHADER_USER_DATA_PS_6 (0B048) SPI_SHADER_USER_DATA_PS_7 (0B04C) SPI_SHADER_USER_DATA_PS_8 (0B050) SPI_SHADER_USER_DATA_PS_9 (0B054) SPI_SHADER_USER_DATA_PS_10 (0B058) SPI_SHADER_USER_DATA_PS_11 (0B05C) SPI_SHADER_USER_DATA_PS_12 (0B060) SPI_SHADER_USER_DATA_PS_13 (0B064) SPI_SHADER_USER_DATA_PS_14 (0B068) SPI_SHADER_USER_DATA_PS_15 (0B06C) SPI_SHADER_USER_DATA_PS_16 (0B070) SPI_SHADER_USER_DATA_PS_17 (0B074) SPI_SHADER_USER_DATA_PS_18 (0B078) SPI_SHADER_USER_DATA_PS_19 (0B07C) SPI_SHADER_USER_DATA_PS_20 (0B080) SPI_SHADER_USER_DATA_PS_21 (0B084) SPI_SHADER_USER_DATA_PS_22 (0B088) SPI_SHADER_USER_DATA_PS_23 (0B08C) SPI_SHADER_USER_DATA_PS_24 (0B090) SPI_SHADER_USER_DATA_PS_25 (0B094) SPI_SHADER_USER_DATA_PS_26 (0B098) SPI_SHADER_USER_DATA_PS_27 (0B09C) SPI_SHADER_USER_DATA_PS_28 (0B0A0) SPI_SHADER_USER_DATA_PS_29 (0B0A4) SPI_SHADER_USER_DATA_PS_30 (0B0A8) SPI_SHADER_USER_DATA_PS_31 (0B0AC) SPI_SHADER_USER_DATA_VS_0 (0B130) SPI_SHADER_USER_DATA_VS_1 (0B134) SPI_SHADER_USER_DATA_VS_2 (0B138) SPI_SHADER_USER_DATA_VS_3 (0B13C) SPI_SHADER_USER_DATA_VS_4 (0B140) SPI_SHADER_USER_DATA_VS_5 (0B144) SPI_SHADER_USER_DATA_VS_6 (0B148) SPI_SHADER_USER_DATA_VS_7 (0B14C) SPI_SHADER_USER_DATA_VS_8 (0B150) SPI_SHADER_USER_DATA_VS_9 (0B154) SPI_SHADER_USER_DATA_VS_10 (0B158) SPI_SHADER_USER_DATA_VS_11 (0B15C) SPI_SHADER_USER_DATA_VS_12 (0B160) SPI_SHADER_USER_DATA_VS_13 (0B164) SPI_SHADER_USER_DATA_VS_14 (0B168) SPI_SHADER_USER_DATA_VS_15 (0B16C) SPI_SHADER_USER_DATA_VS_16 (0B170) SPI_SHADER_USER_DATA_VS_17 (0B174) SPI_SHADER_USER_DATA_VS_18 (0B178) SPI_SHADER_USER_DATA_VS_19 (0B17C) SPI_SHADER_USER_DATA_VS_20 (0B180) SPI_SHADER_USER_DATA_VS_21 (0B184) SPI_SHADER_USER_DATA_VS_22 (0B188) SPI_SHADER_USER_DATA_VS_23 (0B18C) SPI_SHADER_USER_DATA_VS_24 (0B190) SPI_SHADER_USER_DATA_VS_25 (0B194) SPI_SHADER_USER_DATA_VS_26 (0B198) SPI_SHADER_USER_DATA_VS_27 (0B19C) SPI_SHADER_USER_DATA_VS_28 (0B1A0) SPI_SHADER_USER_DATA_VS_29 (0B1A4) SPI_SHADER_USER_DATA_VS_30 (0B1A8) SPI_SHADER_USER_DATA_VS_31 (0B1AC) | not in clear state | SPI_SHADER_USER_DATA_PS_2=3ea8f5c3 SPI_SHADER_USER_DATA_PS_3=3ea8f5c3 SPI_SHADER_USER_DATA_PS_4=3ea8f5c3 SPI_SHADER_USER_DATA_PS_5=3f800000 SPI_SHADER_USER_DATA_VS_2=00000000 SPI_SHADER_USER_DATA_VS_3=00000000 | Preloaded into SGPRs only up to RSRC2 USER_SGPR (PS 2, VS 3); the fullscreen VS and solid-red FS read no SGPR, and the textured / array FS read s0/s1 = USER_DATA_PS_0/1, which the textured override writes. |
| ES / GS / HS / LS stages | SPI_SHADER_PGM_RSRC2_GS_VS (0B1F0) SPI_SHADER_PGM_RSRC4_GS (0B204) SPI_SHADER_USER_DATA_ADDR_LO_GS (0B208) SPI_SHADER_USER_DATA_ADDR_HI_GS (0B20C) SPI_SHADER_PGM_LO_ES (0B210) SPI_SHADER_PGM_HI_ES (0B214) SPI_SHADER_PGM_RSRC3_GS (0B21C) SPI_SHADER_PGM_LO_GS (0B220) SPI_SHADER_PGM_HI_GS (0B224) SPI_SHADER_PGM_RSRC1_GS (0B228) SPI_SHADER_PGM_RSRC2_GS (0B22C) SPI_SHADER_USER_DATA_ES_0 (0B330) SPI_SHADER_USER_DATA_ES_1 (0B334) SPI_SHADER_USER_DATA_ES_2 (0B338) SPI_SHADER_USER_DATA_ES_3 (0B33C) SPI_SHADER_USER_DATA_ES_4 (0B340) SPI_SHADER_USER_DATA_ES_5 (0B344) SPI_SHADER_USER_DATA_ES_6 (0B348) SPI_SHADER_USER_DATA_ES_7 (0B34C) SPI_SHADER_USER_DATA_ES_8 (0B350) SPI_SHADER_USER_DATA_ES_9 (0B354) SPI_SHADER_USER_DATA_ES_10 (0B358) SPI_SHADER_USER_DATA_ES_11 (0B35C) SPI_SHADER_USER_DATA_ES_12 (0B360) SPI_SHADER_USER_DATA_ES_13 (0B364) SPI_SHADER_USER_DATA_ES_14 (0B368) SPI_SHADER_USER_DATA_ES_15 (0B36C) SPI_SHADER_USER_DATA_ES_16 (0B370) SPI_SHADER_USER_DATA_ES_17 (0B374) SPI_SHADER_USER_DATA_ES_18 (0B378) SPI_SHADER_USER_DATA_ES_19 (0B37C) SPI_SHADER_USER_DATA_ES_20 (0B380) SPI_SHADER_USER_DATA_ES_21 (0B384) SPI_SHADER_USER_DATA_ES_22 (0B388) SPI_SHADER_USER_DATA_ES_23 (0B38C) SPI_SHADER_USER_DATA_ES_24 (0B390) SPI_SHADER_USER_DATA_ES_25 (0B394) SPI_SHADER_USER_DATA_ES_26 (0B398) SPI_SHADER_USER_DATA_ES_27 (0B39C) SPI_SHADER_USER_DATA_ES_28 (0B3A0) SPI_SHADER_USER_DATA_ES_29 (0B3A4) SPI_SHADER_USER_DATA_ES_30 (0B3A8) SPI_SHADER_USER_DATA_ES_31 (0B3AC) SPI_SHADER_PGM_RSRC4_HS (0B404) SPI_SHADER_USER_DATA_ADDR_LO_HS (0B408) SPI_SHADER_USER_DATA_ADDR_HI_HS (0B40C) SPI_SHADER_PGM_LO_LS (0B410) SPI_SHADER_PGM_HI_LS (0B414) SPI_SHADER_PGM_RSRC3_HS (0B41C) SPI_SHADER_PGM_LO_HS (0B420) SPI_SHADER_PGM_HI_HS (0B424) SPI_SHADER_PGM_RSRC1_HS (0B428) SPI_SHADER_PGM_RSRC2_HS (0B42C) SPI_SHADER_USER_DATA_LS_0 (0B430) SPI_SHADER_USER_DATA_LS_1 (0B434) SPI_SHADER_USER_DATA_LS_2 (0B438) SPI_SHADER_USER_DATA_LS_3 (0B43C) SPI_SHADER_USER_DATA_LS_4 (0B440) SPI_SHADER_USER_DATA_LS_5 (0B444) SPI_SHADER_USER_DATA_LS_6 (0B448) SPI_SHADER_USER_DATA_LS_7 (0B44C) SPI_SHADER_USER_DATA_LS_8 (0B450) SPI_SHADER_USER_DATA_LS_9 (0B454) SPI_SHADER_USER_DATA_LS_10 (0B458) SPI_SHADER_USER_DATA_LS_11 (0B45C) SPI_SHADER_USER_DATA_LS_12 (0B460) SPI_SHADER_USER_DATA_LS_13 (0B464) SPI_SHADER_USER_DATA_LS_14 (0B468) SPI_SHADER_USER_DATA_LS_15 (0B46C) SPI_SHADER_USER_DATA_LS_16 (0B470) SPI_SHADER_USER_DATA_LS_17 (0B474) SPI_SHADER_USER_DATA_LS_18 (0B478) SPI_SHADER_USER_DATA_LS_19 (0B47C) SPI_SHADER_USER_DATA_LS_20 (0B480) SPI_SHADER_USER_DATA_LS_21 (0B484) SPI_SHADER_USER_DATA_LS_22 (0B488) SPI_SHADER_USER_DATA_LS_23 (0B48C) SPI_SHADER_USER_DATA_LS_24 (0B490) SPI_SHADER_USER_DATA_LS_25 (0B494) SPI_SHADER_USER_DATA_LS_26 (0B498) SPI_SHADER_USER_DATA_LS_27 (0B49C) SPI_SHADER_USER_DATA_LS_28 (0B4A0) SPI_SHADER_USER_DATA_LS_29 (0B4A4) SPI_SHADER_USER_DATA_LS_30 (0B4A8) SPI_SHADER_USER_DATA_LS_31 (0B4AC) | not in clear state | SPI_SHADER_PGM_HI_ES=00000080 SPI_SHADER_PGM_HI_LS=00000080 SPI_SHADER_PGM_RSRC3_HS=ffff003f | Stages disabled by VGT_SHADER_STAGES_EN (written). |
| common user data | SPI_SHADER_USER_DATA_COMMON_0 (0B530) SPI_SHADER_USER_DATA_COMMON_1 (0B534) SPI_SHADER_USER_DATA_COMMON_2 (0B538) SPI_SHADER_USER_DATA_COMMON_3 (0B53C) SPI_SHADER_USER_DATA_COMMON_4 (0B540) SPI_SHADER_USER_DATA_COMMON_5 (0B544) SPI_SHADER_USER_DATA_COMMON_6 (0B548) SPI_SHADER_USER_DATA_COMMON_7 (0B54C) SPI_SHADER_USER_DATA_COMMON_8 (0B550) SPI_SHADER_USER_DATA_COMMON_9 (0B554) SPI_SHADER_USER_DATA_COMMON_10 (0B558) SPI_SHADER_USER_DATA_COMMON_11 (0B55C) SPI_SHADER_USER_DATA_COMMON_12 (0B560) SPI_SHADER_USER_DATA_COMMON_13 (0B564) SPI_SHADER_USER_DATA_COMMON_14 (0B568) SPI_SHADER_USER_DATA_COMMON_15 (0B56C) SPI_SHADER_USER_DATA_COMMON_16 (0B570) SPI_SHADER_USER_DATA_COMMON_17 (0B574) SPI_SHADER_USER_DATA_COMMON_18 (0B578) SPI_SHADER_USER_DATA_COMMON_19 (0B57C) SPI_SHADER_USER_DATA_COMMON_20 (0B580) SPI_SHADER_USER_DATA_COMMON_21 (0B584) SPI_SHADER_USER_DATA_COMMON_22 (0B588) SPI_SHADER_USER_DATA_COMMON_23 (0B58C) SPI_SHADER_USER_DATA_COMMON_24 (0B590) SPI_SHADER_USER_DATA_COMMON_25 (0B594) SPI_SHADER_USER_DATA_COMMON_26 (0B598) SPI_SHADER_USER_DATA_COMMON_27 (0B59C) SPI_SHADER_USER_DATA_COMMON_28 (0B5A0) SPI_SHADER_USER_DATA_COMMON_29 (0B5A4) SPI_SHADER_USER_DATA_COMMON_30 (0B5A8) SPI_SHADER_USER_DATA_COMMON_31 (0B5AC) | not in clear state | — | Broadcast user-data slots; not loaded for the VS / PS RSRC2 USER_SGPR declared by mabda. |
| compute shader | COMPUTE_DISPATCH_INITIATOR (0B800) COMPUTE_DIM_X (0B804) COMPUTE_DIM_Y (0B808) COMPUTE_DIM_Z (0B80C) COMPUTE_START_X (0B810) COMPUTE_START_Y (0B814) COMPUTE_START_Z (0B818) COMPUTE_NUM_THREAD_X (0B81C) COMPUTE_NUM_THREAD_Y (0B820) COMPUTE_NUM_THREAD_Z (0B824) COMPUTE_PIPELINESTAT_ENABLE (0B828) COMPUTE_PERFCOUNT_ENABLE (0B82C) COMPUTE_PGM_LO (0B830) COMPUTE_PGM_HI (0B834) COMPUTE_DISPATCH_PKT_ADDR_LO (0B838) COMPUTE_DISPATCH_PKT_ADDR_HI (0B83C) COMPUTE_DISPATCH_SCRATCH_BASE_LO (0B840) COMPUTE_DISPATCH_SCRATCH_BASE_HI (0B844) COMPUTE_PGM_RSRC1 (0B848) COMPUTE_PGM_RSRC2 (0B84C) COMPUTE_VMID (0B850) COMPUTE_RESOURCE_LIMITS (0B854) COMPUTE_STATIC_THREAD_MGMT_SE0 (0B858) COMPUTE_STATIC_THREAD_MGMT_SE1 (0B85C) COMPUTE_TMPRING_SIZE (0B860) COMPUTE_STATIC_THREAD_MGMT_SE2 (0B864) COMPUTE_STATIC_THREAD_MGMT_SE3 (0B868) COMPUTE_RESTART_X (0B86C) COMPUTE_RESTART_Y (0B870) COMPUTE_RESTART_Z (0B874) COMPUTE_THREAD_TRACE_ENABLE (0B878) COMPUTE_MISC_RESERVED (0B87C) COMPUTE_DISPATCH_ID (0B880) COMPUTE_THREADGROUP_ID (0B884) COMPUTE_RELAUNCH (0B888) COMPUTE_WAVE_RESTORE_ADDR_LO (0B88C) COMPUTE_WAVE_RESTORE_ADDR_HI (0B890) COMPUTE_SHADER_CHKSUM (0B894) COMPUTE_USER_DATA_0 (0B900) COMPUTE_USER_DATA_1 (0B904) COMPUTE_USER_DATA_2 (0B908) COMPUTE_USER_DATA_3 (0B90C) COMPUTE_USER_DATA_4 (0B910) COMPUTE_USER_DATA_5 (0B914) COMPUTE_USER_DATA_6 (0B918) COMPUTE_USER_DATA_7 (0B91C) COMPUTE_USER_DATA_8 (0B920) COMPUTE_USER_DATA_9 (0B924) COMPUTE_USER_DATA_10 (0B928) COMPUTE_USER_DATA_11 (0B92C) COMPUTE_USER_DATA_12 (0B930) COMPUTE_USER_DATA_13 (0B934) COMPUTE_USER_DATA_14 (0B938) COMPUTE_USER_DATA_15 (0B93C) COMPUTE_DISPATCH_END (0B9F8) COMPUTE_NOWHERE (0B9FC) | not in clear state | COMPUTE_START_X=00000000 COMPUTE_START_Y=00000000 COMPUTE_START_Z=00000000 COMPUTE_PGM_HI=00000080 COMPUTE_STATIC_THREAD_MGMT_SE0=ffffffff COMPUTE_STATIC_THREAD_MGMT_SE1=00000000 COMPUTE_STATIC_THREAD_MGMT_SE2=00000000 COMPUTE_STATIC_THREAD_MGMT_SE3=00000000 | Compute dispatch state; mabda's compute composers write their own. |

### D. UConfig registers not written (255)

| family | registers | clear-state values | radv writes | verdict |
|---|---|---|---|---|
| CP / RLC / debug | CP_EOP_DONE_ADDR_LO (30000) CP_EOP_DONE_ADDR_HI (30004) CP_EOP_DONE_DATA_LO (30008) CP_EOP_DONE_DATA_HI (3000C) CP_EOP_LAST_FENCE_LO (30010) CP_EOP_LAST_FENCE_HI (30014) CP_STREAM_OUT_ADDR_LO (30018) CP_STREAM_OUT_ADDR_HI (3001C) CP_NUM_PRIM_WRITTEN_COUNT0_LO (30020) CP_NUM_PRIM_WRITTEN_COUNT0_HI (30024) CP_NUM_PRIM_NEEDED_COUNT0_LO (30028) CP_NUM_PRIM_NEEDED_COUNT0_HI (3002C) CP_NUM_PRIM_WRITTEN_COUNT1_LO (30030) CP_NUM_PRIM_WRITTEN_COUNT1_HI (30034) CP_NUM_PRIM_NEEDED_COUNT1_LO (30038) CP_NUM_PRIM_NEEDED_COUNT1_HI (3003C) CP_NUM_PRIM_WRITTEN_COUNT2_LO (30040) CP_NUM_PRIM_WRITTEN_COUNT2_HI (30044) CP_NUM_PRIM_NEEDED_COUNT2_LO (30048) CP_NUM_PRIM_NEEDED_COUNT2_HI (3004C) CP_NUM_PRIM_WRITTEN_COUNT3_LO (30050) CP_NUM_PRIM_WRITTEN_COUNT3_HI (30054) CP_NUM_PRIM_NEEDED_COUNT3_LO (30058) CP_NUM_PRIM_NEEDED_COUNT3_HI (3005C) CP_PIPE_STATS_ADDR_LO (30060) CP_PIPE_STATS_ADDR_HI (30064) CP_VGT_IAVERT_COUNT_LO (30068) CP_VGT_IAVERT_COUNT_HI (3006C) CP_VGT_IAPRIM_COUNT_LO (30070) CP_VGT_IAPRIM_COUNT_HI (30074) CP_VGT_GSPRIM_COUNT_LO (30078) CP_VGT_GSPRIM_COUNT_HI (3007C) CP_VGT_VSINVOC_COUNT_LO (30080) CP_VGT_VSINVOC_COUNT_HI (30084) CP_VGT_GSINVOC_COUNT_LO (30088) CP_VGT_GSINVOC_COUNT_HI (3008C) CP_VGT_HSINVOC_COUNT_LO (30090) CP_VGT_HSINVOC_COUNT_HI (30094) CP_VGT_DSINVOC_COUNT_LO (30098) CP_VGT_DSINVOC_COUNT_HI (3009C) CP_PA_CINVOC_COUNT_LO (300A0) CP_PA_CINVOC_COUNT_HI (300A4) CP_PA_CPRIM_COUNT_LO (300A8) CP_PA_CPRIM_COUNT_HI (300AC) CP_SC_PSINVOC_COUNT0_LO (300B0) CP_SC_PSINVOC_COUNT0_HI (300B4) CP_SC_PSINVOC_COUNT1_LO (300B8) CP_SC_PSINVOC_COUNT1_HI (300BC) CP_VGT_CSINVOC_COUNT_LO (300C0) CP_VGT_CSINVOC_COUNT_HI (300C4) CP_PIPE_STATS_CONTROL (300F4) CP_STREAM_OUT_CONTROL (300F8) CP_STRMOUT_CNTL (300FC) SCRATCH_REG0 (30100) SCRATCH_REG1 (30104) SCRATCH_REG2 (30108) SCRATCH_REG3 (3010C) SCRATCH_REG4 (30110) SCRATCH_REG5 (30114) SCRATCH_REG6 (30118) SCRATCH_REG7 (3011C) CP_APPEND_DATA_HI (30130) CP_APPEND_LAST_CS_FENCE_HI (30134) CP_APPEND_LAST_PS_FENCE_HI (30138) SCRATCH_UMSK (30140) SCRATCH_ADDR (30144) CP_PFP_ATOMIC_PREOP_LO (30148) CP_PFP_ATOMIC_PREOP_HI (3014C) CP_PFP_GDS_ATOMIC0_PREOP_LO (30150) CP_PFP_GDS_ATOMIC0_PREOP_HI (30154) CP_PFP_GDS_ATOMIC1_PREOP_LO (30158) CP_PFP_GDS_ATOMIC1_PREOP_HI (3015C) CP_APPEND_ADDR_LO (30160) CP_APPEND_ADDR_HI (30164) CP_APPEND_DATA_LO (30168) CP_APPEND_LAST_CS_FENCE_LO (3016C) CP_APPEND_LAST_PS_FENCE_LO (30170) CP_ATOMIC_PREOP_LO (30174) CP_ATOMIC_PREOP_HI (30178) CP_GDS_ATOMIC0_PREOP_LO (3017C) CP_GDS_ATOMIC0_PREOP_HI (30180) CP_GDS_ATOMIC1_PREOP_LO (30184) CP_GDS_ATOMIC1_PREOP_HI (30188) CP_ME_MC_WADDR_LO (301A4) CP_ME_MC_WADDR_HI (301A8) CP_ME_MC_WDATA_LO (301AC) CP_ME_MC_WDATA_HI (301B0) CP_ME_MC_RADDR_LO (301B4) CP_ME_MC_RADDR_HI (301B8) CP_SEM_WAIT_TIMER (301BC) CP_SIG_SEM_ADDR_LO (301C0) CP_SIG_SEM_ADDR_HI (301C4) CP_WAIT_REG_MEM_TIMEOUT (301D0) CP_WAIT_SEM_ADDR_LO (301D4) CP_WAIT_SEM_ADDR_HI (301D8) CP_DMA_PFP_CONTROL (301DC) CP_DMA_ME_CONTROL (301E0) CP_COHER_BASE_HI (301E4) CP_COHER_CNTL (301F0) CP_COHER_SIZE (301F4) CP_COHER_BASE (301F8) CP_COHER_STATUS (301FC) CP_DMA_ME_SRC_ADDR (30200) CP_DMA_ME_SRC_ADDR_HI (30204) CP_DMA_ME_DST_ADDR (30208) CP_DMA_ME_DST_ADDR_HI (3020C) CP_DMA_ME_COMMAND (30210) CP_DMA_PFP_SRC_ADDR (30214) CP_DMA_PFP_SRC_ADDR_HI (30218) CP_DMA_PFP_DST_ADDR (3021C) CP_DMA_PFP_DST_ADDR_HI (30220) CP_DMA_PFP_COMMAND (30224) CP_DMA_CNTL (30228) CP_DMA_READ_TAGS (3022C) CP_COHER_SIZE_HI (30230) CP_PFP_IB_CONTROL (30234) CP_PFP_LOAD_CONTROL (30238) CP_SCRATCH_INDEX (3023C) CP_SCRATCH_DATA (30240) CP_RB_OFFSET (30244) CP_IB1_OFFSET (30248) CP_IB2_OFFSET (3024C) CP_IB1_PREAMBLE_BEGIN (30250) CP_IB1_PREAMBLE_END (30254) CP_IB2_PREAMBLE_BEGIN (30258) CP_IB2_PREAMBLE_END (3025C) CP_CE_IB1_OFFSET (30260) CP_CE_IB2_OFFSET (30264) CP_CE_COUNTER (30268) CP_CE_RB_OFFSET (3026C) CP_CE_INIT_CMD_BUFSZ (302F4) CP_CE_IB1_CMD_BUFSZ (302F8) CP_CE_IB2_CMD_BUFSZ (302FC) CP_IB1_CMD_BUFSZ (30300) CP_IB2_CMD_BUFSZ (30304) CP_ST_CMD_BUFSZ (30308) CP_CE_INIT_BASE_LO (3030C) CP_CE_INIT_BASE_HI (30310) CP_CE_INIT_BUFSZ (30314) CP_CE_IB1_BASE_LO (30318) CP_CE_IB1_BASE_HI (3031C) CP_CE_IB1_BUFSZ (30320) CP_CE_IB2_BASE_LO (30324) CP_CE_IB2_BASE_HI (30328) CP_CE_IB2_BUFSZ (3032C) CP_IB1_BASE_LO (30330) CP_IB1_BASE_HI (30334) CP_IB1_BUFSZ (30338) CP_IB2_BASE_LO (3033C) CP_IB2_BASE_HI (30340) CP_IB2_BUFSZ (30344) CP_ST_BASE_LO (30348) CP_ST_BASE_HI (3034C) CP_ST_BUFSZ (30350) CP_EOP_DONE_EVENT_CNTL (30354) CP_EOP_DONE_DATA_CNTL (30358) CP_EOP_DONE_CNTX_ID (3035C) CP_PFP_COMPLETION_STATUS (303B0) CP_CE_COMPLETION_STATUS (303B4) CP_PRED_NOT_VISIBLE (303B8) CP_PFP_METADATA_BASE_ADDR (303C0) CP_PFP_METADATA_BASE_ADDR_HI (303C4) CP_CE_METADATA_BASE_ADDR (303C8) CP_CE_METADATA_BASE_ADDR_HI (303CC) CP_DRAW_INDX_INDR_ADDR (303D0) CP_DRAW_INDX_INDR_ADDR_HI (303D4) CP_DISPATCH_INDR_ADDR (303D8) CP_DISPATCH_INDR_ADDR_HI (303DC) CP_INDEX_BASE_ADDR (303E0) CP_INDEX_BASE_ADDR_HI (303E4) CP_INDEX_TYPE (303E8) CP_GDS_BKUP_ADDR (303EC) CP_GDS_BKUP_ADDR_HI (303F0) CP_SAMPLE_STATUS (303F4) CP_ME_COHER_CNTL (303F8) CP_ME_COHER_SIZE (303FC) CP_ME_COHER_SIZE_HI (30400) CP_ME_COHER_BASE (30404) CP_ME_COHER_BASE_HI (30408) CP_ME_COHER_STATUS (3040C) RLC_GPM_PERF_COUNT_0 (30500) RLC_GPM_PERF_COUNT_1 (30504) GRBM_GFX_INDEX (30800) SQ_THREAD_TRACE_BASE (30CC0) SQ_THREAD_TRACE_SIZE (30CC4) SQ_THREAD_TRACE_MASK (30CC8) SQ_THREAD_TRACE_TOKEN_MASK (30CCC) SQ_THREAD_TRACE_PERF_MASK (30CD0) SQ_THREAD_TRACE_CTRL (30CD4) SQ_THREAD_TRACE_MODE (30CD8) SQ_THREAD_TRACE_BASE2 (30CDC) SQ_THREAD_TRACE_TOKEN_MASK2 (30CE0) SQ_THREAD_TRACE_WPTR (30CE4) SQ_THREAD_TRACE_STATUS (30CE8) SQ_THREAD_TRACE_HIWATER (30CEC) SQ_THREAD_TRACE_CNTR (30CF0) SQ_THREAD_TRACE_USERDATA_0 (30D00) SQ_THREAD_TRACE_USERDATA_1 (30D04) SQ_THREAD_TRACE_USERDATA_2 (30D08) SQ_THREAD_TRACE_USERDATA_3 (30D0C) SQC_CACHES (30D20) SQC_WRITEBACK (30D24) | not in clear state | — | CP ring, IB, fence, statistics and debug-trace state maintained by the CP and kernel (GRBM_GFX_INDEX: MMIO instance select, broadcast by the kernel; Mesa writes it only for GFX8 harvested raster configs). |
| compute preamble | CP_COHER_START_DELAY (301EC) TA_CS_BC_BASE_ADDR (30E00) TA_CS_BC_BASE_ADDR_HI (30E04) | not in clear state | CP_COHER_START_DELAY=00000000 TA_CS_BC_BASE_ADDR=00000000 TA_CS_BC_BASE_ADDR_HI=00000000 | Mesa writes these in ac_init_compute_preamble_state; mabda's compute composers write them. Not consulted by the rasterizer. |
| GS / tessellation buffers | VGT_GSVS_RING_SIZE (30904) VGT_TF_RING_SIZE (30938) VGT_HS_OFFCHIP_PARAM (3093C) VGT_TF_MEMORY_BASE (30940) VGT_TF_MEMORY_BASE_HI (30944) | not in clear state | — | GS / HS / LS stages off. |
| per-draw CP-loaded | VGT_INDEX_TYPE (3090C) VGT_NUM_INDICES (30930) VGT_NUM_INSTANCES (30934) | not in clear state | — | Loaded from each DRAW_INDEX_AUTO / NUM_INSTANCES packet (NUM_INSTANCES packet written per draw). |
| stream-out results | VGT_STRMOUT_BUFFER_FILLED_SIZE_0 (30910) VGT_STRMOUT_BUFFER_FILLED_SIZE_1 (30914) VGT_STRMOUT_BUFFER_FILLED_SIZE_2 (30918) VGT_STRMOUT_BUFFER_FILLED_SIZE_3 (3091C) | not in clear state | — | Stream-out off (owned). |
| work-distributor NGG buffers | WD_POS_BUF_BASE (30948) WD_POS_BUF_BASE_HI (3094C) WD_CNTL_SB_BUF_BASE (30950) WD_CNTL_SB_BUF_BASE_HI (30954) WD_INDEX_BUF_BASE (30958) WD_INDEX_BUF_BASE_HI (3095C) | not in clear state | — | NGG off (PRIMGEN_EN = 0). |
| line stipple state | PA_SU_LINE_STIPPLE_VALUE (30A00) PA_SC_LINE_STIPPLE_STATE (30A04) | not in clear state | PA_SU_LINE_STIPPLE_VALUE=00000000 PA_SC_LINE_STIPPLE_STATE=00000000 | Lines only; radv writes 0 every preamble. |
| screen extent | PA_SC_SCREEN_EXTENT_MIN_0 (30A10) PA_SC_SCREEN_EXTENT_MAX_0 (30A14) PA_SC_SCREEN_EXTENT_MIN_1 (30A18) PA_SC_SCREEN_EXTENT_MAX_1 (30A2C) | not in clear state | — | Only with PA_SC_SCREEN_EXTENT_CONTROL slice enables (owned 0). |
| screen trap debug | PA_SC_P3D_TRAP_SCREEN_HV_EN (30A80) PA_SC_P3D_TRAP_SCREEN_H (30A84) PA_SC_P3D_TRAP_SCREEN_V (30A88) PA_SC_P3D_TRAP_SCREEN_OCCURRENCE (30A8C) PA_SC_P3D_TRAP_SCREEN_COUNT (30A90) PA_SC_HP3D_TRAP_SCREEN_HV_EN (30AA0) PA_SC_HP3D_TRAP_SCREEN_H (30AA4) PA_SC_HP3D_TRAP_SCREEN_V (30AA8) PA_SC_HP3D_TRAP_SCREEN_OCCURRENCE (30AAC) PA_SC_HP3D_TRAP_SCREEN_COUNT (30AB0) PA_SC_TRAP_SCREEN_HV_EN (30AC0) PA_SC_TRAP_SCREEN_H (30AC4) PA_SC_TRAP_SCREEN_V (30AC8) PA_SC_TRAP_SCREEN_OCCURRENCE (30ACC) PA_SC_TRAP_SCREEN_COUNT (30AD0) | not in clear state | — | Debug trap on a screen coordinate; no Mesa driver writes it. |
| stereo offset | PA_STATE_STEREO_X (30AD4) | not in clear state | — | Only with PA_STEREO_CNTL EN_STEREO (owned 0). |
| occlusion counters | DB_OCCLUSION_COUNT0_LOW (30F00) DB_OCCLUSION_COUNT0_HI (30F04) DB_OCCLUSION_COUNT1_LOW (30F08) DB_OCCLUSION_COUNT1_HI (30F0C) DB_OCCLUSION_COUNT2_LOW (30F10) DB_OCCLUSION_COUNT2_HI (30F14) DB_OCCLUSION_COUNT3_LOW (30F18) DB_OCCLUSION_COUNT3_HI (30F1C) DB_ZPASS_COUNT_LOW (30FF8) DB_ZPASS_COUNT_HI (30FFC) | not in clear state | — | Results, written by the GPU. |

### E. Other register spaces

| space | count | registers | verdict |
|---|---|---|---|
| CONFIG 0x8000-0xAFFF | 82 | GRBM_STATUS2 GRBM_STATUS GRBM_STATUS_SE0 GRBM_STATUS_SE1 GRBM_STATUS_SE2 GRBM_STATUS_SE3 CP_CPC_STATUS CP_CPC_BUSY_STAT CP_CPC_STALLED_STAT1 CP_CPF_STATUS CP_CPF_BUSY_STAT CP_CPF_STALLED_STAT1 CP_CPC_GRBM_FREE_COUNT CP_CPC_SCRATCH_INDEX CP_CPC_SCRATCH_DATA CP_CPF_GRBM_FREE_COUNT CP_CPC_HALT_HYST_COUNT SQ_BUF_RSRC_WORD0 SQ_BUF_RSRC_WORD1 SQ_BUF_RSRC_WORD2 SQ_BUF_RSRC_WORD3 SQ_IMG_RSRC_WORD0 SQ_IMG_RSRC_WORD1 SQ_IMG_RSRC_WORD2 SQ_IMG_RSRC_WORD3 SQ_IMG_RSRC_WORD4 SQ_IMG_RSRC_WORD5 SQ_IMG_RSRC_WORD6 SQ_IMG_RSRC_WORD7 SQ_IMG_SAMP_WORD0 SQ_IMG_SAMP_WORD1 SQ_IMG_SAMP_WORD2 SQ_IMG_SAMP_WORD3 GB_ADDR_CONFIG GB_TILE_MODE0 GB_TILE_MODE1 GB_TILE_MODE2 GB_TILE_MODE3 GB_TILE_MODE4 GB_TILE_MODE5 GB_TILE_MODE6 GB_TILE_MODE7 GB_TILE_MODE8 GB_TILE_MODE9 GB_TILE_MODE10 GB_TILE_MODE11 GB_TILE_MODE12 GB_TILE_MODE13 GB_TILE_MODE14 GB_TILE_MODE15 GB_TILE_MODE16 GB_TILE_MODE17 GB_TILE_MODE18 GB_TILE_MODE19 GB_TILE_MODE20 GB_TILE_MODE21 GB_TILE_MODE22 GB_TILE_MODE23 GB_TILE_MODE24 GB_TILE_MODE25 GB_TILE_MODE26 GB_TILE_MODE27 GB_TILE_MODE28 GB_TILE_MODE29 GB_TILE_MODE30 GB_TILE_MODE31 GB_MACROTILE_MODE0 GB_MACROTILE_MODE1 GB_MACROTILE_MODE2 GB_MACROTILE_MODE3 GB_MACROTILE_MODE4 GB_MACROTILE_MODE5 GB_MACROTILE_MODE6 GB_MACROTILE_MODE7 GB_MACROTILE_MODE8 GB_MACROTILE_MODE9 GB_MACROTILE_MODE10 GB_MACROTILE_MODE11 GB_MACROTILE_MODE12 GB_MACROTILE_MODE13 GB_MACROTILE_MODE14 GB_MACROTILE_MODE15 | CONFIG space 0x008000-0x00AFFF (GRBM / CP status, SQ resource words, GB_TILE_MODE / GB_ADDR_CONFIG golden values). Programmed by the kernel or read-only status; Mesa 26.2.2 writes CONFIG registers only for GFX6 (PA_CL_ENHANCE and friends). |
| everything else | 465 | (not listed) | Outside the SET_CONTEXT_REG / SET_SH_REG / SET_UCONFIG_REG / SET_CONFIG_REG packet spaces (GRBM / SRBM / RLC / BIF / interrupt / device registers the kernel programs). Mesa 26.2.2 emits no GFX9 draw state there. |

Total: 1652 gfx9.json `mm` registers — 137 written, 968 classified by family, 82 CONFIG, 465 in other register spaces.
<!-- END GENERATED: owned_state_audit.py -->
