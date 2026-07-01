# NVIDIA N6.2 — NVK Texture-Sampling Capture Findings

Decoded reference for building mabda's native texture **sampling** (N6.2),
from a **known-good NVK compute-sampling dispatch captured on the TU116**
(2026-06-28). The capture **harness** is preserved at
**`tools/nvidia-capture/`** (`probe_tex.comp` GLSL + `vk_compute_tex.c`
headless NVK sampling app, run under the `nouveau_capture.c` interposer); the
raw bytes do **not** live in the repo (per the punchlist). Re-capture with:

```
cd tools/nvidia-capture
glslc -fshader-stage=comp probe_tex.comp -o probe_tex.spv
gcc -O2 vk_compute_tex.c -o vk_compute_tex -lvulkan
NV_CAP_DIR=./nvcaptex LD_PRELOAD=./nouveau_capture.so ./vk_compute_tex probe_tex.spv
```

The probe uploads a 1×1 RGBA8 texel `(0x11,0x22,0x33,0x44)`, samples it with a
NEAREST/clamp sampler at `(0.5,0.5)`, packs to RGBA8, stores to a buffer;
NVK reads back `0x44332211` (PASS) — so the captured TIC/TSC/SASS are golden.

## The big simplification — BOUND texture model (TIC/TSC index = immediate)

The captured sampling SASS issues:

```
MOV  R0, 0x3f000000              # u = 0.5f
MOV  R1, 0x3f000000              # v = 0.5f
TEX.SCR.LZ R2, R4, R0, R1, 0x1, 0x0, 2D   # <-- TIC index = 0x1, TSC index = 0x0 IMMEDIATES
FADD.FTZ.SAT R0, R4, RZ          # saturate sampled R, *255, round, to-int, pack...
FMUL.FTZ R0, R0, 255
FRND.FTZ R0, R0
F2I.FTZ.U32.TRUNC R0, R0
R2UR UR0, R0
... (G,B,A the same) -> STG the packed RGBA8 to the output buffer
```

The `TEX` instruction encodes the **TIC index (0x1) and TSC index (0x0) as
immediate operands** — the *bound* texture model, NOT bindless. This is far
simpler than NVK's full Vulkan descriptor indirection (its cbuf0 is a root
table pointing at descriptor sets). **mabda's sampling path therefore needs
only:** write a TIC at a fixed index, a TSC at a fixed index, bind the
pools, and run a SASS whose `TEX` names those indices. Coords are MOV
immediates (fixed-coord proof-of-life); the output pointer comes from cbuf0
exactly like the N4 store kernel (`c[0x0]` + the ptxas param ABI).

## The dispatch is structurally identical to the N4 store dispatch

Capture `089` (the sampling dispatch) is byte-shape-identical to the N4 store
dispatch `082`: `subc4 0x0100` → compute invalidates `0x0298/0x1424/0x0244`
→ 3D housekeeping `0x1424/0x0da4/0x3890` → `SEND_PCAS_A(qmd>>8)` →
`SEND_SIGNALING_PCAS_B(0x3)`. **No new dispatch methods for sampling.** The
sampling-ness lives entirely in (a) the QMD's PROGRAM_ADDRESS → a TEX shader,
and (b) the TIC/TSC pools being populated + bound.

## TIC/TSC pool binding — already in mabda's captured stream

The pools are bound once in the queue-init continuation (capture `077`, the
same methods mabda already decoded at N4):

```
SET_TEX_HEADER_POOL  (clc5c0 0x1574, A/B/C) = (0x3f, 0xfdfff000, 0x3ff)  # TIC pool VA 0x3F_FDFFF000, max index 0x3ff
SET_TEX_SAMPLER_POOL (clc5c0 0x155c, A/B/C) = (0x3f, 0xfdfcf000, 0xfff)  # TSC pool VA 0x3F_FDFCF000, max index 0xfff
```

mabda allocates its own TIC pool BO + TSC pool BO, writes descriptors into
them, and emits these two methods (pointing at its pool VAs) in the dispatch
preamble — the N4 `native_nv_push_dispatch` gains a sampling-aware variant.

