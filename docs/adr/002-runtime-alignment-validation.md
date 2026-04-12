# 002 — Uniform buffers use runtime alignment validation

## Status: Accepted (updated for Cyrius port)

## Context

WebGPU requires uniform buffer offsets to be 16-byte aligned (`minUniformBufferOffsetAlignment`). The Rust version chose runtime validation over the `encase` crate. In Cyrius, there is no type system to enforce alignment at compile time.

## Decision

Validate uniform buffer sizes at runtime with a simple check: `if (size & 15 != 0) { return Err(...); }`. No external libraries.

**Rationale:**
- Cyrius has no type system — all values are i64, no compile-time alignment checks possible
- Runtime check is one instruction (bitwise AND)
- Zero misalignments observed in soorat migration (all uniform structs naturally align to 16 bytes)
- No equivalent of Rust's `encase` in Cyrius — would add unnecessary complexity

## Consequences

- Misaligned uniform buffers fail at `uniform_buffer_new()` with `GPU_ERR_BUFFER`
- Consumers must ensure their data sizes are multiples of 16 bytes
- If a consumer hits this, the fix is padding their struct — trivial in Cyrius (`alloc(size_rounded_up)`)
