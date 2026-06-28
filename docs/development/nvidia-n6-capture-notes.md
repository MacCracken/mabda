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

(Per-field bit decode of the TIC/TSC — sourced from Mesa NVK `nil/tic.c` /
`nvk_sampler.c` + envytools `tic.xml`/`tsc.xml` — is the N6.2b reference; a
research pass is fetching it. The build protocol is the PM4/QMD one:
**byte-diff mabda's TIC/TSC builder against these golden dwords** before
trusting it.)

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