## Golden descriptors (re-capture to regenerate; bytes are throwaway)

### TIC — golden, 1×1 RGBA8 linear (PITCH) image @ VA 0x3F_FDF9C000, at TIC index 1

```
dw0 = 0x58d24908   # format word: R8G8B8A8 component sizes + UNORM data types + RGBA swizzle
dw1 = 0xfdf9c000   # ADDRESS_BITS_31_0  (image data VA low)
dw2 = 0x0060003f   # low16 = ADDRESS_BITS_47_32 = 0x003f  (=> VA 0x3F_FDF9C000); hi = layout/header-version (PITCH)
dw3 = 0x00070000
dw4 = 0xe0800000   # width/height region (WIDTH_MINUS_ONE=0, HEIGHT_MINUS_ONE=0 packed with flags)
dw5 = 0x80000000
dw6 = 0x03000000
dw7 = 0x00000000
```

### TSC — golden, NEAREST filter / CLAMP_TO_EDGE wrap, at TSC index 0

```
dw0 = 0x00026092   # ADDRESS_U/V/W = CLAMP_TO_EDGE, MAG/MIN_FILTER = NEAREST, MIP = NEAREST
dw1 = 0x00000291
dw2..dw7 = 0
```

### Authoritative TIC/TSC field decode (N6.2b — built + CPU-asserted)

Layout = envytools rnndb `gm200_texture.xml` (TIC v2, Maxwell2→Turing) +
`g80_texture.xml`/`g80_defs.xml` (TSC + enums). Every field byte-diffed
against the golden above. Implemented in `src/backend_nvidia_tex.cyr`
(`native_nv_tic_build_2d_rgba8`, `native_nv_tsc_build_nearest_clamp`); CPU
asserts in `tests/tcyr/nvidia.tcyr::test_nv_tic_tsc_build`. The empirical
multi-size capture (`vk_compute_tex W H`) confirmed the dimension fields:
8×4 → dw4 low=7, dw5 low=3; 64×1 → dw4 low=0x3f.

**CRITICAL — the golden is BLOCKLINEAR, mabda must emit PITCH.** TIC dw2
bits[23:21] = `HEADER_VERSION`; golden = 3 (`BLOCKLINEAR`). NVK gave its
OPTIMAL-tiled image a gob-tiled layout. mabda's textures are row-major
linear, so the builder emits `HEADER_VERSION = 2 (PITCH)` with a real pitch
in dw3 — a BLOCKLINEAR TIC would gob-swizzle the fetch and read garbage from
linear data (a 1×1 image is byte-identical in both, which is why the golden
1×1 sampled OK; divergence shows from ~16×8 up).

TIC fields (PITCH build, image @ VA `A`, width `W`, height `H`):
- dw0 = `0x58D24908` — `A8B8G8R8` + UNORM×4 + swizzle X=R,Y=G,Z=B,W=A (const for RGBA8)
- dw1 = `A & 0xFFFFFFE0` — `ADDRESS_BITS_31_5` (32 B aligned)
- dw2 = `((A>>32)&0xFFFF) | (2<<21)` — addr hi | `HEADER_VERSION=PITCH`
- dw3 = `(round_up(W*4,32)>>5) | 0x00070000` — `PITCH_BITS_20_5` | NVK LOD-quality
- dw4 = `(W-1) | (1<<23) | (7<<29)` — `WIDTH_MINUS_ONE` | `TEXTURE_TYPE=2D` | `BORDER_SIZE=SAMPLER_COLOR`
- dw5 = `(H-1) | (1<<31)` — `HEIGHT_MINUS_ONE` | `NORMALIZED_COORDS`
- dw6 = `0x03000000` (aniso-spread NVK consts) · dw7 = 0

