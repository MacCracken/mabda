# N9 — SPIR-V → SASS (SM75) for the native NVIDIA backend

**Status:** done, 2026-11-16 (N9.1–N9.6; the done-when is met, see §7). Roadmap: v4.4.0. Issue:
[`2026-11-02-n9-spirv-to-sass-compiler-request.md`](../development/issues/archived/2026-11-02-n9-spirv-to-sass-compiler-request.md).

**Done when:** `gpu_shader_module_create_spirv` on an NVIDIA context compiles ranga's 11 kernels,
and ranga's four GPU suites pass on the TU116 (`chew`) with an NVIDIA context.

## 1. What has to compile

ranga's kernels at `ranga@de54123` (`src/gpu_kernels.cyr`, emitted by `src/gpu_spirv.cyr`). They
were dumped to SPIR-V and disassembled for this note. The issue (filed from ranga 2.3.2) lists
`OpFDiv`, `OpSelect` and `FClamp`; none of the 11 kernels uses them today. The ops that do appear,
across all 11:

| Class | SPIR-V |
|---|---|
| memory | `OpLoad`, `OpStore`, `OpAccessChain` (storage buffers, 2-3 bindings), `OpLoad` of `GlobalInvocationId` + `OpCompositeExtract` |
| integer | `OpIAdd`, `OpISub`, `OpIMul`, `OpUDiv`, `OpUMod`, `OpBitwiseAnd`, `OpBitwiseOr`, `OpShiftLeftLogical`, `OpShiftRightLogical`, `OpUGreaterThan` |
| float | `OpFAdd`, `OpFSub`, `OpFMul`, `OpConvertUToF`, `OpConvertFToS`, GLSL.std.450 `FMax` / `FMin` / `Floor` |
| control | one structured selection per kernel (the bounds guard), `OpBranch`, `OpReturn` |
| shape | `LocalSize 256 1 1`, f32 and u32 only |

The broader corpus N9 is tested against: mabda's own `native_spirv_*` kernels where the op set
allows, and the 24 control-flow kernels in `tests/spirv/control_flow/`. Those were written for
AMD in 4.4.0 and are the "same in N9's SM75 path" half of the compiler-limits item.

## 2. Architecture

```
SPIR-V ─ validate ─ tables ─ spirv_lower ─ uniformity ─┬─ gfx9_isel ─ gfx9_regalloc ─ gfx9_waitcnt ─ gfx9 encode   (AMD, unchanged)
         (shared front end: src/spirv_*.cyr, mir.cyr)   └─ sm75_isel ─ sm75_regalloc ─ sm75_sched  ─ sm75 encode   (N9)
```

The whole front end is reused as is: validation, the type/const/decoration tables, lowering to MIR
and the uniformity sweep. Lowering's integer-division macro is already written in target-neutral
MIR primitives (`VMOV`, `CVT_F32_U`, `RCP_IFLAG`, `MUL_HI`, `ICMP` + `CNDMASK`), so it maps onto SM75
one primitive at a time (§3). The GFX9-specific macros (f32 / f64 `OpFDiv` via `DIV_SCALE` /
`DIV_FMAS`, f64 sqrt) have no SM75 form and fail loud in `sm75_isel` until a target-aware lowering
lands (§7). ranga does not need them.

New modules, all `@internal`, all under the 4.4.0 scratch-buffer contract (every buffer passed
with its byte length, capacities derived):

- `src/sm75_encode.cyr` — the 128-bit instruction builder and one encoder per instruction form.
- `src/sm75_isel.cyr` — MIR → SM75 instruction records (the 48-byte GISEL record shape, SM75 op tags).
- `src/sm75_regalloc.cyr` — one GPR file (R0..R254, RZ = R255) plus predicates.
- `src/sm75_compile.cyr` — emission with the control words (stall / yield / scoreboards / wait
  mask; the v1 schedule is small enough to live with the emitter rather than in a separate
  `sm75_sched.cyr`), the driver `sm75_compile(cc, spirv, n, sass, sass_len, desc)`, and
  `sm75_compile_spirv`, the CPU compile frame.
- In `src/backend_nvidia.cyr`: `_nv_compile_spirv_sass` (the pure-CPU compile frame, the
  `_native_compile_spirv_isa` analogue), and `shader_module_create` accepting `SHADER_SRC_SPIRV`.

