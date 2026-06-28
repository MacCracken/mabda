# NVIDIA N7 — NVK Triangle-Draw Capture Findings (TURING_A 0xC597)

Decoded reference for building mabda's native **render** path (N7), from a
**known-good NVK triangle draw captured on the TU116** (2026-06-28). The
capture **harness** is preserved at **`tools/nvidia-capture/`**
(`probe_render.vert` + `probe_render.frag` GLSL, `vk_render.c` headless NVK
app, run under the `nouveau_capture.c` interposer); the raw bytes do **not**
live in the repo. Re-capture with:

```
cd tools/nvidia-capture
glslc -fshader-stage=vert probe_render.vert -o probe_render.vert.spv
glslc -fshader-stage=frag probe_render.frag -o probe_render.frag.spv
gcc -shared -fPIC nouveau_capture.c -o nouveau_capture.so -ldl
gcc -O2 vk_render.c -o vk_render -lvulkan
NV_CAP_DIR=./nvcaprender LD_PRELOAD=./nouveau_capture.so ./vk_render
```

`vk_render` draws a vertex-less fullscreen triangle (`vec4(0.2,0.4,0.6,1.0)`)
into an offscreen 64×64 RGBA8 target, copies to a host buffer, and reads back
the center pixel `0xFF996633` (PASS). The captured draw EXEC (`081_…len1648`,
141 method records) is decoded in
`tools/nvidia-capture/nvcaprender/render_method_decode.txt`.

Every offset/field below was decoded against NVIDIA open-gpu-doc
`classes/3d/clc597.h` + `classes/dma-copy/clc5b5.h`, **then independently
adversarially verified** against a second source (Mesa NVK
`nvk_cmd_draw.c`/`nvk_shader.c`, the nvc0 gallium driver
`nvc0_state_validate.c`, envytools `gf100_3d.xml`, NAK `sph.rs`). The
verifier corrections are folded in below.

## The two facts that shape the whole render arc

1. **NVK draws (and clears) through MME macros** (`CALL_MME_MACRO` at
   `0x3810…0x38c0`) — on-GPU microcode uploaded once at NVK queue-init via
   `LOAD_MME_INSTRUCTION_RAM`. mabda **cannot** use them; it replaces each
   macro with its **direct clc597 method** equivalent (decoded below). The
   draw macro `CALL_MME_MACRO(11)=[0,3,1,0,0]` is `NVK_MME_DRAW`.
2. **mabda renders to a LINEAR target** (its N7.1 RT is a linear host BO),
   where NVK rendered **block-linear in VRAM + a copy-engine detile blit** to
   read back. clc597 supports a pitch/linear color target directly
   (`SET_COLOR_TARGET_MEMORY` LAYOUT=PITCH), so mabda renders into its linear
   RT and **CPU-reads it with no CE blit** — a real simplification, but a
   *different path than the capture* (the capture validates block-linear; the
   linear path is confirmed by clc597.h + nvc0 gallium + NVK, and mabda will
   HW-prove it at N7.4).

Classes created (NVIF): **`0xC597` TURING_A (3D)**, `0xC5C0` compute,
`0xC5B5` copy, `0x902D` 2D, `0xA140` I2M. Subchannels: **subc0=3D**,
subc1=compute, subc4=copy. Completion is the **nouveau EXEC out-syncobj**
(no `SET_REPORT_SEMAPHORE`) — mabda's existing N4 EXEC+absolute-deadline
syncobj wait applies directly.

## Group A — color target (verified: mostly-solid)

`SET_COLOR_TARGET_A(0)` is a 9-dword block at `0x0800` (stride `0x40` per
target). Capture (block-linear): `[0x3f, 0xfdf96000, 0x40, 0x40, 0xd5, 0x30,
0x01, 0x1000, 0x00]`.

- `0x0800` ADDRESS_UPPER = VA[39:32], `0x0804` ADDRESS_LOWER = VA[31:0]
- `0x0808` WIDTH, `0x080c` HEIGHT, `0x0810` FORMAT (`0xD5` = A8B8G8R8 =
  RGBA8, byte0=R…byte3=A; CPU reads R,G,B,A directly)
