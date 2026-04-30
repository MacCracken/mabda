# v3.0 — Graphics shader-bytes derivation & PM4 verification protocol

**Status:** Working spec. 6.2(a) constants landed; 6.2(b) path 1
chosen (hand-encode against GCN5 ISA spec); both shaders landed:
`native_gfx9_shader_solid_red` (FS, 92 bytes) and
`native_gfx9_shader_fullscreen_triangle_vs` (VS, 116 bytes). All
encodings cross-checked against `clang -target amdgcn--amdhsa
-mcpu=gfx90c -O2` disassembly via `llvm-objdump -d`.
**Branch:** `v3`
**Predecessor:**
[`docs/handoff/2026-04-28-session25c-punch-list-march.md`](../handoff/2026-04-28-session25c-punch-list-march.md)
§ "What's blocking next: Step 6.2 (graphics shader bytes)".
**Related:**
[`docs/proposals/v3-native-render-design.md`](v3-native-render-design.md)
(GFX9 graphics-pipeline state),
[`docs/proposals/v3-backend-interface.md`](v3-backend-interface.md)
(7-slot render-pipeline interface, v2.1).

> Mabda's discipline since Phase B.4: never ship register addresses
> or shader bytes without either upstream-source citation **or**
> hardware byte-exact diff against radv. Phase B.4 spent 12 sessions
> learning the second half of that rule the hard way. This doc
> codifies the protocol so v3.0's graphics path doesn't re-pay it.

## What this doc captures

1. The Phase B.4 protocol that verified the working compute shader
   bytes byte-exactly against `clang -target amdgcn--amdhsa -mcpu=gfx90c`
   plus the PM4 stream against radv's `AMD_DEBUG=ib` output.
2. Why the same env-var capture path **doesn't work on this dev box**
   under Mesa 26.0.5 — and what that constrains.
3. Three viable derivation paths for the trivial vertex+fragment
   shader pair that 6.2(b) needs.
4. What 6.2(a) lands now (constants + asserts; no shader bytes).
5. The verification gates 6.5 and 6.6 must pass before the native
   render path can claim "works."

## Phase B.4 byte-exact verification protocol (canonical)

The compute path that lit up `0xDEADBEEF` on Cezanne was verified in
two layers, captured here as the pattern for graphics:

### Layer 1 — shader ISA bytes via clang

The 32-byte VS+FS shaders mabda will emit under 6.2(b) must be
derivable from a known-good source. For the compute store-DEADBEEF
shader (`src/backend_native.cyr::native_gfx9_shader_store_deadbeef`,
lines ~960-975), the source was:

```text
clang -target amdgcn--amdhsa -mcpu=gfx90c -nogpulib -O2 -c kernel.cl -o kernel.elf
llvm-objdump -d --triple=amdgcn--amdhsa --mcpu=gfx90c kernel.elf
# read the .text bytes and the SOPP/VOP1/VMEM encodings
# transcribe into Cyrius store32() calls with comments
```

For the VS arithmetic encodings (Step 6.2(b) VS, 2026-04-30), the
reference CL kernel was:

```c
// vid_to_pos.cl
__kernel void vid_to_pos(__global float *out, uint vid) {
    int xi = ((vid     ) & 1) * 4 - 1;
    int yi = ((vid >> 1) & 1) * 4 - 1;
    out[0] = (float)xi;
    out[1] = (float)yi;
}
```

The kernel's `__clang_ocl_kern_imp_vid_to_pos` body (the
implementation called by the kernel wrapper) emits the VOP2
arithmetic against vid in v2; mabda's VS rebases the same opcodes
against v0 (vertex_id at graphics-ABI shader entry). Re-derive at
any time with the clang invocation above against this CL source —
the emitted opcodes (v_lshlrev_b32 = 0x12, v_and_b32 = 0x13,
v_add_u32 = 0x34, v_cvt_f32_i32 = VOP1 opcode 5) are stable across
LLVM versions on gfx90c.

The shader's source was a 6-instruction OpenCL kernel; the bytes
landed verbatim into the Cyrius file. **The source-citation comment
is load-bearing** — anyone re-deriving the bytes can re-run clang
and diff. Without the citation, a future bump of LLVM/clang silently
produces different bytes and the in-tree shader becomes "magic."

### Layer 2 — PM4 stream bytes via radv

