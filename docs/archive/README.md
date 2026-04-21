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

Cross-reference the `CHANGELOG.md` entries for the release each item
shipped in; this directory is just the original analysis / scoping
document, preserved for context.