- `0x0814` **MEMORY**: bit12 LAYOUT (0=BLOCKLINEAR, **1=PITCH**); block-dim
  nibbles. Capture `0x30` = BLOCKLINEAR, BLOCK_HEIGHT=EIGHT_GOBS.
- `0x0818` THIRD_DIMENSION, `0x081c` ARRAY_PITCH, `0x0820` LAYER

**mabda LINEAR target** (the key result): `MEMORY = 0x1000` (LAYOUT_PITCH),
and **WIDTH then holds the ROW PITCH IN BYTES** (256 for 64px×4B), HEIGHT
stays rows, THIRD_DIMENSION=1, ARRAY_PITCH=0, LAYER=0. **Both the base VA and
the pitch must be 128-byte aligned** (NVK `assert(addr%128==0)` /
`assert(pitch%128==0)` — verifier corrected the decoder's looser "64 or 128").
mabda's page-aligned BO + 256 pitch satisfy this; for width W use
`pitch = align(W*4, 128)`. Linear color targets are **2D-only, single-layer,
non-MSAA, uncompressed**. Differs from the capture in exactly 3 dwords
(WIDTH px→byte-pitch, MEMORY `0x30`→`0x1000`, ARRAY_PITCH `0x1000`→0).

- `0x121c` SET_CT_SELECT = **`0x1`** (TARGET_COUNT=1, TARGET0=0); capture's
  `0x0fac6881` is the same count=1 with an identity slot map.
- `0x0ff4`/`0x0ff8` SET_SURFACE_CLIP_H/V = `0x00400000` each (origin 0,
  extent 64).
- `0x19e0` SET_COLOR_COMPRESSION(0) = 0 (required for linear).
- `0x1538` SET_ZT_SELECT = 0 (no depth target). **Not** because "pitch forbids
  Z" (verifier corrected that overstatement — nouveau just skips ZETA when the
  color is linear; NVK allows Z with linear color) but because a solid-color
  triangle needs no depth buffer.

## Group B — pipeline + shader binding + the SPH (verified: solid)

`SET_PIPELINE_*` block: base `0x2000`, **per-stage stride `0x40`**,
`idx==type` (VS→slot1, FS→slot5):

- `SET_PIPELINE_SHADER(j)` = `0x2000+j*64`, imm = **ENABLE(bit0) | (TYPE<<4)**.
  TYPE enum: 0=VCBF, 1=VERTEX, 2=TESS_INIT, 3=TESS, 4=GEOMETRY, 5=PIXEL. So
  VS=`0x11` (enable|VERTEX), FS=`0x51` (enable|PIXEL); the disabled in-between
  stages are `0x20/0x30/0x40` (enable=0, type 2/3/4).
- `SET_PIPELINE_PROGRAM_ADDRESS_A/B(j)` = `0x2014+j*64` / `0x2018+j*64`
  (UPPER[7:0] = VA>>32, LOWER = VA). **Absolute 40-bit GPU VA** — Volta+ has
  **no** SET_PROGRAM_REGION, and Turing must **not** emit
  SET_PIPELINE_PROGRAM_PREFETCH (Ampere-B/clc797 only).
- `SET_PIPELINE_REGISTER_COUNT(j)` = `0x200c+j*64` (=`num_gprs`, 24 here),
  `SET_PIPELINE_BINDING(j)` = `0x2010+j*64` (=stage group: VS 0, FS 4).

**The Shader Program Header (SPH).** Graphics VS/FS programs need an SPH
preamble that compute kernels do not. On Turing (SM75) the SPH is **v4 =
128 bytes** (`TU102_SHADER_HEADER_SIZE`; the older GF100/v3 header is 80 B).
**`SET_PIPELINE_PROGRAM_ADDRESS` points AT the SPH**; the SASS begins
immediately after, at **BO+0x80** (0x80-aligned). Program BO layout per stage:
`[0x00..0x80) = 128-B SPH` then `[0x80..) = SM75 SASS`.