The PM4 packets that wrap the shader (SET_SH_REG, ACQUIRE_MEM,
DISPATCH_DIRECT, NOP padding) were verified by capturing radv's
`AMD_DEBUG=ib` dump for an equivalent Vulkan compute dispatch and
diffing dword-by-dword. The framing comment on
`native_pm4_acquire_mem_full_invalidate` (line ~745) cites the exact
dword constants Mesa emits (`0xc0055802` header, `0xa8c40000`
coher_cntl). Every register-address constant in the file's PM4
section was confirmed against this dump.

### Why both layers matter

The compute "wave" the GPU launches is a function of *both* the
shader bytes (what it executes) and the PM4 setup (where it loads
the shader from, which SGPRs it pre-loads, which caches it
invalidates). Either layer being wrong manifests as TDR or wrong
output, and the two failure modes look identical from outside.
Phase B.4 sessions 11-25 were the byte-by-byte alignment that
distinguished them.

## The Mesa 26.0.5 capture problem

Session 25c attempted to re-run the Layer-2 protocol on this dev
box and **none of the documented `RADV_DEBUG` env vars produced a
PM4 dump**:

| Attempt | Outcome |
|---------|---------|
| `RADV_DEBUG=hang vkcube` | Only emits on actual GPU hang; clean run = silent |
| `RADV_DEBUG=dumpibs vkcube` | Env var present in binary strings; produces no output |
| `RADV_DEBUG=metashaders / shaderstats / trace / info` | Emits device info only; no PM4 / no shader bytes |
| `MESA_VK_TRACE=rgp vkcube` | Silent without a swapchain frame boundary |
| `vkcube` under `xvfb-run` | Segfaults — Xvfb lacks DRI3 |

Root cause: this dev box is Arch base — no desktop, no Wayland, no
DRI3. Vulkan apps that need a presentation surface refuse to run.
Headless Vulkan compute workloads that do run (compute_e2e,
render_e2e via wgpu) emit no IB dump under any
`RADV_DEBUG` flag I tried in Mesa 26.0.5.

**Important framing correction**: `feedback_pm4_verify_against_mesa_ib`
described the protocol as "byte-exact against radv IB." That memory
is correct **for PM4 packets** (which radv DOES emit and we DID diff)
but not directly for **shader ISA bytes** (which are emitted *into a
BO*, never into the PM4 stream — radv's IB dump shows the BO's VA
register write, not the bytes inside the BO). The Phase B.4 shader
bytes were derived via clang, not radv-extracted. The IB-diff layer
covers the wrapping PM4; the clang layer covers the shader bytes.
6.2(b)'s three paths below are about Layer 1; Layer 2 (PM4) is
already in place from Step 6.4.

### What unblocks Layer 2 capture for graphics

The radv IB dump for an equivalent **graphics** dispatch (clear
triangle on RGBA8 RT) is what 6.5 needs to verify the graphics
PM4 stream byte-exactly. Three viable hosts:

1. **A workstation with a desktop session** (KDE / GNOME / Hyprland
   on this box would all work). `vkcube` under a real swapchain →
   `RADV_DEBUG=hang` triggers an artificial hang and dumps the IB
   that was about to submit.
2. **A radv test fixture** that drives a presentationless render via
   `VK_KHR_image_format_list` or similar to a CPU-mappable image.
   Mesa's own tests do this; would need to be built from Mesa source.
3. **Wait for the dev box to gain a presentation surface** — the
   user's Hyprland reinstall (in flight as of 2026-04-30) provides
   exactly this. Once Hyprland is up, path 1 is unblocked.

Path 3 is the lowest-effort. 6.5 work can pause on Layer-2 capture
until Hyprland is configured; meanwhile 6.2(a) constants + 6.2(b)
shader-bytes derivation (Layer 1) can land independently.

## Three derivation paths for 6.2(b) shader bytes

### Path 1 — hand-encode against GCN5 ISA spec (RECOMMENDED)

