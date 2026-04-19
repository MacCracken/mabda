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
# GPU integration test (requires wgpu-native + deps/wgpu_main.c shim)
# ---------------------------------------------------------------------------

LOCALIZE_SYMS  = memcpy memset memchr strlen strchr memeq atoi
LOCALIZE_FLAGS = $(foreach s,$(LOCALIZE_SYMS),-L $(s))

deps/wgpu_main.o: deps/wgpu_main.c
	$(GCC) -c $< -I$(WGPU_DIR)/include -o $@

build/phase0.o: programs/phase0.cyr src/*.cyr
	@mkdir -p build
	printf 'object;\n' | cat - programs/phase0.cyr | $(CC5) > $@
	objcopy $(LOCALIZE_FLAGS) -L print_num -L println $@

build/phase0: build/phase0.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/phase0.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-phase0
test-phase0: build/phase0
	./build/phase0

.PHONY: clean
clean:
	rm -rf build/
