# ADR 005: Public API surface marking

**Status:** Accepted
**Date:** 2026-04-12
**Supersedes:** n/a
**Related:** ADR 004 (C launcher FFI — v3.x-era backend, retires v4.0), planned ADR 006 (native Cyrius GPU backend added alongside)

## Context

Mabda ships now via a C shim over wgpu-native so that AGNOS consumers
(soorat, rasa, ranga, bijli, aethersafta, kiran) have a stable GPU
surface to build against. In v3.0 a second backend — pure Cyrius
DRM/KMS — ships alongside the C path (see ADR 006). The C path stays
in-tree for the entire v3.x line as a measurement baseline and
migration runway; it retires in v4.0 once every consumer is running
native in production.

The risk: a consumer that reaches into the FFI modules —
`wgpu_types.cyr`, `wgpu_descriptors.cyr`, `wgpu_ffi.cyr` — couples
itself to wgpu-specific internals and breaks either when the native
backend replaces it (for that consumer) or when the C path itself
retires in v4.0. We need a mechanical way to express "this file is
load-bearing API; that file is backend-specific scaffolding."

## Decision

Every `.cyr` file in `src/` carries a **marker comment as line 1**:

- `# @public — stable API surface (unchanged across the v3.x dual-backend era and into v4.0)`
- `# @internal — backend-specific FFI / toolchain scaffolding (wgpu path retires v4.0)`

The distinction is load-bearing for two reasons:

1. **Human signal.** A consumer reviewing mabda sees at a glance which
   files are safe to depend on.
2. **Tooling signal.** `docs/stdlib-integration.md`, the consumer example
   under `examples/stdlib-consumer/`, and (future) lint checks can
   enforce the boundary by reading the first line of every file.

## Inventory (v2.5.0)

**Public (27 files):** `lib`, `error`, `color`, `capabilities`,
`profiler`, `resource`, `context`, `buffer`, `typed_buffer`,
`gpu_timestamps`, `compute`, `shader_cache`, `pipeline_cache`,
`bind_group_cache`, `vertex`, `blend`, `sampler`, `depth`, `bind_group`,
`texture`, `render_target`, `render_pipeline`, `render_pass`,
`render_graph`, `surface`, `instancing`, `debug`.

**Internal (3 files):** `wgpu_types`, `wgpu_descriptors`, `wgpu_ffi`.

The internal set is deliberately small — everything that a consumer
might reasonably need to call is `@public`. Only the raw FFI slots
(wgpu-specific enums, descriptor builders, function-pointer table)
stay internal.

Inventory history:
- **v2.1.1 (original):** 26 public + 5 internal. Internal set
  included `tagged_obj` (object-mode tagged-union bootstrap) and
  `cache_key` (decimal-string hashmap key helper).
- **v2.4.5:** `cache_key` retired — cyrius v5.5.20 shipped a
  u64-keyed hashmap variant that replaced the decimal-string key
  pattern across the four cache modules.
- **v2.5.0:** `render_graph` added as public (DAG pass
  orchestration). `tagged_obj` no longer tracked as internal —
  tagged-union usage is through the cyrius stdlib's `lib/tagged.cyr`
  rather than a mabda-side bootstrap.

## Consequences

**Positive:**

- The stability contract is machine-readable, not just documentation.
- Breaking changes can be scoped: any edit inside an `@public` file
  requires a minor version bump; edits inside `@internal` are free.
- The v3.0 native-backend addition becomes testable: every file that
  was `@public` in v2.1.1 must remain `@public` in v3.0, and every
  line in the v2.1.1 `examples/stdlib-consumer/main.cyr` must still
  compile under both backends. Same test fires again at v4.0 when
  the wgpu path retires.
- New consumer patterns have an obvious "don't cross this line" rule.

**Negative:**

- Line 1 of every file is now a machine-readable marker instead of the
  traditional module docstring. Slight aesthetic cost; the module
  docstring moves to line 2.
- Adding a new file means remembering to add the marker. Future lint
  step will enforce this — see Follow-up.

**Neutral:**

- The boundary between public and internal is subject to revision in
  future minor versions. If a consumer finds a legitimate need for
  something currently marked internal, we can re-mark it `@public`;
  that's a minor bump, not a breaking change.

## Alternatives considered

- **Separate `src/public/` and `src/internal/` directories.** Rejected
  because it would force a large file move and would complicate the
  `src/lib.cyr` include order (which has real forward-dependency
  concerns between the cache modules and the `cache_key` helper that
  was retired in v2.4.5).
- **Rust-style `pub(crate)` visibility.** Cyrius has no visibility
  modifiers. A marker comment is the cheapest signal we have.
- **No marking; document in `docs/`.** Rejected because docs drift
  and consumers won't read them. A per-file marker that every future
  author has to touch is the only thing that stays accurate.

## Follow-up

- `scripts/lint-markers.sh` (v2.1.2): fail if any `src/*.cyr` file is
  missing an `@public`/`@internal` marker on line 1. Wire into
  `make test-all` alongside `scripts/version-check.sh`.
- `docs/stdlib-integration.md` already cites this ADR and instructs
  consumers to treat `@internal` as "do not reference."
- v3.0 migration guide will include a checklist: every `@public` file
  from v2.1.1 must exist in v3.0 with the same public function names
  across both backends. Same check re-fires at v4.0 when the wgpu
  path retires. Tooling to diff the public-API surface lives in
  `scripts/check-public-api.sh` (planned for v3.0 milestone).
