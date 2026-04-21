# Makefile for mabda
#
# Most commands delegate to the `cyrius` CLI, which reads cyrius.cyml.
# The GPU integration test (programs/phase0.cyr) stays here because it
# links against wgpu-native through a C launcher (deps/wgpu_main.c).
#
# Quick reference:
#   make test           — CPU-only tests (`cyrius test tests/tcyr/mabda.tcyr`)
#   make bench          — CPU-only benchmarks
#   make build          — link-check the library (programs/smoke.cyr)
#   make dist           — regenerate dist/mabda.cyr via `cyrius distlib`
#   make test-phase0    — GPU integration test (requires wgpu-native)
#   make test-all       — version-check + dist regen + CPU tests
#   make lint / fmt-check / vet  — quality gates
#   make clean          — scrub build/

CYRIUS     ?= cyrius
CC5        ?= cc5
GCC        ?= gcc
WGPU_DIR   ?= deps/wgpu-native

# ---------------------------------------------------------------------------
# Library gates (no GPU needed)
# ---------------------------------------------------------------------------

.PHONY: build
build:
	@mkdir -p build
	CYRIUS_DCE=1 $(CYRIUS) build programs/smoke.cyr build/mabda_smoke
	@echo "smoke: $$(wc -c < build/mabda_smoke) bytes"

.PHONY: test
test:
	$(CYRIUS) test tests/tcyr/mabda.tcyr

.PHONY: bench
bench:
	$(CYRIUS) bench tests/bcyr/mabda.bcyr

.PHONY: lint
lint:
	@fail=0; \
	for f in src/*.cyr programs/*.cyr tests/tcyr/*.tcyr tests/bcyr/*.bcyr; do \
		out=$$($(CYRIUS) lint $$f 2>&1); echo "$$out"; \
		echo "$$out" | grep -qE '^\s*warn ' && fail=1; \
	done; \
	[ $$fail -eq 0 ] || { echo "lint: warnings present"; exit 1; }

.PHONY: fmt-check
fmt-check:
	@fail=0; \
	for f in src/*.cyr programs/*.cyr tests/tcyr/*.tcyr tests/bcyr/*.bcyr; do \
		diff -q <($(CYRIUS) fmt $$f --check 2>/dev/null) $$f > /dev/null || \
			{ echo "needs fmt: $$f"; fail=1; }; \
	done; \
	[ $$fail -eq 0 ] || { echo "fmt: drift detected"; exit 1; }

.PHONY: vet
vet:
	$(CYRIUS) vet programs/smoke.cyr

.PHONY: dist
dist:
	$(CYRIUS) distlib

.PHONY: version-check
version-check:
	@./scripts/version-check.sh

.PHONY: test-all
test-all: version-check dist test

# ---------------------------------------------------------------------------
# GPU integration tests (require wgpu-native + deps/wgpu_main.c shim)
#
# `object;` mode is the one sanctioned direct-cc5 invocation (see CLAUDE.md).
# A future `cyrius build --object` (queued upstream for 5.4.10+) will retire it.
# ---------------------------------------------------------------------------

LOCALIZE_SYMS  = memcpy memset memchr strlen strchr strstr memeq atoi
LOCALIZE_FLAGS = $(foreach s,$(LOCALIZE_SYMS),-L $(s))

deps/wgpu_main.o: deps/wgpu_main.c
	$(GCC) -c $< -I$(WGPU_DIR)/include -o $@

# Pattern rule for all programs/*.cyr GPU programs.
build/%.o: programs/%.cyr src/*.cyr
	@mkdir -p build
	printf 'object;\n' | cat - $< | $(CC5) > $@
	objcopy $(LOCALIZE_FLAGS) -L print_num -L println $@

build/phase0: build/phase0.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/phase0.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

build/compute_e2e: build/compute_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/compute_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

build/render_e2e: build/render_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/render_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

build/benchmarks: build/benchmarks.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/benchmarks.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

build/render_graph_e2e: build/render_graph_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/render_graph_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-phase0
test-phase0: build/phase0
	./build/phase0

.PHONY: test-compute-e2e
test-compute-e2e: build/compute_e2e
	./build/compute_e2e

.PHONY: test-render-e2e
test-render-e2e: build/render_e2e
	./build/render_e2e

.PHONY: test-render-graph-e2e
test-render-graph-e2e: build/render_graph_e2e
	./build/render_graph_e2e

# GPU-backed benchmarks. Parity with Rust v1.0's benches/benchmarks.rs
# (13 benches). Reports both human-readable lines and CSV:name,ns rows
# that scripts/bench-record.sh can append to bench-history.csv.
.PHONY: bench-gpu
bench-gpu: build/benchmarks
	./build/benchmarks

# Developer gate: run every GPU integration program in sequence.
.PHONY: test-gpu
test-gpu: test-phase0 test-compute-e2e test-render-e2e test-render-graph-e2e

# CI gate: syntax + semantic check every programs/*.cyr without needing
# wgpu-native on the runner. Fails on any cyrius warning/error coming from
# mabda-owned source (programs/ or src/). Warnings whose path begins with
# `lib/` originate in the cyrius stdlib and are filtered out — they are
# tracked upstream, not here.
# Closes the Issue-2-class bug (missing includes compiling silently) from
# docs/issues/2026-04-19-phase0-build-broken.md.
.PHONY: build-gpu-programs
build-gpu-programs:
	@fail=0; \
	for f in programs/*.cyr; do \
		out=$$($(CYRIUS) check $$f 2>&1); \
		flagged=$$(echo "$$out" | grep -E '(warning|error):' | grep -vE '(warning|error):lib/' || true); \
		if [ -n "$$flagged" ]; then \
			echo "$$f:"; echo "$$out"; fail=1; \
		fi; \
	done; \
	[ $$fail -eq 0 ] || { echo "build-gpu-programs: warnings/errors in programs/"; exit 1; }

.PHONY: clean
clean:
	rm -rf build/