TSC fields (NEAREST / CLAMP_TO_EDGE — reproduces the golden exactly):
- dw0 = `0x00026092` — `ADDRESS_U/V/W = CLAMP_TO_EDGE(2)` | NVK sRGB/font defaults
- dw1 = `0x00000291` — `MAG/MIN = NEAREST(1)`, `MIP = NEAREST(2)`, cubemap-filter default
- dw2..dw7 = 0 (LOD clamps 0 → base level; border color unused under CLAMP)

**Pitch caveat for N6.2c:** the PITCH field is a 32-byte multiple, so for
tightly-packed linear data the row stride matches only when `W*4 % 32 == 0`
(`W % 8 == 0`). A 1×1 sample (single row) is unaffected — use a 1×1 or
8-aligned-width texture for the sampling proof; arbitrary-width linear
sampling needs the texture written at the padded stride (a v0+ follow-up).

### Sampling QMD (256 B) — same shape as the N4 store QMD

Non-zero dwords match the store QMD field-for-field except the per-shader
addresses: `SM_GLOBAL_CACHING_ENABLE` (dw4=0x40), `API_VISIBLE_CALL_LIMIT`
(dw11), grid (1,1,1), `dw17=0x22240000`, version `dw18=0x00010022`,
`dw20=0x00121803` (REGISTER_COUNT_V=24 for NVK's shader — mabda's captured
sampling SASS will carry its own regcount), CONSTANT_BUFFER[0]/[1], and
PROGRAM_ADDRESS. **The QMD builder is unchanged for sampling** — only the
program address (a TEX shader) and regcount differ.

## N6.2 build order (against this reference)

1. **N6.2b** — `backend_nvidia_tex.cyr` (or extend an existing module): a
   32-byte TIC builder (linear RGBA8 2D, arbitrary W/H/VA) + a 32-byte TSC
   builder (NEAREST/clamp), each **byte-diffed against the golden** above.
   CPU asserts pin every dword.
