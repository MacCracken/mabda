# 001 — GpuContext uses accessor functions, not opaque handles

## Status: Accepted (updated for Cyrius port)

## Context

In the Rust version, `GpuContext` exposed `pub instance`, `pub adapter`, `pub device`, `pub queue` as raw wgpu types. In the Cyrius port, all handles are opaque i64 pointers stored in a heap-allocated struct.

## Decision

Provide accessor functions (`gpu_ctx_device(ctx)`, `gpu_ctx_queue(ctx)`, etc.) that return the raw handles. Consumers can call wgpu FFI directly with these handles.

**Rationale:**
- Cyrius has no struct field access syntax for opaque types — accessor functions are the standard pattern
- Zero-cost: `gpu_ctx_device(ctx)` compiles to `load64(ctx + 16)` — one instruction
- Consumers still get direct access to wgpu handles for advanced use
- Consistent with all other Cyrius struct patterns (DeviceInfo, Color, etc.)

## Consequences

- Same as Rust version: consumers can bypass mabda and call wgpu directly
- Multi-queue support would add new accessor functions (additive, not breaking)
- The struct layout (32 bytes: instance/adapter/device/queue at fixed offsets) is part of the ABI
