# Security/Correctness Audit — v3.3.0 (asset loading, Phase AL), 2026-06-19

**Scope:** the v3.3 asset-loading parsers, which consume **untrusted input**
(attacker-controlled byte buffers + claimed length) — `src/asset_load.cyr`
(`_dds_parse`, `_ktx2_parse`, `gpu_texture_load_{dds,ktx2,png,png_mipped}` +
`_result`, the magic-byte sniffer `gpu_texture_load`, the caps gate
`gpu_ctx_supports_format`) and `src/asset_format.cyr` (the VkFormat/DXGI →
`MABDA_TEXFMT_*` mapping). The chitra PNG decoder itself is audited in the
chitra package; this audit covers mabda's parse/validate/upload glue.

**Threat model:** the attacker fully controls the buffer contents AND the claimed
`len`. Cyrius `load32`/`load64` are raw, unchecked memory reads; `var buf[N]` is
N bytes; i64 add/mul/shift can overflow. Discipline: every read off `bytes` is
bounds-checked against `len` before the read; size/dimension/offset math must not
overflow into a check; the format-map sentinel is -1 (not 0, which is RGBA8).

**Method:** one read-only adversarial security review (Explore agent, no writes)
over both files, hunting OOB reads, integer overflow defeating a subsequent
bounds check, unvalidated dimensions, sign-extension mis-validation, and
divide-by-zero — with each "safe" path required to cite its guarding line, not
just the positives.

**Result:** **1 CRITICAL → fixed + regression-tested**, all other paths verified
sound. Fixed before the 3.3.0 cut.

---

## CRITICAL-1 (fixed) — KTX2 level-index `byteOffset` signed-overflow OOB read

`gpu_texture_load_ktx2_result`, the level-index walk (`asset_load.cyr`). `boff`
and `blen` are read as raw i64 from the attacker-controlled level index. The
truncation guard was:

```
if (boff + blen > len) { return GPU_ERR_CONTAINER_PARSE; }   # truncated
```

**Attack:** a KTX2 with a valid header (`vkFormat`, `levelCount=1`, dims) and a
level-index entry `byteOffset = 0x7FFFFFFFFFFFFFFF` (i64_max),
`byteLength = uncompressedByteLength = <the exact expected level size>`. The
preceding guards pass — `blen == expected`, `ulen == blen`, and `boff >=
end-of-index` (i64_max ≥ 104). Then `boff + blen` **overflows i64 to a large
negative value**, so `negative > len` is false and the guard silently passes.
`gpu_texture_write_level(ctx, tex, level, bytes + i64_max, blen)` then reads the
level bytes from `bytes + i64_max` — a wild out-of-bounds read (crash or info
leak), with attacker control over the offset.

**Fix:** check `boff` without ever forming `boff + blen`. At that point `blen ==
expected` (> 0, dimension-bounded via `mabda_texfmt_data_size`), so `len - blen`
is a safe subtraction:

```
if (boff > len - blen) { return GPU_ERR_CONTAINER_PARSE; }   # truncated / OOB
```

If `blen > len` the right side goes negative and any in-range `boff` (the prior
guard already requires `boff >= 80 + levels*24`, which also rejects negative
`boff`) is rejected. **Regression test** (`asset_load.tcyr`,
`test_ktx2_load_orchestration`): the exact i64_max attack → `gpu_texture_load_ktx2
== 0` and `write_level` is never reached.

---

## Verified sound (guards cited)

- **DDS parser / loader.** Magic + `len >= 128` gate all header reads; DX10
  header read only when `len >= 148`; `width/height <= 0` rejected; the per-level
  data walk uses an **internally-computed** `off` (starts 128/148, += per-level
  `n`), both `off` and `n` dimension-bounded, with `off + n > len` rejecting
  truncation — no attacker-direct value enters the offset, so no overflow path
  (unlike KTX2's index-supplied `boff`).
- **KTX2 parser (`_ktx2_parse`).** Identifier read guarded by `len >= 80`; the
  12-byte identifier compared as three **masked** u32s (`& 0xFFFFFFFF`) so the
  high-bit byte can't sign-extend into a mis-validate; `supercompressionScheme
  != 0` / `faceCount != 1` / `layerCount > 1` fail loud; the level index is
  bounds-checked (`len >= 80 + levels*24`) before any index read.
- **PNG loaders.** Dimensions from chitra validated `> 0`; `create_2d_sampleable`
  / `_rgba8_mipped` enforce `dim <= MABDA_MAX_TEXTURE_DIM_2D` (8192), so
  `w*h*4 <= 256 MiB` — no i64 overflow; decode failure → `GPU_ERR_IMAGE_DECODE`.
- **Format mapping.** Unmapped VkFormat/DXGI → **-1** (checked before use; 0 is
  the valid RGBA8 id); SNORM/SFLOAT/BC2/ETC2-R8G8B8A1 → -1 (fail loud);
  sRGB collapses to UNORM but stays queryable (`is_srgb`), never silent.
- **Sniffer.** `len < 4` guards the magic `load32`; masked compare; unknown
  magic → `GPU_ERR_CONTAINER_PARSE`.
- **`mabda_texfmt_data_size` with attacker dims** returns 0 for oversized
  (> 8192) dimensions, which every caller rejects before using it as a size.

No divide/modulo on an attacker-zeroable value was found in the audited paths.