**N7.3 guidance:** mabda must **embed a precompiled SPH+SASS blob** (the
byte-polymorphic pre-compiled-ISA shader module, like the compute SASS), NOT
hand-author the SPH's trailing imap/omap vectors — only the 80-byte classic
header is public; the v4 extension (omap_targets ~576:608, omap_g 432:560,
imap) is NVK/NAK-only and must come from a real NAK/SM75 compile + HW
validation. CommonWord0 (public): SphType[0:4] (VTG=1/PS=2), Version[5:9]=4,
ShaderType[10:13] (VERTEX=1/PIXEL=5), MrtEnable[14] (PS sets 1),
KillsPixels[15], DoesGlobalStore[16], SassVersion[17:20]=1. The FS omap_targets
must mark RT0 RGBA (`0xf`). Device-init dependency: `SET_SPA_VERSION` (`0x0310`)
must be configured at channel init to match the SM75 ISA.

Minimal bind (6 methods on a fresh TURING_A object; +4 stage-disables for a
reused channel): `0x2040=0x11`, `0x2054 INCRx2 [VS_hi, VS_lo]`,
`0x204c INCRx2 [24, 0]`, `0x2140=0x51`, `0x2154 INCRx2 [FS_hi, FS_lo]`,
`0x214c INCRx2 [24, 4]`. NVK binds shaders with pure direct clc597 methods
(no MME macro).

## Group C — the draw (verified: solid)

The direct (non-MME) Turing draw that replaces `NVK_MME_DRAW`
(`CALL_MME_MACRO(11)=[0,3,1,0,0]`) is the Volta+ **fused begin/draw/end** pair:

```
0x0260 SET_DRAW_CONTROL_A   INCRx2  [draw_control_a, instance_count]
0x0264 SET_DRAW_CONTROL_B            (= instance_count, 2nd dword above)
0x0270 DRAW_VERTEX_ARRAY_BEGIN_END_A INCRx2 [first_vertex, vertex_count]
0x0274 DRAW_VERTEX_ARRAY_BEGIN_END_B  (= vertex_count — WRITING IT FIRES THE DRAW)
```