`gfx9_regalloc` is not reused. Its SGPR/VGPR split, ABI register bases and EXEC-pair ceiling are
GFX9's. The loop-liveness extension and the dead-load rule (both 4.4.0 lessons) are re-applied in
`sm75_regalloc`.

## 3. Instruction selection

Every value lives in a 32-bit GPR; an f64 value takes an even-aligned pair. Turing's uniform
datapath (UR registers, `U*` instructions) is not used in v1: uniformity only matters for AMD's
SALU/VALU split, and on NVIDIA a uniform value in an R register is correct.

| MIR | SM75 |
|---|---|
| `IADD` / `ISUB` | `IADD3` (negated second source for SUB) |
| `IMUL` | `IMAD` (`a * b + RZ`) |
| `SHL` / `SHR` / `ASHR` | `SHF.L.U32` / `SHF.R.U32.HI` / `SHF.R.S32.HI` |
| `AND` / `OR` / `XOR` | `LOP3.LUT` (0xC0 / 0xFC / 0x3C) |
| `ICMP_*` | `ISETP.<cmp>.<U32/I32>.AND P0, PT, a, b, PT` |
| `FADD` / `FSUB` / `FMUL` / `FMA` | `FADD` (negated source for SUB) / `FMUL` / `FFMA` |
| `FMIN` / `FMAX` | `FMNMX` (predicate `PT` = min, `!PT` = max) |
| `FFLOOR` | `FRND.FLOOR` |
| `CVT_F32` / `CVT_F32_U` / `CVT_I32` | `I2F` / `I2F.U32` / `F2I.TRUNC.NTZ` |
| `RCP_IFLAG` | `MUFU.RCP` |
| `MUL_HI` | `IMAD.HI.U32` |
| `VMOV` | `MOV` (immediate or register) |
| `CNDMASK` / `ISELECT` | `SEL dst, true, false, P0` |
| `LOAD` / `STORE` | `IMAD.WIDE.U32 Ra, off, 1, c[0x0][0x160 + 8*binding]`, then `LDG.E.STRONG.GPU` / `STG.E.STRONG.GPU` |
| builtins | `S2R SR_TID.X/Y/Z`, `S2R SR_CTAID.X/Y/Z`; GlobalInvocationId = `IMAD ctaid, LocalSize, tid` (LocalSize is a compile-time constant) |
| `RET` | `EXIT` |

Memory scope is `.STRONG.GPU`, never `.SYS`: a system-scope store to a GART buffer hangs
(HW-observed for the captured kernels, `src/backend_nvidia_sass.cyr`).

**Conditions.** As on AMD, an integer compare's result is a flag: here predicate P0, written by
`ISETP` and read by `SEL` or a predicated branch. The 4.4.0 freshness rule carries over unchanged:
a reader re-emits its compare unless the record just before it is that compare. Turing has seven
predicates, so allocating them would avoid re-emission; v1 uses P0 alone because the rule is
proven and simple.

**Control flow.** NVIDIA's SIMT keeps a program counter per thread (Volta independent thread
scheduling), so a divergent branch is just a predicated `BRA`. There is no EXEC mask to save,
narrow or restore. A divergent if becomes `@!P0 BRA merge`. A loop exit becomes `@!P0 BRA exit`,
and the back-edge an unconditional `BRA`. An `OpReturn` inside a divergent region is `EXIT` and is
**legal** here, since it retires only the threads that execute it, unlike on AMD where it ended
the wave. Phi copies use 4.4.0's edge placement (forward copies before the branch, back-edge
copies after the exit test).

Reconvergence (`BSSY` / `BSYNC` around divergent regions, as ptxas emits) is not required for
correctness of these kernels: there are no warp-synchronous operations, and threads that never
reconverge still finish. v1 omits it, and each divergent region runs serialized. §7 lists it.

## 4. Registers

Linear scan over the selected list, one GPR file, with the 4.4.0 rules:

- loop-carried values extend to the loop's end;
- each value is allocated once (phi re-definitions share the register);
- a dead load keeps its register to the end (a load lands whenever memory returns).

