# Contributing to Mabda

Thank you for your interest in contributing to Mabda.

## Prerequisites

- [Cyrius](https://github.com/MacCracken/cyrius) 3.4.14+
- gcc (for GPU tests — links Cyrius .o with wgpu-native)
- Vulkan drivers (for GPU tests)

## Development Workflow

1. Fork and clone the repository
2. Set up the Cyrius stdlib symlink: `cd cyr && ln -sf ~/.cyrius/lib lib`
3. Fetch wgpu-native (one-time): `cd deps && sh fetch-wgpu.sh`
4. Make your changes in `cyr/src/`
5. Run tests: `cyrius test tests/test_color.tcyr` (and other test suites)
6. For GPU changes: `make test-phase0`
7. Submit a PR

## Project Structure

```
cyr/
├── src/           # Cyrius source modules
├── tests/         # Test suites (.tcyr files)
├── deps/          # wgpu-native binaries + C shim (gitignored)
├── cyrius.toml    # Build configuration
└── Makefile       # Hybrid C/Cyrius build
```

## Running Tests

```sh
cd cyr

# Standalone tests (no GPU needed)
cyrius test tests/test_color.tcyr
cyrius test tests/test_profiler.tcyr
cyrius test tests/test_vertex.tcyr
cyrius test tests/test_dynlib.tcyr

# GPU integration tests (requires Vulkan)
make test-phase0
```

## Adding a Module

1. Create `cyr/src/mymodule.cyr`
2. Add `include "src/mymodule.cyr"` in `cyr/src/mabda.cyr`
3. Add tests in `cyr/tests/test_mymodule.tcyr`
4. Update CHANGELOG.md

## Code Style

- Use `#` comments (not `//`)
- Prefix private functions with `_`
- Document struct layouts with byte offset comments
- Use `alloc(N)` for heap structs, `store64/load64` for field access
- Error handling via tagged unions (Ok/Err from tagged.cyr)
- No `unwrap()` or `panic!()` equivalent — return errors
- Use `f64_*` builtins for float math, `f64_to_f32` at GPU boundary

## Commit Messages

Follow [Keep a Changelog](https://keepachangelog.com/) categories:
- `add: feature description` for new features
- `fix: bug description` for bug fixes
- `change: what changed` for modifications

## License

By contributing, you agree that your contributions will be licensed under GPL-3.0-only.