For a 3-vertex single-instance triangle: `SET_DRAW_CONTROL_A=0x200`
(INSTANCE_ITERATE_ENABLE bit9; this is NVK's actual value — topology left 0),
`SET_DRAW_CONTROL_B=1`, `BEGIN_END_A=0` (first_vertex), `BEGIN_END_B=3`
(vertex_count). No legacy `BEGIN`(0x1618)/`END`(0x1614) — that's the
pre-Turing else-branch.

**Topology** comes from the separate-state register because NVK pins
`SET_PRIMITIVE_TOPOLOGY_CONTROL` (`0x1948`) = USE_SEPARATE_TOPOLOGY_STATE(1)
**at channel init** (NOT in the capture). **mabda MUST emit `0x1948=1`
itself** (verifier promoted this from "optional" to mandatory — the
fresh-context OVERRIDE default is undefined; without it `0x1970` is ignored
and topology falls back to `SET_DRAW_CONTROL_A.TOPOLOGY`=0=POINTS → blank).
Then `0x1970 SET_PRIMITIVE_TOPOLOGY = 4` (TRIANGLELIST) and
`0x1644 SET_DA_PRIMITIVE_RESTART = 0`.

> The four `0x0260/64/70/74` dwords are the MME-macro **expansion** — the
> capture shows only `CALL_MME_MACRO(11)`, so they're verified against
> clc597.h + NVK + envytools, not the raw capture bytes. envytools
> independently confirms `0x1970` TRIANGLES=4, `0x1644` restart, and the
> legacy `0x1618`/`0x1614` begin/end offsets mabda avoids.

## Group D — minimal state, clear, readback, sync (verified: mostly-solid)

**Mandatory gates** (a wrong/zero value silently blanks the draw): the color
target + `SET_CT_SELECT`; **`0x1a00` SET_CT_WRITE(0) = `0x1111`** (per-target
RGBA write mask — NVK's MME macro 24; if 0, fragments shade but nothing
writes); **`0x037c` SET_RASTER_ENABLE = 1** (the rasterization gate);
`0x1918` OGL_SET_CULL = 0 (else the lone triangle can be back-face-culled
blank); `SET_SURFACE_CLIP` 64×64; viewport scale/offset `0x0a00`×6 =
scale(32,32,1)/offset(32,32,0); viewport clip `0x0c00`×4 = 64×64/z[0,1];
scissor `0x0e00`×3 = enable+full; topology + the draw (Group C).

**Cheap insurance** (golden-context defaults on a real channel, but mabda
builds a bare pushbuffer so set them): depth/stencil/blend disable
(`0x12cc`/`0x12e8`/`0x1380`/`0x1360` = 0), anti-alias `0x15d0`=0, color
compression `0x19e0`=0, ZT_SELECT `0x1538`=0, viewport coord swizzle
`0x0a18`=`0x6420` (identity), front-face `0x191c`=`0x901` (CCW). The other
~110 captured methods (window-clip, poly-offset, line state, stream-out,
sample positions, scissors 1-15, color targets 1-7, ZCULL) are **skippable**.

**Clear: SKIP** — a fullscreen triangle overwrites every pixel. (Direct
non-MME clear if ever wanted: `SET_CLEAR_SURFACE_CONTROL 0x10f8=0x10`,
`SET_COLOR_CLEAR_VALUE 0x0d80`×4, `SET_CLEAR_RECT 0x0d6c/0x0d70`, then
`CLEAR_SURFACE 0x19d0 = 0x3c` (R|G|B|A).)

**Readback + the two open N7.4 risks:**

1. **L2/ROP→memory coherency.** The EXEC syncobj guarantees *engine
   completion* but **not** a cache flush. The capture gets coherency for free
   from the CE blit's `LAUNCH_DMA` FLUSH_ENABLE bit. mabda's pure-3D
   direct-linear path has no CE op, so **mabda must flush ROP/L2→memory before
   the CPU read** — options: an explicit flush method, mabda's in-tree
   native-render cache-flush, or retain a tiny `0xC5B5` CE copy purely for
   FLUSH_ENABLE. **Must prove on HW at N7.4.**
2. **Vertex distributor.** A `gl_VertexID`-only VS with no bound vertex
   buffers may fault the vertex distributor unless the inactive-attribute
   markers are set (`0x1160 SET_VERTEX_ATTRIBUTE_A` INCRx32 = `0x38200040`,
   as NVK does). Confirm at N7.3/N7.4.

**Sync:** nouveau `DRM_NOUVEAU_EXEC` out-syncobj alone (no semaphore). The
3× `WAIT_FOR_IDLE` (`0x0110`) are NVK's intra-pushbuffer 3D→COPY ordering,
unneeded for mabda's syncobj-gated linear CPU read.

**CE detile fallback** (only if mabda ever renders block-linear): `0xC5B5`
`SET_REMAP_COMPONENTS 0x0708 = 0x00033210`, `OFFSET_IN/OUT 0x0400`×8,
`SET_SRC_BLOCK_SIZE 0x0728`, `LAUNCH_DMA 0x0300 = 0x705`
(PIPELINED|FLUSH_ENABLE|SRC=BLOCKLINEAR|DST=PITCH|MULTI_LINE|REMAP).

## The synthesized minimal mabda render pushbuffer (for N7.2/N7.4)

All on subc0=3D. Op `INCRn` = n consecutive 4-byte methods from the offset;
`IMMD` = single inline immediate. `hi`=(VA>>32)&0xff, `lo`=VA&0xffffffff.

```
# --- framebuffer (linear RGBA8 RT @ va, pitch = align(W*4,128)) ---
0x0ff4 INCR2 [0x00400000, 0x00400000]     # SURFACE_CLIP H(W) / V(H)  (W,H=64 here)
0x0800 INCR9 [hi, lo, pitch_B, H, 0xd5, 0x1000, 1, 0, 0]   # COLOR_TARGET(0) LINEAR
0x19e0 IMMD  0                            # COLOR_COMPRESSION(0) off
0x121c IMMD  1                            # CT_SELECT count=1
0x1a00 IMMD  0x1111                       # CT_WRITE(0) RGBA  (MANDATORY)
0x1538 IMMD  0                            # ZT_SELECT none
# --- viewport / scissor ---
0x0a00 INCR6 [0x42000000,0x42000000,0x3f800000,0x42000000,0x42000000,0]  # scale(32,32,1)/off(32,32,0)
0x0a18 IMMD  0x6420                       # COORD_SWIZZLE identity (insurance)
0x0c00 INCR4 [0x00400000,0x00400000,0,0x3f800000]  # vp clip 64x64, z[0,1]
0x0e00 INCR3 [1, 0x00400000, 0x00400000]  # scissor(0) enable + full
# --- raster / cull / disables ---
0x037c IMMD  1                            # RASTER_ENABLE  (MANDATORY gate)
0x1918 IMMD  0                            # OGL_SET_CULL off
0x191c INCR1 0x901                        # FRONT_FACE CCW (insurance)
0x12cc IMMD  0 ; 0x12e8 IMMD 0 ; 0x1380 IMMD 0 ; 0x1360 IMMD 0 ; 0x15d0 INCR1 0  # depth/stencil/blend/AA off
# --- shaders (VS slot1, FS slot5; programs point AT the 128-B SPH) ---
0x2040 IMMD  0x11 ; 0x2054 INCR2 [VS_hi, VS_lo] ; 0x204c INCR2 [24, 0]
0x2140 IMMD  0x51 ; 0x2154 INCR2 [FS_hi, FS_lo] ; 0x214c INCR2 [24, 4]
# (+ optional stage disables 0x2080=0x20, 0x20c0=0x30, 0x2100=0x40 on a reused channel)
# --- topology + draw ---
0x1948 IMMD  1                            # PRIMITIVE_TOPOLOGY_CONTROL = SEPARATE (MANDATORY, not in capture)
0x1970 IMMD  4                            # PRIMITIVE_TOPOLOGY = TRIANGLELIST
0x1644 IMMD  0                            # PRIMITIVE_RESTART off
0x0260 INCR2 [0x200, 1]                   # DRAW_CONTROL_A / _B(instance_count=1)
0x0270 INCR2 [0, 3]                       # BEGIN_END_A(first=0) / _B(count=3)  <-- fires the draw
# then: EXEC with an out-syncobj; FLUSH L2/ROP->memory; CPU-read the linear RT.
```

## N7 build order (against this reference)

- **N7.2** — clc597 method encoders in `backend_nvidia_push.cyr` (the render
  analogue of `native_nv_push_dispatch`): a `native_nv_push_draw` building the
  sequence above, with CPU asserts pinning every method header/arg (like the
  compute dispatch test). Needs the 3D class (`0xC597`) created via NVIF
  alongside the existing compute class.
- **N7.3** — _DONE._ SM75 VS + FS as **SPH(128B)+SASS** blobs in
  `backend_nvidia_sass.cyr` (`native_nv_sass_render_{vs,fs}`), sliced verbatim
  from this capture's shader-heap BO (NVK's compile of `probe_render.{vert,frag}`)
  and re-emitted from the raw bytes, `nvdisasm`-verified. **Both self-contained**
  — no cbuf/UBO/descriptor reads; the VS's only input is the VertexID sysval
  (`a[0x2fc]`, hardware-supplied), so the render path needs zero constant-buffer
  setup. FS = solid `vec4(0.2,0.4,0.6,1.0)`; VS = fullscreen triangle from
  VertexID writing `a[0x70]` (gl_Position). regcount 24 each.
- **N7.4** — _DONE — THE RENDER GATE IS GREEN._ `programs/nvidia_render_e2e.cyr`
  + `make test-nvidia-render-e2e`: creates the `0xC597` 3D class, binds a linear
  RT, uploads the VS/FS SPH+SASS, builds the draw via `native_nv_push_draw`,
  EXEC + syncobj, and CPU-reads back the rendered `0xFF996633`. **First-try
  green — both flagged risks were non-issues on mabda's path:** NO explicit L2
  flush (the EXEC syncobj + a HOST-coherent BO gives CPU coherency, same as the
  N4 compute STG to GART), and NO vertex-distributor inactive-attribute markers
  (the VertexID-only VS draws fine without them on a fresh channel). No param
  bank (self-contained shaders), no SET_SPA_VERSION, no shader-cache invalidate
  needed. So the minimal `native_nv_push_draw` sequence is sufficient on HW.
  The v2 render-slot public-API wiring (136..168) is the remaining N7.5 piece.
