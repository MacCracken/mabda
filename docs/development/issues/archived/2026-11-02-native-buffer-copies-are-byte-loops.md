# Native `gpu_buffer_write` / `gpu_buffer_read` copy one byte at a time: about 30 ms per 1080p frame round trip — RESOLVED

**Status:** ✅ **RESOLVED in mabda 4.3.0** (2026-11-16), options 1 and 2. (1) `gpu_copy_bytes` (`src/buffer.cyr`): 32 bytes per iteration when the pointers share 8-byte alignment (4-byte or bytewise otherwise, safe on write-combined BO memory); native AMD and NVIDIA buffer write / read and the wgpu buffer and texture readbacks use it. `cyrius bench tests/bcyr/mabda.bcyr` on the Cezanne: 128 KB in 26 µs (stdlib `memcpy` 208 µs, 7.9x); a 1080p frame in 1.72 ms (12.9 ms, 7.5x) — ranga's 16.6 MB round trip from ~30 ms to ~4 ms of copying. (2) `gpu_buffer_map(ctx, buf)` returns a native buffer's CPU mapping for zero-copy writes and reads (0 on wgpu, with the reason); proven on the TU116 (`nvidia_multibind_e2e` case 5). Option 3, the stdlib `memcpy`, stays open in cyrius. Was: open.
**Filed:** 2026-11-02, from ranga 2.3.1's GPU benchmarks (`tests/gpu.bcyr`).
**Placement:** `src/backend_native.cyr` — `_backend_native_buffer_write` / `_read` (line ~910 / ~927);
`src/backend_nvidia.cyr` — `_backend_nvidia_buffer_write` / `_read` (line ~317 / ~333). The root cause is
the stdlib's `memcpy`.
**Severity:** **Medium.** Every native GPU operation on a full frame is dominated by the copies, not the
kernel.
**Affects:** mabda 4.2.0 on cyrius 6.7.18, both native backends. Measured on AMD Cezanne (Radeon Vega
iGPU).

## Summary

Both native backends move data with one call to the stdlib `memcpy`, and at cyrius 6.7.18 that `memcpy` is
a byte loop (`store8(dst + i, load8(src + i))`; the x86 `rep movsb` path was removed for aarch64). On
Cezanne this copies about **530 MB/s**.

ranga's 1080p operations upload a frame, dispatch and read it back. Every one of them costs the same,
whatever the kernel does:

| ranga operation (1080p) | Bytes copied | Native AMD (mabda) | wgpu on the same iGPU (Rust ranga) |
| --- | ---: | ---: | ---: |
| invert / grayscale / fade / saturation | 16.6 MB | 30.1–30.5 ms | 2.6–3.0 ms |
| dissolve, flip (two uploads) | 24.9 MB | 45.8–46.5 ms | 4.5–5.6 ms |

So the native path is about 10× slower than wgpu here, and almost all of it is the two copies.

## Suggested fix (any of these)

1. **Copy wider inside mabda:** an 8-byte `load64`/`store64` loop with a byte tail, or an
   architecture-guarded `rep movsb` on x86_64. This would be a local helper, so mabda doesn't wait on the
   stdlib.
2. **Let consumers write in place.** Native buffers are host-visible GTT BOs that already carry their
   mapped address (`NATIVE_BUF_FIELD_ADDR`, `NV_BUF_ADDR`). A public `gpu_buffer_map(ctx, buf)` returning
   that pointer would let a consumer fill or read the buffer directly, with zero copies.
3. **Upstream:** a fast `memcpy`/`memset` in the cyrius stdlib. Filed as
   `cyrius/docs/development/issues/2026-11-02-memcpy-memset-byte-loops-dominate-frame-copies.md`, which
   measured 8-byte chunks at 6.5× (copy) and 6.9× (fill) on a frame. It is the same root cause behind ranga's
   CPU-side crop and buffer-pool numbers. If it lands, option 1 comes for free.

## Measure

`tests/gpu.bcyr` in ranga (`cyrius bench tests/gpu.bcyr`) prints the per-operation times. A fix should
bring the one-buffer rows from about 30 ms toward the kernel time.
