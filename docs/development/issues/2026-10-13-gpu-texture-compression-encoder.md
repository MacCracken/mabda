# GPU texture compression (BC1–BC7 / ASTC encode) belongs in mabda — roadmap request, not yet scheduled

**Status:** 📋 **OPEN — roadmap request** (backlog; to be scheduled when a consumer names the formats it
needs).
**Filed:** 2026-10-13, from sankoch, at the user's decision on sankoch's 2.8.6 roadmap item.
**Placement:** a new mabda module (texture encode), next to `src/texture_format.cyr`,
`src/asset_format.cyr` and `src/asset_load.cyr`.
**Severity:** none (feature). **Affects:** nothing shipped.

## The decision

sankoch's roadmap carried "2.8.6 — GPU texture compression (BC1–BC7 / ASTC)", whose first step was
deciding where it lives. The decision (2026-10-13): **in mabda.** GPU texture formats are lossy and
GPU-specific, and mabda already owns everything around them: the `MABDA_TEXFMT_BC*` / `ASTC_*` ids, the
VkFormat / DXGI mappings (`src/asset_format.cyr`), the DDS and KTX2 loaders (`src/asset_load.cyr`), the
per-backend compression capability masks, and compute dispatch (`src/compute.cyr`). sankoch stays the
home of *lossless* codecs and provides the lossless primitives a texture pipeline needs (below).
sankoch's side: its ADR 0004 and roadmap.

## What the work is (a starting shape, not a spec)

- **A CPU reference block encoder** for the first format a consumer needs: BC1 / BC3 / BC4 / BC5 /
  BC7 for desktop, or ASTC (4×4 / 6×6 / 8×8, the sizes `asset_format.cyr` already maps) for mobile.
  Block-based, deterministic.
- **Validation** against an independent decoder: a reference BCn / ASTC decoder run locally, and the
  round trip through mabda's own upload-and-sample path on hardware that samples the format (the
  native backend is BC-only on Cezanne; ETC2 / ASTC are HW-blocked there).
- **A quality bar**: PSNR (or similar) per format on a fixed image set, recorded so later speed work
  can't silently trade quality away.
- **Later, if it pays**: a GPU compute encoder through mabda's compute path, checked against the CPU
  reference.

## What sankoch already provides (no new sankoch work needed)

mabda's `[deps] stdlib` already lists `sankoch` (it arrives with chitra), and the stdlib's
`lib/sankoch.cyr` is the full bundle, so these are callable today:

1. **KTX2 supercompression decode.** `_ktx2_parse` rejects every `supercompressionScheme != 0` with
   `GPU_ERR_FORMAT_UNSUPPORTED` ("needs a decompressor mabda does not have", `src/asset_load.cyr`).
   Two of the three schemes are sankoch codecs:
   - scheme 2, **Zstandard**: `zstd_decompress` (each mip level is its own zstd stream; check the
     result against the level's `uncompressedByteLength`);
   - scheme 3, **ZLIB**: `zlib_decompress_capped(src, len, dst, cap, uncompressedByteLength)`, which
     fails closed at exactly the expected size.
   Scheme 1 (BasisLZ / ETC1S) is a separate transcoder and is not in sankoch.
2. **KTX2 supercompression encode**, for a texture-writing path: `zstd_compress_level` (1..9) or
   `zlib_compress`.
3. **One bundle per program**: take sankoch through the stdlib fold only. Adding a sankoch profile
   bundle (`sankoch-zstd.cyr` etc.) beside it would duplicate its runtime globals (sankoch
   `docs/architecture/003-per-profile-reset-dispatch.md`).

## What sankoch could add, on request

If a BCn encoder wants **rate-distortion optimization** (choosing, among near-equal block encodings,
the one that supercompresses best), it needs a cost model: roughly, "how many bits would this block
cost after LZ + entropy coding, given the bytes before it". sankoch has the parts (match finders,
Huffman / FSE coders, a Q16 integer log2) but no public cost API. File the request in sankoch's
`docs/development/issues/` with the shape mabda needs, and sankoch schedules it.

## References

- sankoch: `docs/adr/0004-gpu-texture-compression-in-mabda.md`; roadmap *Backlog* ("Primitives for
  mabda's texture work").
- Khronos KTX 2.0 specification, `supercompressionScheme`: 0 none, 1 BasisLZ, 2 Zstandard, 3 ZLIB;
  each mip level is supercompressed on its own, with `byteLength` and `uncompressedByteLength` in the
  level index.
