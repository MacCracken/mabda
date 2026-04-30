# Makefile for mabda
#
# Most commands delegate to the `cyrius` CLI, which reads cyrius.cyml.
# The GPU integration test (programs/phase0.cyr) stays here because it
# links against wgpu-native through a C launcher (deps/wgpu_main.c).
#
# Quick reference:
#   make test           — CPU-only tests (`cyrius test tests/tcyr/mabda.tcyr`)
#   make bench          — CPU-only benchmarks
#   make fuzz           — invariant harnesses under fuzz/*.fcyr
#   make build          — link-check the library (programs/smoke.cyr)
#   make dist           — regenerate dist/mabda.cyr via `cyrius distlib`
#   make test-phase0    — GPU integration test (requires wgpu-native)
#   make test-native-enum — v3 Phase B.1 DRM probe (requires DRM hardware)
#   make test-native-gem-roundtrip — v3 Phase B.2 GEM BO round-trip (requires DRM hardware)
#   make test-native-submit-setup — v3 Phase B.3.a ctx/BO-list/VA setup (requires DRM hardware)
#   make test-all       — version-check + dist regen + CPU tests + fuzz
#   make lint / fmt-check / vet  — quality gates
#   make clean          — scrub build/

CYRIUS     ?= cyrius
CC5        ?= cc5
GCC        ?= gcc
WGPU_DIR   ?= deps/wgpu-native

# ---------------------------------------------------------------------------
# Lib-wiring guard — refuses to build if lib/ is a symlink to a cyrius
# checkout. See CLAUDE.md "Dependency wiring" — that configuration causes
# cross-repo writes when an agent working in mabda edits lib/*.cyr.
# ---------------------------------------------------------------------------
.PHONY: check-lib-wiring
check-lib-wiring:
	@if [ -L lib ]; then \
		echo "ERROR: lib/ is a symlink ($$(readlink lib))."; \
		echo "       mabda's lib/ must be a real directory populated by"; \
		echo "       'cyrius deps'. See CLAUDE.md > Dependency wiring."; \
		echo "       Fix: rm lib && mkdir lib && cyrius deps"; \
		exit 1; \
	fi

# ---------------------------------------------------------------------------
# Library gates (no GPU needed)
# ---------------------------------------------------------------------------

.PHONY: build
build: check-lib-wiring
	@mkdir -p build
	CYRIUS_DCE=1 $(CYRIUS) build programs/smoke.cyr build/mabda_smoke
	@echo "smoke: $$(wc -c < build/mabda_smoke) bytes"

.PHONY: test
test: check-lib-wiring
	$(CYRIUS) test tests/tcyr/mabda.tcyr
	$(CYRIUS) test tests/tcyr/mabda_v3.tcyr
	$(CYRIUS) test tests/tcyr/mabda_v3_phase_d.tcyr

.PHONY: bench
bench: check-lib-wiring
	$(CYRIUS) bench tests/bcyr/mabda.bcyr

