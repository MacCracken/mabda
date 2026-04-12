# 003 — Fixed vertex types with manual layout, no codegen

## Status: Accepted (updated for Cyrius port)

## Context

Mabda provides three canonical vertex types: Vertex2D (32B), Vertex3D (48B), SkinnedVertex3D (96B). The Rust version used a `VertexLayout` trait with manual implementations. The question was whether to add a derive macro for custom vertex types.

## Decision

Keep fixed vertex types with manual `vertex2d_write()` / `vertex3d_new()` functions and explicit attribute layout builders (`vertex2d_attributes()`, `vertex3d_attributes()`). No codegen for custom types.

**Rationale:**
- Cyrius has no traits, generics, or derive macros — custom vertex types are just `alloc(N)` + `store32` at known offsets
- Three types cover all current consumer needs (soorat uses Vertex2D and Vertex3D)
- Adding codegen for custom vertex types would require compiler changes (`#derive` for vertex layouts) with no current demand
- Manual layout is explicit and auditable — each field's byte offset and shader location is documented

**Cyrius vertex pattern:**
```cyrius
# Vertex2D: 32 bytes
# +0: position (2 x f32)   location 0
# +8: tex_coords (2 x f32) location 1
# +16: color (4 x f32)     location 2
fn vertex2d_write(dst, px, py, tx, ty, cr, cg, cb, ca) {
    store32(dst, f64_to_f32(px));
    store32(dst + 4, f64_to_f32(py));
    # ...
}
```

## Consequences

- Custom vertex types require manual `store32` at byte offsets — simple but verbose
- Attribute descriptors must be built manually (24 bytes per attribute)
- If demand arises, a `#derive(VertexLayout)` could be added to Cyrius — demand-gated
