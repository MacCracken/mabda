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

test-all: test-dynlib test-color test-profiler test-vertex test-phase0

clean:
	rm -rf build/

.PHONY: test-phase0 test-color test-dynlib test-profiler test-vertex test-all clean