# Fuzz harnesses — each fuzz/*.fcyr is a standalone program that
# exits 0 on pass, nonzero on invariant violation. `cyrius test`
# runs and checks the exit code. Convention matches cyrius stdlib
# (../cyrius/fuzz/*.fcyr).
.PHONY: fuzz
fuzz:
	@fail=0; \
	for f in fuzz/*.fcyr; do \
		printf '%-48s ' "$$f"; \
		$(CYRIUS) test $$f > /tmp/mabda-fuzz.log 2>&1; \
		rc=$$?; \
		if [ $$rc -eq 0 ]; then echo "PASS"; else echo "FAIL (exit $$rc)"; fail=1; fi; \
	done; \
	[ $$fail -eq 0 ] || { echo "fuzz: at least one harness failed"; exit 1; }

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
test-all: version-check dist test fuzz

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

# v3 Phase B.1 — hardware integration. Probes /dev/dri/renderD128 via
# direct syscall(SYS_IOCTL), prints driver name + version. Requires
# DRM hardware on the host; not in CI. Pure Cyrius — no wgpu-native,
# no C launcher, no libdrm linked.
build/native_device_enum: programs/native_device_enum.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_device_enum.cyr $@

.PHONY: test-native-enum
test-native-enum: build/native_device_enum
	./build/native_device_enum

# v3 Phase B.2 — GEM BO round-trip. Creates a 4 KiB GTT buffer object,
# mmaps it, writes a deterministic pattern, reads it back byte-identical,
# releases. Requires DRM hardware; not in CI.
build/native_gem_roundtrip: programs/native_gem_roundtrip.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_gem_roundtrip.cyr $@

.PHONY: test-native-gem-roundtrip
test-native-gem-roundtrip: build/native_gem_roundtrip
	./build/native_gem_roundtrip

# v3 Phase B.3.a — submission prerequisites (ctx, BO list, VA map).
# Exercises every setup ioctl without submitting any GPU work.
# Requires DRM hardware; not in CI.
build/native_submit_setup: programs/native_submit_setup.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_submit_setup.cyr $@

.PHONY: test-native-submit-setup
test-native-submit-setup: build/native_submit_setup
	./build/native_submit_setup

# v3 Phase B.3.d — first live compute dispatch. Uploads an s_endpgm
# shader, builds a PM4 stream, submits via DRM_IOCTL_AMDGPU_CS, waits
# on a sync-obj. Exits 0 iff the dispatch completes.
build/native_compute_spike: programs/native_compute_spike.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_compute_spike.cyr $@

.PHONY: test-native-compute-spike
test-native-compute-spike: build/native_compute_spike
	./build/native_compute_spike

# v3 Phase B.4 — compute shader writes a constant to memory; CPU
# reads back and verifies. First real mabda GPU work with verifiable
# output. Clang-assembled GFX9 ISA; no WGSL compilation pipeline yet
# (that's post-v3.0).
build/native_compute_store: programs/native_compute_store.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_compute_store.cyr $@

.PHONY: test-native-compute-store
test-native-compute-store: build/native_compute_store
	./build/native_compute_store

build/native_texture_e2e: programs/native_texture_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_texture_e2e.cyr $@

.PHONY: test-native-texture-e2e
test-native-texture-e2e: build/native_texture_e2e
	./build/native_texture_e2e

# v3 Step 6.9(b) — native render end-to-end (mirror of render_e2e
# on the native graphics ring). HW-gated; runs the full 6.x chain
# (shader bytes → PM4 composer → GFX dispatch → CPU readback).
build/native_render_e2e: programs/native_render_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_render_e2e.cyr $@

.PHONY: test-native-render-e2e
test-native-render-e2e: build/native_render_e2e
	./build/native_render_e2e

# v3 Step 7.1(c) — DRM/KMS topology diagnostic. Needs a DRM master
# fd (/dev/dri/card0); typically requires a desktop session or
# root. Run via `make test-native-kms-summary`.
build/native_kms_summary: programs/native_kms_summary.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_kms_summary.cyr $@

.PHONY: test-native-kms-summary
test-native-kms-summary: build/native_kms_summary
	./build/native_kms_summary

# v3 Step 7.2(d) — end-to-end modeset live test. Requires DRM
# master — run from a tty (Ctrl-Alt-F2 to drop the running
# compositor's master). Screen turns solid red for 3 seconds
# on success.
build/native_kms_modeset_smoke: programs/native_kms_modeset_smoke.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_kms_modeset_smoke.cyr $@

.PHONY: test-native-kms-modeset
test-native-kms-modeset: build/native_kms_modeset_smoke
	./build/native_kms_modeset_smoke

# v3 Phase B (Sessions 11–12) — libdrm_amdgpu reference programs.
# Used to differentiate "is this bug in our direct ioctls?" (spike =
# libdrm-canonical, hangs identically) from "is shader-dispatch the
# fix?" (store_spike = full Mesa preamble + real shader, also hangs).
# Both are diagnostic — shipping mabda doesn't depend on them.
build/libdrm_spike: deps/libdrm_spike.c
	@mkdir -p build
	cc -O2 -Wall -o $@ $< -ldrm_amdgpu

build/libdrm_store_spike: deps/libdrm_store_spike.c
	@mkdir -p build
	cc -O2 -Wall -o $@ $< -ldrm_amdgpu

.PHONY: test-libdrm-spike
test-libdrm-spike: build/libdrm_spike
	./build/libdrm_spike

.PHONY: test-libdrm-store-spike
test-libdrm-store-spike: build/libdrm_store_spike
	./build/libdrm_store_spike

# Post-reboot diagnostic baseline (Session 13 entry point). Runs Mesa
# cl_probe (must work — proves GPU is alive), then both libdrm spikes
# (must hang identically — confirms blocker is reproducible). Inspects
# kernel reset count via journalctl. Order matters: cl_probe first so
# we know GPU is healthy before we start triggering resets.
.PHONY: gpu-baseline
gpu-baseline: build/libdrm_spike build/libdrm_store_spike build/shader/cl_probe
	@echo "==== Mesa cl_probe (must PASS, ~80 ms) ===="
	time ./build/shader/cl_probe
	@echo
	@echo "==== libdrm_spike — bare WRITE_DATA (expect ECANCELED ~10 s) ===="
	-time ./build/libdrm_spike
	@echo
	@echo "==== libdrm_store_spike — full shader-dispatch (expect ECANCELED) ===="
	-time ./build/libdrm_store_spike
	@echo
	@echo "==== recent AMDGPU journal entries ===="
	-journalctl --since "2 minutes ago" --no-pager 2>/dev/null | \
		grep -iE "amdgpu|gpu reset" | tail -10

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
