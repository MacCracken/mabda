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

| Date       | Title                                                                                | Shipped in |
|------------|--------------------------------------------------------------------------------------|------------|
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

> v3.1's punchlist notes items deferred to **3.1.2** (TRANSFER→SDMA ring
> flip + public buffer-copy API; render-graph multi-queue scheduling) —
> still a live backlog, tracked in `docs/development/roadmap.md`.

Cross-reference the `CHANGELOG.md` entries for the release each item
shipped in; this directory is just the original analysis / scoping
document, preserved for context.
