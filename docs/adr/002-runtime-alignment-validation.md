# 002 — UniformBuffer uses runtime alignment validation, not encase

## Status: Accepted

## Context

`UniformBuffer<T>` validates `size_of::<T>() % 16 == 0` at runtime during `new()`. This catches misaligned types but only at creation time, not compile time. The alternative is `encase`, which provides compile-time std140 layout enforcement via derive macros, but adds a dependency and changes the consumer API.

This decision was evaluated during the soorat migration (phases 0-3).

## Decision

Keep runtime validation. Do not add `encase` as a dependency.

**Evidence from soorat migration:**
- Soorat defines 4 uniform types (`CameraUniforms`, `LightUniforms`, `MaterialUniforms`, `TransformUniforms`). All are manually padded to 16-byte alignment with `_padding` fields.
- Zero alignment errors were encountered during migration — soorat's existing types were already correctly aligned.
- The runtime check in `UniformBuffer::new()` catches misalignment immediately with a clear error message, which is sufficient for the creation-time-only failure mode.

**Rationale:**
- `encase` adds ~15 transitive dependencies and requires all uniform types to derive `ShaderType` instead of `bytemuck::Pod`. This is a significant API change for all consumers.
- The runtime check fires exactly once per buffer creation, not per frame. The cost is negligible.
- Manual padding with `bytemuck::Pod` is idiomatic in the wgpu ecosystem. Consumers already understand `repr(C)` layout rules.
- If a consumer hits alignment issues, the error message tells them exactly what's wrong: "size (N bytes) must be a multiple of 16".

## Consequences

- Consumers must manually ensure 16-byte alignment for uniform types. This is standard practice in GPU programming.
- Alignment errors are caught at runtime (buffer creation), not compile time. In practice, this means they surface on first run, not in CI type-checking.
- If a future consumer has complex nested structs where manual padding is error-prone, `encase` can be added behind a feature gate (`encase` feature) without breaking existing `bytemuck`-based consumers. This is an additive change.
