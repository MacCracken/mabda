# Mabda — Claude Code Instructions

## Project Identity

**Mabda** (مبدأ — Arabic: origin, principle, starting point) — GPU foundation layer for AGNOS (device lifecycle, buffers, compute, textures, profiling, capability detection)

- **Type**: Library (Cyrius)
- **License**: GPL-3.0-only
- **Version**: 2.0.0
- **Language**: Cyrius 3.4.14+
- **GPU FFI**: wgpu-native v29 C API

## Consumers

soorat (renderer), rasa (image editor), ranga (image processing), bijli (EM simulation), aethersafta (desktop compositor), kiran (game engine, via soorat)

**Depends on**: wgpu-native (GPU abstraction, C API), sakshi (logging)

**Modules**: error, color, capabilities, context, buffer, compute, texture, render_target, render_pipeline, render_pass, depth, vertex, sampler, surface, blend, bind_group, instancing, profiler, shader_cache, pipeline_cache, bind_group_cache, debug, resource, wgpu_types, wgpu_descriptors, wgpu_ffi

## Development Process

### Work Loop (continuous)

1. Work phase — new features, roadmap items, bug fixes
2. Cleanliness check: run all test suites
3. Test + benchmark additions for new code
4. Internal review — performance, memory, correctness
5. Deeper tests from audit observations
6. Documentation — update CHANGELOG, roadmap, docs
7. Version check — VERSION, cyrius.toml in sync
8. Return to step 1

### Task Sizing

- **Low/Medium effort**: Batch freely
- **Large effort**: Small bites — break into sub-tasks, verify each
- **If unsure**: Treat as large

### Key Principles

- **Tests are the way.** 92+ tests, all passing.
- **Own the stack.** If AGNOS wraps an external lib, depend on the AGNOS crate.
- **No magic.** Every operation is measurable, auditable, traceable.
- **Manual memory.** `alloc/store64/load64` — document struct layouts with byte offsets.
- **Tagged unions for errors.** `Ok(value)` / `Err(gpu_err(...))` via tagged.cyr.
- **f64 internally, f32 at GPU boundary.** Use `f64_to_f32()` when writing to GPU buffers.
- **Document struct layouts.** Every struct gets a comment block with field offsets.
- **Prefix private helpers with `_`.** Public API uses descriptive names.

## FFI Architecture

GPU programs use a C launcher (`deps/wgpu_main.c`):
1. C `main()` calls `_cyrius_init()` then `alloc_init()`
2. C pre-initializes GPU (instance/adapter/device/queue)
3. C builds function table (40 wgpu function pointers)
4. C calls `mabda_main(fn_table_ptr, preinit_ptr)`
5. Cyrius calls wgpu via `fncall2(_fp(N), arg1, arg2)`

For standalone tests (no GPU): use `cyrius test tests/test_foo.tcyr` directly.

## DO NOT

- **Do not commit or push** — the user handles all git operations
- **NEVER use `gh` CLI** — use `curl` to GitHub API only
- Do not add unnecessary dependencies
- Do not skip tests before claiming changes work
- Do not commit `cyr/build/`, `cyr/deps/wgpu-native/`, or `cyr/deps/*.o`

## Documentation Structure

```
Root files:
  README.md, CHANGELOG.md, CLAUDE.md, CONTRIBUTING.md,
  SECURITY.md, CODE_OF_CONDUCT.md, LICENSE, VERSION

docs/:
  architecture/overview.md   — module map, FFI architecture, data flow
  development/roadmap.md     — completed, pending, backlog
  guides/usage.md            — code examples for all APIs
  guides/integration.md      — consumer integration patterns
  adr/001-004                — architectural decision records
```

## CHANGELOG Format

```markdown
## [X.Y.Z] — YYYY-MM-DD
### Added — new features
### Changed — changes to existing features
### Fixed — bug fixes
### Breaking — breaking changes with migration guide
```