N9 kernels use no local memory, so R1 (the CUDA ABI's stack pointer) is not set up and is
allocated like any other register. The QMD's `REGISTER_COUNT_V` is `max(4, used + 2)`: Volta
and later burn two GPRs per thread for the program counter (NAK `hw_reserved_gprs`). That
matches ptxas's 4 for the captured `store_deadbeef` (pinned in N9.3).

## 5. Scheduling (control words)

Every SM75 instruction carries its own scheduling bits: stall cycles (bits 105-108), yield (109),
write and read scoreboard indices (110-112, 113-115; 7 = none) and a scoreboard wait mask
(116-121). Getting them wrong produces garbage or hangs without any error, so v1 is deliberately
conservative:

- **fixed-latency ops** (NAK's "coupled" classes: `IADD3`, `IMAD`, `LOP3`, `SHF`, `ISETP`, `FADD`,
  `FMUL`, `FFMA`, `FMNMX`, `SEL`, `MOV`) stall 15 cycles, more than any SM75 fixed latency;
- **variable-latency ops** ("decoupled": `S2R`, `LDG`, `MUFU`, `I2F`, `F2I`, `FRND`) write
  scoreboard 0, and the next instruction waits on it;
- **`STG`** sets read scoreboard 1 (its source registers are read late), and the next instruction
  waits on it.

That serializes everything. These kernels are short, and the gate is correctness. Using NAK's
SM75 latency tables (`sm75_instr_latencies.rs`) to tighten the stalls is a later bite (§7), and
each tightening has to keep the hardware gates green.

**v2 (4.4.0, the first N9 follow-on): landed.** `sm75_schedule` (`src/sm75_compile.cyr`) sets
every control word from the Turing tables NAK carries, for the subset `sm75_isel` emits:
- coupled read-after-write latencies:
  - ALU → ALU 4;
  - ALU ↔ FMA/IMAD 5;
  - FMA → FMA 4;
  - any coupled op → a decoupled reader 6;
- predicates: ISETP → SEL 4, and ISETP → a guarded BRA 12;
- uniform registers: ULDC → LDG/STG 2;
- write-after-write: a coupled write followed by a decoupled write of the same register, 6.

Decoupled results take a scoreboard, which is waited on at the first reader or overwriter. A
forward look gives an op a read scoreboard when a source is overwritten before its result is
waited on, and gives a STG one when a branch follows it. A scoreboard is waited on no sooner
than 2 cycles after it is set. Every branch and EXIT waits on all scoreboards, and its stall
drains coupled writes (6), so no hazard crosses a block boundary. Yield stays clear (NAK's
default).

How it is checked:
- **Independent checker:** the CPU suite decodes the emitted SASS and re-derives every hazard
  with its own latency table (`_ck_sched` in `tests/tcyr/sm75_compile.tcyr`). Every
  control-flow and ranga kernel passes, and three injected faults are caught.
- **Pinned values:** a hand-built program pins each table value.
- **Hardware:** on the TU116 every lane check and golden still matches. The minimum gaps are
  exercised: of the 33 compiled kernels, 157 ALU→ALU pairs, 135 ALU→decoupled, 86 FMA→FMA and
  49 ISETP→guard sit at exactly their minimum.
- **ptxas comparison:** in the one captured kernel, ptxas agreed except for ALU → STG data,
  where it used 8 against the table's 6. 42 pairs sit at exactly 6 and pass on hardware.

Result: 4.5× fewer issue cycles per warp across the 33 kernels. On the TU116 that is 1.48×
faster for the arithmetic-bound `loop_lcg` and unchanged for ranga's memory-bound kernels
(`make bench-nvidia-compute`).

## 6. Encoding and the oracle

NVIDIA publishes no ISA document. The field layout comes from Mesa's NAK encoder
(`src/nouveau/compiler/nak/sm70_encode.rs`, Mesa `6aa7eaf09fc1`, MIT). The 128-bit word has:

- opcode in bits 0-8 and operand form in 9-11 (1 = R,R; 4 = R,imm32; 5 = R,c[]; 2 / 3 = third
  source imm / c[]);
- guard predicate in 12-14 (negate in 15), dst in 16-23, src0 in 24-31;
- src1 at 32-39 (register), 32-63 (immediate) or 32-58 (constant bank: offset at 38-53, bank at
  54-58), src2 in 64-71;
- per-op modifiers from bit 72 up.

NAK is a reference for facts, not copied code; each encoder cites the field it sets.

Every encoder is verified two ways on `chew` (CUDA 13.4 at `/opt/cuda/bin`):

1. **Golden bytes.** A CUDA or PTX snippet producing the same instruction is compiled with
   `nvcc -arch=sm_75 -cubin`. The instruction's opcode word, and its control word with the
   scheduling bits masked, are asserted in the CPU suite. That is the `backend_nvidia_sass.cyr`
   capture doctrine (ADR 007), one instruction at a time.
2. **Round trip.** Whole compiled programs go through `nvdisasm -b SM75`. `scripts/disasm-sass.sh`
   gains a file mode. A decode error or an unexpected mnemonic fails.

CUDA stays a build- and verify-time tool, as for the captured kernels. Nothing at runtime depends
on it.

## 7. Bites, in order

| Bite | What | Verified by |
|---|---|---|
| N9.1 | `sm75_encode.cyr`: instruction builder + the §3 encoders | golden bytes per encoder (CPU); `nvdisasm` round trip on chew |
| N9.2 | `sm75_isel.cyr`: MIR → records for the §1 op set, builtins, global memory | CPU unit tests on hand-built MIR |
| N9.3 | `sm75_regalloc` + conservative `sm75_sched` + `sm75_compile` driver; register-count rounding | CPU; whole-kernel `nvdisasm` round trip |
| N9.4 | backend wiring: SPIR-V `shader_module_create` on NVIDIA, QMD LocalSize + register count + binding count; first compiled kernel (SAXPY) on the TU116 | `test-nvidia-spirv-*` program on chew |
| N9.5 | branches, loops, phis, int-compare select, the udiv macro; the 24 control-flow kernels on the TU116 | `test-nvidia-spirv-compute-e2e` (N9.4's gate, now checking every kernel), every lane |
| N9.6 | ranga's 11 kernels through the public API on the TU116; then ranga's four GPU suites with an NVIDIA context | the done-when: `test-nvidia-ranga-kernels-e2e` (goldens from an independent SPIR-V interpreter) and ranga's suites in a scratch clone |

Status (2026-11-16): N9.1–N9.6 have landed and the done-when is met. On the TU116:
- 22 of the 24 control-flow kernels match every lane. `loop_phi_f64` is refused as
  `SM75_ERR_UNSUPPORTED_OP` (the f64 item below), and `oob_label` as `MIR_ERR_ID_OOR`.
- ranga's 11 kernels match the generator's oracle (`tests/spirv/ranga/`).
- ranga's four GPU suites pass with an NVIDIA context: `gpu` 48, `gpu_kernels` 53,
  `gpu_pipeline` 102, `gpu_spirv` 54.

N9.6 found one bug: SM75 counted only the bindings a kernel reads, where GFX9 counts the
bindings it declares. It now counts declared bindings too (CHANGELOG [4.4.0]).

Named for after the done-when. Since N9 closed they are tracked in the roadmap's 4.4.0 section,
"N9 follow-ons". The latency-table scheduling is done (§5 v2); the rest are open:
`BSSY`/`BSYNC` reconvergence; tightened scheduling from the SM75 latency tables; predicate
allocation in place of P0 re-emission; a target-aware lowering for `OpFDiv` (`MUFU.RCP` + Newton
on SM75) and f64.

## 8. Risks

- **Wrong control bits fail silently** (garbage or a hang). Mitigated by the all-conservative v1
  schedule and by the lane-checked hardware gates. A hung kernel on `chew` is a channel kill
  (`GPU_ERR_DEVICE_LOST`), recoverable without a reboot (4.1.4).
- **No official encoding spec.** NAK plus per-encoder golden bytes from ptxas plus the `nvdisasm`
  round trip: three independent checks.
- **ranga's own NVIDIA guard** (ranga ADR 005) refuses compute on NVIDIA contexts. Running ranga's
  suites (N9.6) needs that guard lifted in ranga's repository. That is the maintainer's call and
  outside mabda. N9.6 ran the suites in a scratch clone with the guard lifted; ranga's
  repository is unchanged, and the changes it needs are listed in the roadmap's consumer check.

## 9. Not changing

The public API. `gpu_shader_module_create_spirv` already exists and already routes by context;
today it refuses SPIR-V on NVIDIA with `SHMOD_ERR_SRC_KIND`. Pre-captured SASS keeps working
through `SHADER_SRC_SASS`. No launcher slot is involved.
