# 003 — Fixed vertex types with VertexLayout trait, no derive macro

## Status: Accepted

## Context

Mabda provides three fixed vertex types (`Vertex2D`, `Vertex3D`, `SkinnedVertex3D`) covering 2D UI/sprites, 3D meshes, and skeletal animation. The `VertexLayout` trait exists for custom types, but consumers must hand-write `wgpu::VertexBufferLayout` with manual offset calculations.

The alternative is a derive macro (`#[derive(VertexLayout)]`) or a builder for vertex layout generation, which would reduce boilerplate for custom vertex types.

This decision was evaluated during the soorat migration (phases 0-3).

## Decision

Keep fixed vertex types and the manual `VertexLayout` trait. Do not add a derive macro.

**Evidence from soorat migration:**
- Soorat uses all three provided types: `Vertex2D` for UI/sprites, `Vertex3D` for meshes, `SkinnedVertex3D` for animated characters.
- Soorat defines zero custom vertex types — the three provided cover all its rendering needs.
- No other current consumer (rasa, ranga, bijli, aethersafta) has reported needing custom vertex formats.

**Rationale:**
- A derive macro is a proc-macro crate, adding compile-time cost and a new crate to maintain. This is unjustified without consumer demand.
- The three provided types cover the overwhelmingly common cases: 2D (32B), 3D (48B), and skinned 3D (96B).
- For consumers that do need custom formats, hand-writing `VertexBufferLayout` is a one-time cost per type. The offset calculations are straightforward with `std::mem::size_of` and `std::mem::offset_of`.
- The `VertexLayout` trait provides the generic interface needed for pipeline construction without forcing a specific generation strategy.

## Consequences

- Consumers needing custom vertex types must manually implement `VertexLayout`. This is acceptable for a foundation library — custom vertex formats are an advanced use case.
- If kiran (game engine) or a future consumer needs many custom vertex formats, a derive macro can be added as a separate `mabda-derive` crate without changing the core library. This is purely additive.
- The vertex type extensibility backlog item is moved to demand-gated status. It will be promoted if a consumer requests it.
