# Makefile for mabda Cyrius port
# Compiles Cyrius code to .o via cc3, links with wgpu shim via gcc

CC3 = /home/macro/Repos/cyrius/build/cc3
GCC = gcc
WGPU_DIR = deps/wgpu-native
SHIM_SRC = deps/wgpu_shim.c

# Build the wgpu shim (C wrapper with wgpu-native statically linked)
deps/wgpu_shim.o: $(SHIM_SRC)
	$(GCC) -c -fPIC $(SHIM_SRC) -I$(WGPU_DIR)/include -o deps/wgpu_shim.o

# Phase 0 test: compile Cyrius to .o, link with C launcher + wgpu-native
LOCALIZE_SYMS = memcpy memset memchr strlen strchr memeq atoi
LOCALIZE_FLAGS = $(foreach sym,$(LOCALIZE_SYMS),-L $(sym))

build/test_phase0.o: tests/test_phase0.tcyr src/*.cyr
	@mkdir -p build
	printf 'object;\n' | cat - tests/test_phase0.tcyr | $(CC3) > build/test_phase0.o
	objcopy $(LOCALIZE_FLAGS) -L print_num -L println build/test_phase0.o

deps/wgpu_main.o: deps/wgpu_main.c
	$(GCC) -c deps/wgpu_main.c -I$(WGPU_DIR)/include -o deps/wgpu_main.o

build/test_phase0: build/test_phase0.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/test_phase0.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o build/test_phase0

test-phase0: build/test_phase0
	./build/test_phase0

# Color test (no GPU needed)
build/test_color: tests/test_color.tcyr src/color.cyr
	@mkdir -p build
	cat tests/test_color.tcyr | $(CC3) > build/test_color && chmod +x build/test_color

test-color: build/test_color
	./build/test_color

# Dynlib test (no GPU needed)
build/test_dynlib: tests/test_dynlib.tcyr lib/dynlib.cyr
	@mkdir -p build
	cat tests/test_dynlib.tcyr | $(CC3) > build/test_dynlib && chmod +x build/test_dynlib

test-dynlib: build/test_dynlib
	./build/test_dynlib

# Profiler test (no GPU needed)
build/test_profiler: tests/test_profiler.tcyr src/profiler.cyr src/color.cyr
	@mkdir -p build
	cat tests/test_profiler.tcyr | $(CC3) > build/test_profiler && chmod +x build/test_profiler

test-profiler: build/test_profiler
	./build/test_profiler

# Vertex + blend test (no GPU needed)
build/test_vertex: tests/test_vertex.tcyr src/vertex.cyr src/blend.cyr src/color.cyr
	@mkdir -p build
	cat tests/test_vertex.tcyr | $(CC3) > build/test_vertex && chmod +x build/test_vertex

test-vertex: build/test_vertex
	./build/test_vertex

# Typed buffer pure-data test (no GPU needed)
build/test_typed_buffer: tests/test_typed_buffer.tcyr src/error.cyr src/wgpu_types.cyr
	@mkdir -p build
	cat tests/test_typed_buffer.tcyr | $(CC3) > build/test_typed_buffer && chmod +x build/test_typed_buffer

test-typed-buffer: build/test_typed_buffer
	./build/test_typed_buffer

# Error module pure-data test (no GPU needed)
build/test_error: tests/test_error.tcyr src/error.cyr
	@mkdir -p build
	cat tests/test_error.tcyr | $(CC3) > build/test_error && chmod +x build/test_error

test-error: build/test_error
	./build/test_error

# Capabilities pure-data test (no GPU needed)
build/test_capabilities: tests/test_capabilities.tcyr src/capabilities.cyr
	@mkdir -p build
	cat tests/test_capabilities.tcyr | $(CC3) > build/test_capabilities && chmod +x build/test_capabilities

test-capabilities: build/test_capabilities
	./build/test_capabilities

# Blend + sampler + depth pure-data tests (no GPU needed)
build/test_state: tests/test_state.tcyr src/blend.cyr src/sampler.cyr src/depth.cyr
	@mkdir -p build
	cat tests/test_state.tcyr | $(CC3) > build/test_state && chmod +x build/test_state

test-state: build/test_state
	./build/test_state

# Cache module pure-data tests (no GPU needed)
build/test_caches: tests/test_caches.tcyr src/shader_cache.cyr src/pipeline_cache.cyr src/bind_group_cache.cyr src/cache_key.cyr
	@mkdir -p build
	cat tests/test_caches.tcyr | $(CC3) > build/test_caches && chmod +x build/test_caches

test-caches: build/test_caches
	./build/test_caches

# Surface pure-data test (no GPU or window system needed)
build/test_surface: tests/test_surface.tcyr src/wgpu_descriptors.cyr src/wgpu_types.cyr
	@mkdir -p build
	cat tests/test_surface.tcyr | $(CC3) > build/test_surface && chmod +x build/test_surface

test-surface: build/test_surface
	./build/test_surface

# CPU-only benchmark harness (no GPU needed). Run via `make bench`.
build/bench_mabda: tests/mabda.bcyr src/color.cyr src/profiler.cyr src/capabilities.cyr
	@mkdir -p build
	cat tests/mabda.bcyr | $(CC3) > build/bench_mabda && chmod +x build/bench_mabda

bench: build/bench_mabda
	./build/bench_mabda

test-all: test-dynlib test-color test-profiler test-vertex test-typed-buffer \
          test-error test-capabilities test-state test-caches test-surface test-phase0

clean:
	rm -rf build/

.PHONY: test-phase0 test-color test-dynlib test-profiler test-vertex test-typed-buffer \
        test-error test-capabilities test-state test-caches test-surface bench test-all clean
