# 001 — GpuContext exposes public fields, not accessor methods

## Status: Accepted

## Context

`GpuContext` exposes `pub instance`, `pub adapter`, `pub device`, `pub queue` as raw wgpu types. This lets consumers bypass mabda entirely and call wgpu directly. The alternative is accessor methods, which would let mabda control access and enable future features like resource tracking, newtype wrappers, or multi-queue support without breaking the API.

This decision was evaluated during the soorat migration (phases 0-3), which is mabda's first and most complex consumer integration.

## Decision

Keep public fields. Do not wrap with accessor methods before v1.

**Evidence from soorat migration:**
- Soorat accesses `ctx.device` and `ctx.queue` in many call sites (buffer creation, texture uploads, pipeline building, command submission).
- No call sites needed resource tracking or interception — soorat passes the raw device/queue directly to wgpu helpers and mabda's own APIs.
- The backlog items that would benefit from wrapping (multi-queue, GPU memory stats) are demand-gated and not needed by any current consumer.

**Rationale:**
- Public fields provide zero-cost access with no indirection.
- Wrapping these types adds API surface without current benefit.
- If multi-queue or resource tracking is needed later, a new `GpuDevice` wrapper type can be introduced alongside the existing fields (additive change) rather than replacing them (breaking change).
- The `instance` and `adapter` are rarely accessed after initialization — only `device` and `queue` are hot-path.

## Consequences

- Consumers can call wgpu directly, bypassing mabda's abstractions. This is a feature, not a bug — mabda is a foundation layer, not a sandbox.
- If multi-queue support is added, it will need a new API rather than modifying `GpuContext`. This is acceptable because multi-queue is a fundamentally different execution model.
- GPU memory statistics, if added, would be implemented via a separate `GpuStats` struct that observes allocations rather than intercepting field access.
