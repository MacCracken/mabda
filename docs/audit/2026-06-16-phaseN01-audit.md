# Security/Correctness Audit — v3.2.5 (native SPIR-V→GFX9 compiler N.0 + N.1), 2026-06-16

**Scope:** the v3.2.5 diff vs the `3.2.4` tag — the compiler foundation:
`src/gfx9_encode.cyr` (operand-parameterized GFX9 encoders + emit helpers),
the N.0 follow-on refactor of `src/backend_native_shaders.cyr` (the six
hand-authored builders re-expressed through the encoders), and
`src/spirv_parse.cyr` (the SPIR-V parser: validate gate + entry-point/LocalSize
probes + type/constant/decoration lookup tables). Plus the test oracle
(`tests/tcyr/compiler.tcyr`, 169 asserts) and the build-chain/manifest wiring.

**Threat model.** Two distinct risk surfaces:
1. **Generated/encoded bytes (N.0).** Wrong ISA bytes → silent GPU
   miscompute/hang. Mitigation = differential oracle: every encoder reproduces
   the hand-authored, llvm-mc-verified dwords byte-for-byte, and the refactored
   builders are pinned by the pre-existing per-dword / checksum golden tests in
   `native.tcyr` (the suite is the proof the bytes did not move).
2. **Untrusted input (N.1).** A compiler ingests fully consumer-controlled bytes
   — the classic parser attack surface (OOB read, integer overflow on
   sizes/counts, non-terminating walks).

**Method:** read every new fn against the SPIR-V spec (§2.3 physical layout,
§3.37 opcodes) and the GFX9 ISA bit-layouts; traced each parser read for
bounds-safety; enumerated the rejection cases and confirmed each has a test.

**Result:** **0 CRITICAL / 0 HIGH / 0 MEDIUM / 0 confirmed findings.**

## Untrusted-input review (the parser)
- **Header.** Reuses the Phase S `_spirv_validate` (byte_len ≥ 20, 4-aligned,
  magic, byte-swap detect, id-bound ∈ (0, 0x40000000), word-cap). ✔
- **Non-termination.** The instruction walk advances by the declared word count;
  a zero word count is rejected (`SPIRV_PARSE_ERR_ZERO_WC`) before it can stall
  the loop. Every walk loop is bounded by `off < total`. ✔
- **Out-of-bounds reads.** `spirv_validate_stream` rejects any instruction whose
  words exceed the stream (`SPIRV_PARSE_ERR_TRUNC`); the probe/table fns are
  documented to run only on a validated stream AND additionally guard each
  multi-word operand read by the instruction's word count (`wc >= N`), so a
  read never crosses the instruction boundary even on a validated-but-odd
  stream. The table builders also bail `-1` on a zero-wc instruction
  (defense-in-depth). ✔
- **Index safety.** Tables are indexed by SPIR-V `<id>`; the caller sizes each
  buffer to `id_bound * REC` and `_spirv_validate` guarantees the header
  id-bound is sane, so `id < id_bound` write/read offsets stay in the buffer
  (SPIR-V requires every `<id> < id_bound`; a stream violating that is rejected
  upstream by the bound check, and the lowering will re-validate id ranges when
  it consumes them). ✔
- **Integer overflow.** All offsets are word-index * 4 / id * record-size on
  `id_bound`-bounded values; no unchecked size multiply on consumer-supplied
  64-bit quantities. ✔

## Generated-bytes review (the encoders)
- Every encoder field is masked to its bit width before shifting (defensive
  against a bad operand corrupting an adjacent field). ✔
- The ~90-dword regression oracle + per-form llvm-mc round-trip + the
  byte-identical builder refactor (golden/checksum tests unchanged-green)
  establish correctness against trusted ground truth. ✔

## Notes / scope boundaries (not findings)
- N.1 builds the tables but does not yet semantically validate type/id
  *references* (e.g. a pointer's pointee id pointing at a non-type) — that is
  N.2's lowering-time concern and is called out there. The parser's job is
  structural safety, which is complete.
- EXP/MIMG encoders remain unlifted (compute-only compiler scope) — unchanged
  raw dwords, still covered by existing byte-pinned tests.

## Verification
- CPU suite **3077/0** across 13 domain files (compiler oracle 169 asserts incl.
  5 parser rejection cases); smoke build, `cyrius lint`/`fmt` clean, `distlib`
  idempotent, `version-check` consistent. No HW gate (pure CPU phase).
