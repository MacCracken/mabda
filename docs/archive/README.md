# Archive — resolved issues, shipped proposals

Historical record. Nothing here is actionable; the fixes and features
documented below all shipped. Kept for post-mortem value, not as
a live workqueue. Live items live under `docs/issues/` and
`docs/proposals/` at the same level as this directory.

## Index

### Resolved issues

| Date       | Title                                                                                    | Resolved in |
|------------|------------------------------------------------------------------------------------------|-------------|
| 2026-04-19 | [`fncall6` + wgpu crash — resolution](issues/2026-04-19-fncall6-wgpu-crash-resolution.md) | reclassified in v2.4.2 (SysV / AAPCS64 struct-by-value handshake, not a cyrius bug) |
| 2026-04-19 | [phase0 build broken](issues/2026-04-19-phase0-build-broken.md)                          | cyrius 5.4.9 (upstream `_cyrius_init` fix) + mabda v2.4.0 (missing `lib/str.cyr` include) |

### Shipped proposals

Design/scoping docs, archived once their feature shipped. `CHANGELOG.md` is the
authoritative record of what landed in each release; these are the original
analysis, kept for post-mortem context. Newest arc first.

| Arc / Date | Title                                                                                       | Shipped in |
|------------|--------------------------------------------------------------------------------------------|------------|
| v3.2 (2026-06-15) | [Render-graph multi-queue](proposals/v3.2-render-graph-multiqueue.md)                | v3.2.13–v3.2.14 (Phase R) |
| v3.2 (2026-06-15) | [f64 compute](proposals/v3.2-f64-compute.md)                                         | v3.2.12 (Phase F) |
| v3.2 (2026-06-15) | [SPIR-V → GFX9 native compiler](proposals/v3.2-spirv-gfx9-native-lowering.md)        | v3.2.5–v3.2.11 (Phase N) |
| v3.2 (2026-06-15) | [Native compressed sampling](proposals/v3.2-native-compressed-sampling.md)           | v3.2.2–v3.2.3 (Phase TS) |
| v3.2 (2026-06-15) | [SPIR-V ingestion (wgpu)](proposals/v3.2-spirv-ingestion-wgpu.md)                     | v3.2.4 (Phase S) |
| v3.2 (2026-06-15) | [TRANSFER + buffer-copy](proposals/v3.2-transfer-copy.md)                             | v3.2.1 (Phase X) |
| v3.2 (2026-06-15) | [Compressed textures (storage)](proposals/v3.2-compressed-textures.md)               | v3.2.0 (Phase T) |
| v3.1 (2026-06-15) | [Multi-queue coordination](proposals/v3.1-multiqueue.md)                              | v3.1.1 |
| v3.1 (2026-06-15) | [Mipmap generation](proposals/v3.1-mipmap-generation.md)                              | v3.1.0 |
| v3.0 (2026-04)    | [Backend interface](proposals/v3-backend-interface.md)                                | v3.0 (extended through v3.2.x) |
| v3.0 (2026-04)    | [Native API principles](proposals/v3-native-api-principles.md)                        | v3.0 onward |
| v3.0 (2026-04)    | [Native render design](proposals/v3-native-render-design.md)                          | v3.0 (Steps 6.x) |
| v3.0 (2026-04)    | [Shader-bytes capture](proposals/v3-shader-bytes-capture.md)                          | v3.0 (Steps 6.2–6.5); Phase N compiler in v3.2.x |
| v3.0 (2026-04)    | [Surface API design](proposals/v3-surface-api-design.md)                             | v3.0 (Step 7.7) |
| pre-v3 (2026-04)  | [Cyrius 5.6.x optimization requests](proposals/cyrius-5.6x-optimization-requests.md) | obsolete — toolchain now 6.2.22, well past the 5.6.x window |
| 2026-04-19 | [Render-pass FFI expansion](proposals/2026-04-19-render-pass-ffi.md)                 | v2.4.3 (7 FFI slots, 2 struct-packing shims, `programs/render_e2e.cyr`) |

### Shipped punchlists

Per-arc release punch lists, archived once the arc shipped. Each is the
original scoping + tick-list, preserved for post-mortem; the authoritative
record of what shipped is `CHANGELOG.md`.

| Arc   | Punchlist                                                       | Shipped |
|-------|----------------------------------------------------------------|---------|
| v3.0  | [`3-0-punchlist.md`](punchlists/3-0-punchlist.md)              | 3.0.0 GA 2026-06-02 |
| v3.0  | [`3-0-rc-2-punchlist.md`](punchlists/3-0-rc-2-punchlist.md)    | 3.0.0-rc.2 |
| v3.0  | [`3-0-rc-3-punchlist.md`](punchlists/3-0-rc-3-punchlist.md)    | 3.0.0-rc.3 |
| v3.0  | [`3-0-rc-4-punchlist.md`](punchlists/3-0-rc-4-punchlist.md)    | 3.0.0-rc.4 (24 h soak) |
| v3.1  | [`3-1-punchlist.md`](punchlists/3-1-punchlist.md)             | 3.1.0 (mipmaps) + 3.1.1 (multi-queue), 2026-06-15 |
| v3.2  | [`3-2-punchlist.md`](punchlists/3-2-punchlist.md)             | 3.2.0 → 3.2.14 (texture & shader breadth: compressed textures, native sampling, SPIR-V + native SPIR-V→GFX9 compiler, f64, render-graph multi-queue), 2026-06-15 → 2026-06-19 |

> v3.1's punchlist deferred items to **3.1.2** (TRANSFER→SDMA ring flip +
> public buffer-copy API; render-graph multi-queue scheduling) — these all
> SHIPPED inside the v3.2.x arc (Phase X in 3.2.1, Phase R in 3.2.13–3.2.14),
> see the 3-2 punchlist. The whole v3.2.x arc is complete; `CHANGELOG.md` is
> the authoritative record.

Cross-reference the `CHANGELOG.md` entries for the release each item
shipped in; this directory is just the original analysis / scoping
document, preserved for context.
