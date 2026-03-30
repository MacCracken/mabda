# Contributing to Mabda

Thank you for your interest in contributing to Mabda.

## Development Workflow

1. Fork and clone the repository
2. Create a feature branch from `main`
3. Make your changes
4. Run `make check` to validate
5. Open a pull request

## Prerequisites

- Rust stable (MSRV 1.89)
- Components: `rustfmt`, `clippy`
- Optional: `cargo-audit`, `cargo-deny`, `cargo-llvm-cov`, `cargo-tarpaulin`

## Makefile Targets

| Command | Description |
|---------|-------------|
| `make check` | fmt + clippy + test + audit |
| `make fmt` | Check formatting |
| `make clippy` | Lint with `-D warnings` |
| `make test` | Run test suite |
| `make audit` | Security audit |
| `make deny` | Supply chain checks |
| `make bench` | Run benchmarks with history tracking |
| `make coverage` | Generate HTML coverage report |
| `make coverage-check` | Verify coverage >= 70% threshold |
| `make doc` | Build documentation |

## Adding a Module

1. Create `src/module_name.rs` with module doc comment
2. Add `pub mod module_name;` to `src/lib.rs`
3. Re-export key types from `lib.rs`
4. Add unit tests in the module (`#[cfg(test)] mod tests`)
5. Add `/// # Examples` doc block on all public types
6. Update README feature list

If the module requires an external dependency, gate it behind a feature flag.

## Feature Flags

| Feature | Description |
|---------|-------------|
| `full` | Enables both `graphics` and `compute` (default) |
| `graphics` | Render pipelines, textures, surfaces, depth, blend, instancing |
| `compute` | Compute pipelines, dispatch, ping-pong buffers |

## Code Style

- `cargo fmt` — mandatory
- `cargo clippy -- -D warnings` — zero warnings
- Doc comments on all public items
- `#[non_exhaustive]` on public enums
- `#[must_use]` on all pure functions
- `#[inline]` on hot-path functions
- No `unwrap()` or `panic!()` in library code
- Use `tracing` for structured logging, not `println!`
- `write!` over `format!` — avoid temporary allocations
- `Cow` over clone — borrow when you can, allocate only when you must

## Testing

- Unit tests colocated in modules (`#[cfg(test)] mod tests`)
- GPU integration tests use headless wgpu backend (skip if no adapter)
- Feature-gated tests with `#[cfg(feature = "...")]`
- Target: 80%+ line coverage (CI gate at 70%)
- Use `f32::EPSILON` or small epsilon for float comparisons

## Benchmarks

- All benchmarks use Criterion
- Run `make bench` to record results to `bench-history.csv`
- Performance claims in CHANGELOG must include benchmark numbers
- Never skip benchmarks before claiming improvements

## Commits

- Use conventional-style messages
- One logical change per commit

## License

By contributing, you agree that your contributions will be licensed under GPL-3.0-only.
