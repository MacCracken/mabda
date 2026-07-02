# Archive — resolved issues, shipped proposals

Historical record. Nothing here is actionable; the fixes and features
documented below all shipped. Kept for post-mortem value, not as
a live workqueue. Live items live under `docs/development/issues/` and
`docs/proposals/` at the same level as this directory.

## Index

### Resolved issues

| Date       | Title                                                                                    | Resolved in |
|------------|------------------------------------------------------------------------------------------|-------------|
| 2026-04-19 | [`fncall6` + wgpu crash — resolution](issues/2026-04-19-fncall6-wgpu-crash-resolution.md) | reclassified in v2.4.2 (SysV / AAPCS64 struct-by-value handshake, not a cyrius bug) |
| 2026-04-19 | [phase0 build broken](issues/2026-04-19-phase0-build-broken.md)                          | cyrius 5.4.9 (upstream `_cyrius_init` fix) + mabda v2.4.0 (missing `lib/str.cyr` include) |
| 2026-04-21 | [GFX9 store blocker (native AMD B.4)](issues/2026-04-21-gfx9-store-blocker.md)            | v3.0 native AMD bring-up — compute store proven (session25 B.4), 3.0.0 GA 2026-06-02 |
| 2026-05-13 | [Native render Cezanne TDR](issues/2026-05-13-native-render-cezanne-tdr.md)              | Resolved 2026-05-13 (native render PM4 cache-flush sequencing) |
| 2026-06-01 | [Soak stale-binary](issues/2026-06-01-soak-stale-binary.md)                              | Resolved — `soak.sh` rebuilds on staleness + monitor-death guard |

### Shipped proposals

Design/scoping docs, archived once their feature shipped. `CHANGELOG.md` is the
authoritative record of what landed in each release; these are the original
analysis, kept for post-mortem context. Newest arc first.

| Arc / Date | Title                                                                                       | Shipped in |
|------------|--------------------------------------------------------------------------------------------|------------|
| v3.4 (2026-06-19) | [Array textures + cubemaps](proposals/v3.4-array-cube-textures.md)                   | v3.4.0–v3.4.1 (Phase AA) |
| v3.3 (2026-06-19) | [Asset loading](proposals/v3.3-asset-loading.md)                                     | v3.3.0 (Phase AL) |
| v3.3 (2026-06-19) | [chitra PNG decoder package](proposals/v3.3-chitra-png-decoder-package.md)            | v3.3.0 (chitra 0.1.0) |
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
| v3.3  | [`3-3-punchlist.md`](punchlists/3-3-punchlist.md)             | 3.3.0 (asset loading — DDS/KTX2 in-tree + PNG via chitra + sniffer), 2026-06-19 |
| v3.4  | [`3-4-punchlist.md`](punchlists/3-4-punchlist.md)             | 3.4.0 (array textures + cubemaps) + 3.4.1 (BC tiled arrays, wgpu draw-time layer, MABDA_F64_* fix), 2026-06-19 |
| v4.0  | [`4-0-punchlist.md`](punchlists/4-0-punchlist.md)             | 4.0.0 (NVIDIA native backend — nouveau, Turing/SM75) + 4.0.1 (AMD wgpu deprecation), 2026-06-28 → 2026-07-02 |

> v3.1's punchlist deferred items to **3.1.2** (TRANSFER→SDMA ring flip +
> public buffer-copy API; render-graph multi-queue scheduling) — these all
> SHIPPED inside the v3.2.x arc (Phase X in 3.2.1, Phase R in 3.2.13–3.2.14),
> see the 3-2 punchlist. The whole v3.2.x arc is complete; `CHANGELOG.md` is
> the authoritative record.

Cross-reference the `CHANGELOG.md` entries for the release each item
shipped in; this directory is just the original analysis / scoping
document, preserved for context.

### Session handoffs & bring-up notes

Per-session debugging handoffs and vendor bring-up capture-notes from
completed arcs, kept for post-mortem value. `CHANGELOG.md` and the ADRs are
the authoritative record.

- `handoff/2026-04-*.md` — the v3.0 native-AMD GFX9 bring-up saga
  (session9 → session26: the GEM/PM4/CS/CPC-fault investigation ending in the
  B.4 compute-store proof and 3.0.0 GA).
- `handoff/soak-2026-05*/`, `handoff/soak-2026-06-0[13]*/` — the v3.x AMD
  soak-burn-in runs (compute/render/wgpu). The v4.0 NVIDIA soaks live in the
  live `docs/handoff/` alongside the current GA evidence.
- `handoff/2026-06-19-handoff.md` — stale v3.4-era "start here" pointer.
- `development/nvidia-n4/n6/n7-capture-notes.md` — decoded NVK Vulkan captures
  (compute / texture-sampling / triangle-draw) that seeded the v4.0 NVIDIA
  backend; `development/nvidia-bringup-hardware.md` — the TU116 HW ladder.