2. **N6.2c** — capture a **bound-texture sampling SASS** for mabda (a CUDA
   `tex2D`/texture-object fetch + STG, or hand-encode the
   `MOV/TEX 0x0,0x0/pack/STG` sequence) into `backend_nvidia_sass.cyr`;
   verify via the N5.2 `nvdisasm` oracle. Use TIC index 0 / TSC index 0 for
   mabda (vs NVK's 1/0) so the pool writes are at offset 0.
3. **N6.2c** — a sampling dispatch: allocate TIC+TSC pool BOs, write the
   descriptors, emit `SET_TEX_HEADER_POOL`/`SET_TEX_SAMPLER_POOL` +
   `SEND_PCAS` (the QMD points at the sampling SASS, cbuf0 carries the output
   ptr). EXEC + wait (absolute deadline). `programs/nvidia_texture_sample_e2e.cyr`
   reads back the sampled texel. **EXIT: a GPU-sampled texel is correct.**

This closes the sampled-texel half of N6.4. The N6.1/N6.3 v1 texture slots
(CPU create/write/read/release) are already done + HW-proven.

## N6.2c-i — the mabda sampling SASS (DONE)

`native_nv_sass_sample_tex` (`src/backend_nvidia_sass.cyr`) is the ptxas
capture of `tools/nvidia-capture/store_tex.cu` (`tex2D<uchar4>` → pack →
store), 256 B / 16 instrs, CPU-asserted (`nvidia.tcyr`). Key facts the
sampling dispatch (N6.2c-ii) must honor:

- **BOUND TEX**: `TEX.SCR.LL R4, R2, R6, R2, 0x0, 0x58, 2D` — TIC index **0**,
  TSC index **0x58** as immediates. The dispatch writes mabda's TIC at TIC-pool
  index 0 and TSC at TSC-pool index 0x58 (byte offset 0x58*32 = 0xB00).
- **Output ptr** at `c[0x0][0x168]` (ptxas param ABI; note +0x168, not the
  store kernel's +0x160 — the unused `cudaTextureObject_t` param sits at +0x160).
- **REGISTER_COUNT = 10** (the QMD's REGISTER_COUNT_V).
- **GPU-scope store**: the captured `STG.E.SYS` control word `0x..c10e904`
  was patched to `.STRONG.GPU` `0x..c114904` — `.SYS` hangs on a GART output
  BO (the proven store-kernel finding); round-trips through `nvdisasm`.
- **Coord** is a clamp-friendly constant (`R2 = -32.0f`) → texel (0,0) on a
  **1×1** NEAREST/CLAMP texture. Use a 1×1 texture for the proof; arbitrary
  coords/sizes are a follow-up.

## N6.2c-ii — the sampling dispatch (DONE + HW-proven)

`programs/nvidia_texture_sample_e2e.cyr` (`make test-nvidia-texture-sample-e2e`)
binds a TIC/TSC pool and runs a bound-texture TEX compute dispatch that samples
a 1×1 RGBA8 texel and stores the packed result — read back on the CPU. It
samples **two distinct texels** (0x44332211, then 0x8899AABB from a TIC
repointed at a second texture VA) so the readback provably *tracks* the bound
texture rather than echoing a constant.

### The UINT-vs-UNORM correctness catch (load-bearing)

The N6.2b TIC builder emits **UNORM** (`0x58D24908`) — correct for the NVK
normalized-float probe, whose SASS denormalizes (`FMUL 255`/`F2I`). But mabda's
sampling SASS (`native_nv_sass_sample_tex`, from a `tex2D<uchar4>` element-read
kernel) does **TEX → PRMT with NO float conversion** — it expects the TEX unit
to deliver the *raw integer texel*. On Turing the TEX unit normalizes iff the
TIC's `*_DATA_TYPE` fields say UNORM; for raw integers they must say **UINT**.
So the sampling path needs a UINT header: `native_nv_tic_build_2d_rgba8_uint`,
dw0 = `0x58D49208` (the four data-type fields UNORM(2)→UINT(4), `^0x6DB00`;
`G80_TIC_TYPE` from envytools g80_defs.xml). A UNORM TIC here would PRMT the low
bytes of IEEE-754 floats → garbage. (A 1×1 sample on UNORM would have *looked*
plausible only by accident; the two-texel HW check + the type fix make it real.
UINT textures are point-sampled only — the NEAREST TSC already is.)

### clc5c0 pool-bind + invalidate methods (verified vs clc5c0.h + NVK)

`native_nv_push_dispatch_sample` = the N4 store dispatch (SET_OBJECT + memory
windows + SEND_PCAS) plus, before SEND_PCAS:

```
SET_TEX_HEADER_POOL  A/B/C  0x1574/78/7C  INCR×3  (A=VA[48:32] 17b, B=VA[31:0], C=MAXIMUM_INDEX 22b = entries-1)
INVALIDATE_TEXTURE_HEADER_CACHE_NO_WFI  0x0244  IMMD imm 0 (LINES_ALL)
SET_TEX_SAMPLER_POOL A/B/C  0x155C/60/64  INCR×3  (C is 20b)
INVALIDATE_SAMPLER_CACHE_NO_WFI         0x1424  IMMD imm 0
INVALIDATE_SKED_CACHES                  0x0298  IMMD imm 0  (compute-scheduler flush; NVK does this at every compute cmdbuf begin)
```

Pool VAs are written **unshifted** (A = va>>32, B = va low-32). mabda uses a
64 KiB pool BO with `MAXIMUM_INDEX = 0x7FF` (2048 32-byte entries — covers TIC
index 0 and TSC index 0x58). The cache invalidates aren't strictly required on
a truly-cold channel but cannot hang and guard against a warm line serving a
stale texel, so all three are emitted (matching NVK). Every pushbuffer dword +
the UINT dw0 are CPU-asserted (`nvidia.tcyr` `test_nv_push_dispatch_sample` /
`test_nv_tic_build_uint`, 216 total).

QMD is unchanged from the store path: program = `native_nv_sass_sample_tex`,
regcount = 10, cbuf0 = param bank with the output VA at **+0x168**. EXEC + wait
on an absolute deadline. **This closes N6.4 — N6 (textures) is complete.**