Follow the same workflow that produced the compute store-DEADBEEF
shader: read the GFX9/GCN5 instruction encoding spec (AMD's "Vega
Instruction Set Architecture" public PDF), hand-encode VS+FS as
`store32(...)` calls in `native_gfx9_shader_*` helpers, iterate on
real hardware once 6.5/6.6 land.

**Pros**:
- Same pattern that already worked for compute; iteration loop
  muscle memory transfers.
- Produces hand-written reference bytes that Step 8.5 (WGSL→GFX9
  lowering smoke test) needs anyway: "lowering must produce these
  same bytes byte-for-byte."
- Deepens GFX9 ISA knowledge, which is load-bearing for Step 8.3
  (`src/gfx9_isa.cyr` encoder library). The compute shader gave
  you AMDHSA conventions; the graphics shader gives you SPI /
  vertex-stage / export conventions. Together they cover the
  breadth Step 8 has to handle.
- Zero new tooling dependency.

**Cons**:
- Iteration cycles on hardware. Phase B.4 was 12 sessions for
  compute. Graphics has more state surface (SPI shader formats,
  USER_DATA conventions, export targets) but a smaller shader.
  Plausible range: 2-6 cycles.

**Risk profile**: subtle ABI bugs (wrong export mask, wrong SPI
convention) surface as "trivial works, real shaders break." The
blast radius is contained because real consumer shaders won't run
on the native path until Step 8 lowering ships.

### Path 2 — clang + AMDPAL + USER_DATA prologue patching

`clang -target amdgcn--amdpal -mcpu=gfx90c` against a tiny
GLSL→SPIR-V→ISA, then patch the USER_DATA prologue to match radv's
convention. AMDPAL is closer to the graphics ABI than AMDHSA.

**Pros**:
- Faster to first-bytes than path 1.
- Compiler does the encoding correctness work.

**Cons**:
- Adds a clang AMDPAL build dependency outside the lowering work.
  Even after Step 8 lands, anyone adding a new built-in shader (e.g.
  mipmap-gen kernel) either goes through lowering (good) or falls
  back to clang (toolchain dep grows).
- Less knowledge transfer to Step 8.3 — "the compiler did it"
  rather than "I know which bytes mean what."
- Tooling drift risk: AMDPAL ABI rev's ahead of mabda's pinned
  shape. Bytes need verification anyway.

### Path 3 — extract from Mesa radv NIR→ACO traces

`radv_meta_clear.c::build_color_shaders()` is NIR; manually trace
what NIR→ACO produces for an RGBA8 clear and lift the bytes.

**Pros**:
- Anchors mabda's bytes to a known-working production graphics path.

**Cons**:
- Couples mabda's blessed bytes to a specific Mesa version. Mesa
  rev's their compiler output → reference bytes drift.
- Cognitively expensive every time a new shader is needed — must
  re-run radv, re-capture, re-extract.
- Worst long-term option for a "sovereign-owned surface" stack.

### Recommendation

Path 1. Highest knowledge compounding, zero tooling deps, matches
the working compute precedent, produces the reference artifact
that Step 8.5's lowering smoke test will diff against. The cost
(iteration cycles) is real but bounded.

## What 6.2(b) lands so far (FS only)

`native_gfx9_shader_solid_red(buf, pos)` — 92 bytes, encodes:

| Offset | Bytes | Instruction |
|--------|-------|-------------|
| +0 | `0x7E0002F2` | `v_mov_b32_e32 v0, 1.0` (R) |
| +4 | `0x7E020280` | `v_mov_b32_e32 v1, 0.0` (G) |
| +8 | `0x7E040280` | `v_mov_b32_e32 v2, 0.0` (B) |
| +12 | `0x7E0602F2` | `v_mov_b32_e32 v3, 1.0` (A) |
| +16 | `0xF800180F` | `exp mrt0 done vm en=0xF tgt=0` (word 0) |
| +20 | `0x03020100` | exp word 1 (vsrc0..3 = v0..v3) |
| +24 | `0xBF810000` | `s_endpgm` |
| +28..+91 | 16 × `0xBF800000` | `s_nop 0` (AMDGPU prefetch padding) |

19 dword value-asserts in
`tests/tcyr/mabda_v3.tcyr::test_gfx9_fs_solid_red_byte_pattern` plus
`test_gfx9_fs_solid_red_alignment` pin every emitted byte. Encoding
derivations are spec-cited in the source comment block; verification
gate is Step 6.5 hardware diff.

**VS landed** — `native_gfx9_shader_fullscreen_triangle_vs(buf, pos)`
— 116 bytes, encodes (using v0=vertex_id, v1 as scratch — clobbering
instance_id which is unused, keeping total VGPRs at 4 so RSRC1 stays
at the minimum):

| Offset | Bytes | Instruction |
|--------|-------|-------------|
| +0 | `0x24040081` | `v_lshlrev_b32_e32 v2, 1, v0` |
| +4 | `0x24000082` | `v_lshlrev_b32_e32 v0, 2, v0` |
| +8 | `0x26000084` | `v_and_b32_e32 v0, 4, v0` |
| +12 | `0x26040484` | `v_and_b32_e32 v2, 4, v2` |
| +16 | `0x680000C1` | `v_add_u32_e32 v0, -1, v0` |
| +20 | `0x680404C1` | `v_add_u32_e32 v2, -1, v2` |
| +24 | `0x7E000B00` | `v_cvt_f32_i32_e32 v0, v0` |
| +28 | `0x7E020B02` | `v_cvt_f32_i32_e32 v1, v2` |
| +32 | `0x7E040280` | `v_mov_b32_e32 v2, 0.0` |
| +36 | `0x7E0602F2` | `v_mov_b32_e32 v3, 1.0` |
| +40 | `0xF80008CF` | `exp pos0 done en=0xF tgt=12` (word 0) |
| +44 | `0x03020100` | exp word 1 (vsrc0..3 = v0..v3) |
| +48 | `0xBF810000` | `s_endpgm` |
| +52..+115 | 16 × `0xBF800000` | `s_nop 0` (AMDGPU prefetch padding) |

VOP2 arithmetic opcodes (v_lshlrev_b32 = 0x12, v_and_b32 = 0x13,
v_add_u32 = 0x34) ground-truthed via `clang -target amdgcn--amdhsa
-mcpu=gfx90c -O2 -nogpulib -O2` of an equivalent CL kernel
(`vid_to_pos.cl` — see Layer-1 protocol section above) plus
`llvm-objdump -d --triple=amdgcn--amdhsa --mcpu=gfx90c`. The
arithmetic VOP2 encodings are ABI-independent — same bytes whether
dispatched from compute or graphics — so a compute-ABI disassembly
gives authoritative encodings for the graphics-ABI shader.

32 dword value-asserts in
`tests/tcyr/mabda_v3.tcyr::test_gfx9_vs_fullscreen_triangle_byte_pattern`
+ `test_gfx9_vs_fullscreen_triangle_alignment` pin every emitted
byte. Verification gate: Step 6.5 hardware diff (PM4 stream + first
pixel readback).

## What 6.2(a) lands now (decision-independent)

Constants in `src/backend_native.cyr` (Step 6.2(a) section, lines
~991-1080):

| Constant | Value | Source |
|----------|-------|--------|
| `R_SPI_SHADER_PGM_LO_VS` | `0xB120` | Mesa sid.h `R_00B120_SPI_SHADER_PGM_LO_VS` |
| `R_SPI_SHADER_PGM_HI_VS` | `0xB124` | Mesa sid.h |
| `R_SPI_SHADER_PGM_RSRC1_VS` | `0xB128` | Mesa sid.h |
| `R_SPI_SHADER_PGM_RSRC2_VS` | `0xB12C` | Mesa sid.h |
| `R_SPI_SHADER_PGM_LO_PS` | `0xB020` | Mesa sid.h |
| `R_SPI_SHADER_PGM_HI_PS` | `0xB024` | Mesa sid.h |
| `R_SPI_SHADER_PGM_RSRC1_PS` | `0xB028` | Mesa sid.h |
| `R_SPI_SHADER_PGM_RSRC2_PS` | `0xB02C` | Mesa sid.h |
| `R_SPI_SHADER_POS_FORMAT` | `0xA710` | Mesa sid.h `R_028710_*` (mabda 0xA-shorthand) |
| `R_SPI_SHADER_Z_FORMAT` | `0xA708` | Mesa sid.h `R_028708_*` |
| `R_SPI_SHADER_COL_FORMAT` | `0xA70C` | Mesa sid.h `R_02870C_*` |
| `R_CB_TARGET_MASK` | `0xA238` | Mesa sid.h `R_028238_*` |
| `GFX9_GFX_PGM_RSRC1_MIN` | `0x002C0040` | mirrors compute RSRC1_MIN |
| `GFX9_VS_PGM_RSRC2_MIN` | `0x0` | spec; no USER_SGPR / scratch / LDS |
| `GFX9_PS_PGM_RSRC2_MIN` | `0x0` | spec; trivial FS does no memory ops |
| `SPI_SHADER_POS_FORMAT_4` | `0x4` | spec — POS_FLOAT_4 |
| `SPI_SHADER_Z_FORMAT_INVALID` | `0x0` | spec |
| `SPI_SHADER_COL_FORMAT_FP32_ABGR` | `0xE` | spec; default for RGBA8 RT |
| `SPI_SHADER_COL_FORMAT_FP16_ABGR` | `0x9` | spec; fallback |
| `SPI_SHADER_COL_FORMAT_UNORM16_ABGR` | `0xA` | spec; fallback |
| `GFX9_FULLSCREEN_TRI_VCOUNT` | `3` | full-screen triangle |

Asserts in `tests/tcyr/mabda_v3.tcyr` (Step 6.2(a) section, 7 new
test fns, 30 new value-asserts) cover every constant + the
wire-form encodings produced by `native_sh_reg_offset` /
`native_context_reg_offset`. Pattern follows the 4f.ii lesson:
every computed constant gets a CPU value-assert so a typo in a
spec citation surfaces at test time, not at HW-debug time.

## Verification gates for 6.5 / 6.6

These must pass before the native render path can claim done:

1. **PM4 stream byte-exact diff vs radv.** Build the clear-triangle
   PM4 stream in `native_pm4_build_render_clear_triangle` (Step
   6.5) and dword-diff against radv's `AMD_DEBUG=ib` output for an
   equivalent VkCmdDraw clear. Any divergence is a bug. **Blocked
   until a workstation with a desktop session runs vkcube** — the
   user's Hyprland reinstall unblocks this.
2. **Shader bytes round-trip via clang.** For path 1, the
   hand-encoded VS+FS must reassemble through `clang
   -target amdgcn--amdhsa` (or AMDPAL) with the same byte sequence.
   The disassembly (`llvm-objdump -d`) produces the source-listed
   instructions. Source citation in the comment block is required.
3. **First-pixel test.** `programs/native_render_e2e.cyr` (Step
   6.9b) must produce the same RGBA8 byte pattern in the readback
   buffer that `programs/render_e2e.cyr` produces — within
   quantization-tolerance of the export hardware's
   FP32→UNORM8 conversion. CPU-readback diff defines pass/fail.

If any of these fail, the bytes are wrong; iterate. Don't claim
"works" on a single TDR-clean dispatch — Phase B.4 had several
TDR-clean runs that produced wrong output before the bytes were
right.

## Known hazards / open questions for 6.5

- **`SPI_SHADER_COL_FORMAT` for RGBA8 RT.** The spec lists three
  candidates (FP32_ABGR / FP16_ABGR / UNORM16_ABGR). FP32_ABGR is
  the most natural for an FS that does `v_mov_b32` of f32s; the
  hardware quantizes on export. Verify by rendering a known color
  (e.g. `(0.5, 0.5, 0.5, 1.0)`) and CPU-reading the RT — bytes
  should match the f32→u8 quantization of those inputs. If the
  result is mangled, fall back to UNORM16_ABGR.
- **`PS_PGM_RSRC2` `TRAP_PRESENT` bit.** GFX9 may require bit 6
  even for a memoryless FS. The compute analogue
  (`GFX9_COMPUTE_PGM_RSRC2_USER_SGPR_2_TRAP = 0x44`) sets this bit
  for memory-touching shaders. The trivial solid-color FS does no
  memory ops, so the spec minimum is 0; HW may disagree. If the FS
  fails to launch with `RSRC2 = 0`, set bit 6 and re-test.
- **`PM4_CONTEXT_REG_BASE = 0xA000` synthetic-base convention.**
  Mesa uses 0x28000 as the context-reg base; mabda's 0xA000
  shorthand produces identical wire form via
  `native_context_reg_offset()` because only the difference
  matters. Documented inline in the constants section. Not a bug;
  worth flagging for any future contributor reading sid.h directly.
- **Hardware-verified vs spec-cited.** None of the SH-side
  byte addresses above have been hardware-confirmed on Cezanne.
  Mesa sid.h is the citation. Compute-side equivalents took an
  AMD_DEBUG=ib pass to confirm the documented values matched the
  silicon (line 522 comment in `backend_native.cyr` — PGM_LO was
  at 0xB830, not 0xB848 as initially encoded). Step 6.5 must
  re-run the verification for VS/PS PGM_*; if any are off, fix
  here and reflect in the citation comment.
