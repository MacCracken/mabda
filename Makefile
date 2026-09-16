# Makefile for mabda
#
# Most commands delegate to the `cyrius` CLI, which reads cyrius.cyml.
# The GPU integration test (programs/phase0.cyr) stays here because it
# links against wgpu-native through a C launcher (deps/wgpu_main.c).
#
# Quick reference:
#   make test           — CPU-only tests (globs tests/tcyr/*.tcyr domain suites)
#   make bench          — CPU-only benchmarks
#   make fuzz           — invariant harnesses under fuzz/*.fcyr (logs: build/fuzz-logs/)
#   make check-fuzz-logs — self-test of the fuzz recipe's per-harness logs (no compiler)
#   make check-make-dry-run — self-test: `make -n/-t/-q` stops every $(MAKE) recipe (no compiler)
#   make check-stack-array-sizing — static gate: no write past a `var buf[N]` (+ its self-test)
#   make check-program-write-lengths — static gate: no hand-counted write lengths / pass tallies
#   make check-count-script — self-test of scripts/count-test-assertions.sh
#   make lint-deferrals — the pinned cyrlint --strict-deferrals over every linted file
#   make example-link   — build examples/stdlib-consumer, never run it (link step needs wgpu-native)
#   make build          — link-check the library (programs/smoke.cyr)
#   make dist           — regenerate dist/mabda.cyr via `cyrius distlib`
#   make test-phase0    — GPU integration test (requires wgpu-native)
#   make test-native-enum — v3 Phase B.1 DRM probe (requires DRM hardware)
#   make test-native-gem-roundtrip — v3 Phase B.2 GEM BO round-trip (requires DRM hardware)
#   make test-native-submit-setup — v3 Phase B.3.a ctx/BO-list/VA setup (requires DRM hardware)
#   make test-all       — version-check + static gates + dist regen + CPU tests + fuzz + self-tests
#                         + deferral lint + example link (all CPU-only; no GPU is touched)
#   make lint / fmt-check / vet  — quality gates
#   make clean          — scrub build/

CYRIUS     ?= cyrius
# The Cyrius C-backend object compiler. Renamed cc5 -> cycc in cyrius 6.1
# (6.0.x shipped cc5/cc5_aarch64/cc5_win; 6.1+ ship cycc/cycc_aarch64/
# cycc_win). The wgpu integration programs link against deps/wgpu_main.c
# and build via this object-mode path. Override with CYCC=... if needed.
CYCC       ?= cycc
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
# Functionality-grouped CPU suites (v3.1 test reorg 2026-06-15): one
# file per domain under tests/tcyr/. Globbed so new domain files are
# picked up automatically; each is a standalone suite with its own main().
test: check-lib-wiring
	@for f in tests/tcyr/*.tcyr; do $(CYRIUS) test "$$f" || exit 1; done

.PHONY: bench
bench: check-lib-wiring
	$(CYRIUS) bench tests/bcyr/mabda.bcyr

# ---------------------------------------------------------------------------
# Recursion guard for recipes that call $(MAKE).
#
# GNU make runs a recipe line that references $(MAKE) even under -n (--dry-run),
# -t (--touch) and -q (--question), and the inner make inherits the flag. A recipe
# that loops over $(MAKE) and tallies the results then reports on work that never
# happened: under -n / -t every inner gate "passes" without running (before 4.1.3,
# `make -n test-native-all` printed "71 passed"), and under -q every one "fails".
# So every such recipe starts with $(RECURSION_GUARD): under -n / -t it prints
# "<target>: not run under make -n / -t" and succeeds; under -q it exits 1 without
# output, which is make's own -q answer for a phony target (never up to date).
# `make check-make-dry-run` proves every $(MAKE) recipe in this file does.
# ---------------------------------------------------------------------------
MAKE_MODE_FLAGS = $(firstword -$(MAKEFLAGS))
MAKE_NO_RUN     = $(or $(findstring n,$(MAKE_MODE_FLAGS)),$(findstring t,$(MAKE_MODE_FLAGS)))
RECURSION_GUARD = $(if $(findstring q,$(MAKE_MODE_FLAGS)),exit 1;)$(if $(MAKE_NO_RUN),echo "$@: not run under make -n / -t"; exit 0;)

# Fuzz harnesses — each fuzz/*.fcyr is a standalone program that
# exits 0 on pass, nonzero on invariant violation. `cyrius test`
# runs and checks the exit code. Convention matches cyrius stdlib
# (../cyrius/fuzz/*.fcyr).
#
# Each harness writes its own log, $(FUZZ_LOG_DIR)/<harness>.log, and every log
# is kept after the run (build/ is gitignored; `make clean` removes it). A FAIL row
# names its log and shows the last three lines. Until 4.1.3 every harness wrote the
# one shared /tmp/mabda-fuzz.log, so only the last harness's output survived: a
# failing harness's log was overwritten by whichever harness ran after it.
# FUZZ_LOG_DIR can point anywhere (even a shared directory like /tmp), so a run only
# removes the per-harness logs it is about to rewrite, never other *.log files.
# `make check-fuzz-logs` is the regression gate for this recipe.
FUZZ_LOG_DIR ?= build/fuzz-logs

.PHONY: fuzz
fuzz:
	@mkdir -p "$(FUZZ_LOG_DIR)" || exit 1; \
	for f in fuzz/*.fcyr; do rm -f "$(FUZZ_LOG_DIR)/$$(basename "$$f" .fcyr).log"; done; \
	fail=0; \
	for f in fuzz/*.fcyr; do \
		log="$(FUZZ_LOG_DIR)/$$(basename "$$f" .fcyr).log"; \
		printf '%-48s ' "$$f"; \
		$(CYRIUS) test $$f > "$$log" 2>&1; \
		rc=$$?; \
		if [ $$rc -eq 0 ]; then echo "PASS"; else \
			echo "FAIL (exit $$rc)  log: $$log"; tail -3 "$$log" | sed 's/^/      /'; fail=1; \
		fi; \
	done; \
	[ $$fail -eq 0 ] || { echo "fuzz: at least one harness failed (logs: $(FUZZ_LOG_DIR)/)"; exit 1; }

# Regression gate for the `fuzz` recipe itself — needs no compiler and no GPU. Runs
# `make fuzz` twice with a stub CYRIUS that prints one line naming the harness it was
# given, into a FUZZ_LOG_DIR that already holds an unrelated.log:
#   1. the stub fails the first harness: `make fuzz` must exit non-zero, its FAIL row
#      must name that harness's log, and every harness must leave a log holding
#      exactly its own line (the shared-log recipe this replaced fails that);
#   2. every harness passes: `make fuzz` must exit 0 and print no FAIL row (a recipe
#      that always exits 1 passed scenario 1 alone).
# Both runs must leave unrelated.log untouched (the recipe once ran `rm *.log` there).
.PHONY: check-fuzz-logs
check-fuzz-logs:
	@$(RECURSION_GUARD) \
	tmp=$$(mktemp -d -t mabda-fuzzcheck-XXXXXX) || exit 1; \
	trap 'rm -rf "$$tmp"' EXIT; \
	first=$$(ls fuzz/*.fcyr | head -1); \
	[ -n "$$first" ] || { echo "check-fuzz-logs: no fuzz/*.fcyr harnesses"; exit 1; }; \
	mkdir -p "$$tmp/logs" && echo "not a harness log" > "$$tmp/logs/unrelated.log"; \
	bad=0; \
	for scenario in one-fails all-pass; do \
		stub="$$tmp/cyrius-stub-$$scenario"; \
		failing="$$first"; [ "$$scenario" = all-pass ] && failing="(none)"; \
		printf '#!/bin/sh\necho "stub-output-for $$2"\n[ "$$2" = "%s" ] && exit 7\nexit 0\n' "$$failing" > "$$stub"; \
		chmod +x "$$stub"; \
		out=$$($(MAKE) --no-print-directory -f $(firstword $(MAKEFILE_LIST)) fuzz CYRIUS="$$stub" FUZZ_LOG_DIR="$$tmp/logs" 2>&1); rc=$$?; \
		echo "  [$$scenario]"; echo "$$out" | sed 's/^/  | /'; \
		if [ "$$scenario" = one-fails ]; then \
			if [ $$rc -eq 0 ]; then echo "check-fuzz-logs: make fuzz exited 0 with a failing harness"; bad=1; fi; \
			flog="$$tmp/logs/$$(basename "$$first" .fcyr).log"; \
			if ! echo "$$out" | grep -qF "FAIL (exit 7)  log: $$flog"; then \
				echo "check-fuzz-logs: the FAIL row does not name $$flog"; bad=1; fi; \
		else \
			if [ $$rc -ne 0 ]; then echo "check-fuzz-logs: make fuzz exited $$rc with every harness passing"; bad=1; fi; \
			if echo "$$out" | grep -q 'FAIL'; then echo "check-fuzz-logs: FAIL printed with every harness passing"; bad=1; fi; \
		fi; \
		for f in fuzz/*.fcyr; do \
			log="$$tmp/logs/$$(basename "$$f" .fcyr).log"; \
			if [ "$$(cat "$$log" 2>/dev/null)" != "stub-output-for $$f" ]; then \
				echo "check-fuzz-logs: [$$scenario] $$f has no log holding only its own output ($$log)"; bad=1; fi; \
		done; \
		if [ "$$(cat "$$tmp/logs/unrelated.log" 2>/dev/null)" != "not a harness log" ]; then \
			echo "check-fuzz-logs: [$$scenario] make fuzz removed or changed an unrelated *.log in FUZZ_LOG_DIR"; bad=1; fi; \
	done; \
	if [ $$bad -ne 0 ]; then exit 1; fi; \
	echo "check-fuzz-logs: OK (one log per harness, failure reported with its log, all-pass exits 0, other logs kept)"

# Regression gate for RECURSION_GUARD — needs no compiler and no GPU. Finds every recipe
# here that calls $(MAKE) (test-native-all, check-fuzz-logs, this one) and runs each under
# -n, -t and -q (and the long spellings) with MAKE pointed at a stub: the stub must never
# be reached, -n / -t must print the guard line and exit 0, -q must exit 1 silently. The
# script's --self-test proves it catches an unguarded recipe and an -n-only guard.
.PHONY: check-make-dry-run
check-make-dry-run:
	@$(RECURSION_GUARD) \
	MAKE="$(MAKE)" sh scripts/check-make-recursion-guards.sh --self-test $(firstword $(MAKEFILE_LIST)) || exit 1; \
	MAKE="$(MAKE)" sh scripts/check-make-recursion-guards.sh $(firstword $(MAKEFILE_LIST))

# Every Cyrius file the quality gates cover, CI included (.github/workflows/ci.yml).
# The consumer example is here so its source is linted and fmt-checked like the rest
# (`make example-link` compiles it; its link step needs wgpu-native).
LINT_FILES = src/*.cyr programs/*.cyr tests/tcyr/*.cyr tests/tcyr/*.tcyr tests/bcyr/*.bcyr \
             fuzz/*.fcyr examples/stdlib-consumer/src/main.cyr

.PHONY: lint
lint:
	@fail=0; \
	for f in $(LINT_FILES); do \
		[ -e "$$f" ] || continue; \
		out=$$($(CYRIUS) lint $$f 2>&1); echo "$$out"; \
		echo "$$out" | grep -qE '^\s*warn ' && fail=1; \
	done; \
	[ $$fail -eq 0 ] || { echo "lint: warnings present"; exit 1; }

# Untracked-deferral gate. `cyrius lint` cannot forward --strict-deferrals (as the first
# argument it prints usage and exits 1; as the last it is silently ignored and exits 0 —
# docs/development/issues/2026-09-16-cyrius-lint-drops-strict-deferrals.md), so this calls
# the pinned toolchain's cyrlint binary directly. It exits non-zero on an untracked
# deferral; a missing binary is a failure, never a skip.
CYRIUS_PIN = $(shell sed -n 's/^cyrius = "\(.*\)"/\1/p' cyrius.cyml | head -1)
CYRLINT   ?= $(HOME)/.cyrius/versions/$(CYRIUS_PIN)/bin/cyrlint

.PHONY: lint-deferrals
lint-deferrals:
	@lint="$(CYRLINT)"; \
	[ -x "$$lint" ] || { echo "lint-deferrals: no executable $$lint (cyrius $(CYRIUS_PIN) not installed)"; exit 1; }; \
	fail=0; n=0; \
	for f in $(LINT_FILES); do \
		[ -e "$$f" ] || continue; \
		n=$$((n + 1)); \
		if ! out=$$("$$lint" --strict-deferrals "$$f" 2>&1); then \
			echo "$$out"; echo "untracked deferral(s): $$f"; fail=1; \
		fi; \
	done; \
	[ $$fail -eq 0 ] || { echo "lint-deferrals: untracked deferrals present"; exit 1; }; \
	echo "lint-deferrals: $$n file(s) clean under $$lint --strict-deferrals"

.PHONY: fmt-check
fmt-check:
	@# cyrius 6.x's `cyrfmt --check <file>` reports formatting via the EXIT
	@# CODE only (0 = clean, non-zero = needs fmt) — it no longer echoes the
	@# formatted file to stdout the way 5.x did, so the old diff-against-stdout
	@# gate false-failed every file. Mirror CI (.github/workflows/ci.yml).
	@fail=0; \
	for f in $(LINT_FILES); do \
		[ -e "$$f" ] || continue; \
		if ! $(CYRIUS) fmt $$f --check > /dev/null 2>&1; then \
			echo "needs fmt: $$f"; fail=1; \
		fi; \
	done; \
	[ $$fail -eq 0 ] || { echo "fmt: drift detected"; exit 1; }

# Static gate for the SPIR-V->GFX9 compiler's caller-provided scratch buffers.
# Several pipeline stages memset a full caller-declared CAPACITY into a buffer
# whose size they cannot see, so an undersized buffer silently overruns the
# caller's stack. `native_spirv_saxpy_e2e` shipped that way for four releases
# (4.0.5 -> 4.0.10). Both numbers are compile-time literals, hence a static gate.
.PHONY: check-buffer-sizing
check-buffer-sizing:
	@./scripts/check-compiler-buffer-sizing.py

# Static gate: no write may land past a `var buf[N]` (N BYTES for a local) or a scalar
# local. Resolves memset/memcpy/storeNN/read/ioctl writes and every helper that writes
# through a parameter, transitively. tests/tcyr/texture.tcyr and queue.tcyr memset
# BACKEND_SIZE (328) into `var be[256]` / `var be[248]` until 4.1.3 — the 72 B overrun
# was the suite's old "leading NUL byte" — and programs/native_array_sample_e2e.cyr /
# wgpu_texture_sample_e2e.cyr wrote 32 B into [20] and 24 B into [3]. Reads lib/, so it
# needs `cyrius deps` first. The self-test proves every detection path on fixtures.
.PHONY: check-stack-array-sizing
check-stack-array-sizing: check-lib-wiring
	@python3 scripts/check-stack-array-sizing.py --self-test > /dev/null || python3 scripts/check-stack-array-sizing.py --self-test
	@python3 scripts/check-stack-array-sizing.py

# Static gate: programs/ never write a string literal with a hand-counted length and never
# print a hand-typed "N passed" tally (21 wrong counts and 10 stale tallies until 4.1.3);
# elsewhere a literal+length write must be the literal's exact byte length.
.PHONY: check-program-write-lengths
check-program-write-lengths:
	@python3 scripts/check-program-write-lengths.py --self-test > /dev/null || python3 scripts/check-program-write-lengths.py --self-test
	@python3 scripts/check-program-write-lengths.py

# Self-test of scripts/count-test-assertions.sh against a stand-in `cyrius`: a crashing,
# summary-less or failing suite must be a named FAIL, a NUL-prefixed summary must count.
.PHONY: check-count-script
check-count-script:
	@./scripts/count-test-assertions.sh --self-test

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
test-all: version-check check-buffer-sizing check-stack-array-sizing check-program-write-lengths \
          lint-deferrals dist test check-count-script check-fuzz-logs check-make-dry-run fuzz \
          example-link

# ---------------------------------------------------------------------------
# GPU integration tests (require wgpu-native + deps/wgpu_main.c shim)
#
# `object;` mode is the one sanctioned direct-cycc invocation (see CLAUDE.md).
# A future `cyrius build --object` (queued upstream for 5.4.10+) will retire it.
# ---------------------------------------------------------------------------

LOCALIZE_SYMS  = memcpy memset memchr strlen strchr strstr memeq atoi
LOCALIZE_FLAGS = $(foreach s,$(LOCALIZE_SYMS),-L $(s))

# The launcher is built warning-free and must stay that way: until 4.1.3 a -Wall
# warning (a const cast on WGPUInstanceDescriptor.nextInChain) sat in every build log
# unnoticed. -Wextra is on since 4.1.4 (the unused callback parameters are voided).
LAUNCHER_WARNINGS = -Wall -Wextra -Werror

deps/wgpu_main.o: deps/wgpu_main.c
	$(GCC) -c $< -I$(WGPU_DIR)/include $(LAUNCHER_WARNINGS) -o $@

# Build gate for examples/stdlib-consumer — it never RUNS the program (that needs a GPU).
# The example's README recipe went unbuilt from 4.0.2 to 4.1.3, so a missing
# `include "lib/thread_local.cyr"` (the launcher calls thread_local_use_foreign_tls) broke
# every consumer following it and nothing noticed. Steps 1-3 need only cycc + binutils and
# run everywhere, CI included; step 4 needs wgpu-native:
#   1. every `include "lib/X.cyr"` in main.cyr names a module the example's [deps].stdlib
#      or dist/mabda.deps provides (`cyrius deps` fetches nothing else);
#   2. main.cyr compiles in object mode against the committed dist/mabda.cyr plus this
#      tree's resolved lib/, and objcopy -L leaves none of LOCALIZE_SYMS global (the
#      2.4.2 glibc interposition crash);
#   3. the object defines every function deps/wgpu_main.c declares `extern`;
#   4. with $(WGPU_DIR) present: the object links with deps/wgpu_main.o (built with
#      LAUNCHER_WARNINGS) + libwgpu_native.a, and the binary defines mabda_main. Without
#      it the link is reported as SKIPPED, never as passed.
EXAMPLE_DIR = examples/stdlib-consumer
EXAMPLE_OUT = build/example-link

.PHONY: example-link
example-link: check-lib-wiring $(if $(wildcard $(WGPU_DIR)/lib/libwgpu_native.a),deps/wgpu_main.o)
	@out="$(EXAMPLE_OUT)"; \
	mods=" $$(sed -n '/^stdlib = \[/,/\]/p' $(EXAMPLE_DIR)/cyrius.cyml | grep -oE '"[a-z0-9_]+"' | tr -d '"' | tr '\n' ' ')"; \
	mods="$$mods $$(grep -vE '^#|^$$' dist/mabda.deps | tr '\n' ' ') mabda "; \
	for inc in $$(sed -n 's/^include "lib\/\([a-z0-9_]*\)\.cyr".*/\1/p' $(EXAMPLE_DIR)/src/main.cyr); do \
		case "$$mods" in *" $$inc "*) ;; *) \
			echo "example-link: main.cyr includes lib/$$inc.cyr, but no declared dep provides it"; exit 1;; esac; \
	done; \
	rm -rf "$$out" && mkdir -p "$$out/lib" || exit 1; \
	cp -L lib/*.cyr "$$out/lib/" && cp dist/mabda.cyr "$$out/lib/mabda.cyr" || exit 1; \
	(cd "$$out" && printf 'object;\n' | cat - "$(CURDIR)/$(EXAMPLE_DIR)/src/main.cyr" | $(CYCC) > hello_gpu.o) \
		|| { echo "example-link: object compile of $(EXAMPLE_DIR)/src/main.cyr failed"; exit 1; }; \
	objcopy $(LOCALIZE_FLAGS) -L print_num -L println "$$out/hello_gpu.o" || exit 1; \
	if nm "$$out/hello_gpu.o" | grep -E " T ($$(echo $(LOCALIZE_SYMS) | tr ' ' '|'))\$$"; then \
		echo "example-link: the symbols above are still GLOBAL after objcopy -L"; exit 1; \
	fi; \
	externs=$$(sed -n 's/^extern [^(]*[ *]\([A-Za-z_][A-Za-z0-9_]*\)(.*/\1/p' deps/wgpu_main.c); \
	[ -n "$$externs" ] || { echo "example-link: found no extern declarations in deps/wgpu_main.c"; exit 1; }; \
	for sym in $$externs; do \
		nm "$$out/hello_gpu.o" | grep -qE " T $$sym\$$" || { \
			echo "example-link: deps/wgpu_main.c calls $$sym, which $(EXAMPLE_DIR)/src/main.cyr does not define"; exit 1; }; \
	done; \
	if [ ! -f "$(WGPU_DIR)/lib/libwgpu_native.a" ]; then \
		echo "example-link: compiled OK, launcher symbols present; link SKIPPED (no $(WGPU_DIR)/lib/libwgpu_native.a)"; \
		exit 0; \
	fi; \
	$(GCC) deps/wgpu_main.o "$$out/hello_gpu.o" $(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm \
		-o "$$out/hello_gpu" || { echo "example-link: LINK FAILED"; exit 1; }; \
	nm "$$out/hello_gpu" | grep -qE ' T mabda_main$$' || { echo "example-link: no mabda_main in the binary"; exit 1; }; \
	echo "example-link: OK, $$out/hello_gpu linked (not run: it needs a GPU)"

# Pattern rule for all programs/*.cyr GPU programs.
build/%.o: programs/%.cyr src/*.cyr
	@mkdir -p build
	printf 'object;\n' | cat - $< | $(CYCC) > $@
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

# v3.2 S.5 — wgpu SPIR-V shader ingestion e2e (SPIR-V vs WGSL cross-source
# identity). Requires wgpu-native + an instance built with ShaderSourceSPIRV
# (deps/wgpu_main.c, S.4).
build/spirv_e2e: build/spirv_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/spirv_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

# v3.2 F.8 — wgpu f64 via SPIR-V passthrough: proves an f64 SPIR-V compute module
# CREATES via the passthrough path (naga cannot carry f64) on a shaderFloat64 device,
# and that gpu_caps_wgpu_shader_f64 is honestly 0 (wgpu compute dispatch is a v3.0
# stub — M.6b). Requires wgpu-native + the SpirvShaderPassthrough device feature (F.8a).
build/f64_compute_e2e: build/f64_compute_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/f64_compute_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

# v3.2 T.8 — wgpu compressed (BC1) create+upload, verified by byte-exact
# copy-back round-trip. Requires wgpu-native + a BC-capable adapter.
build/compressed_texture_e2e: build/compressed_texture_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/compressed_texture_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-compressed-texture-e2e
test-compressed-texture-e2e: build/compressed_texture_e2e
	./build/compressed_texture_e2e

# v3.2 X.7 — wgpu serialized buffer-copy verify: public gpu_buffer_copy
# round-trip on a real wgpu device (the TRANSFER queue aliases the single
# device queue). Requires wgpu-native + deps/wgpu_main.c.
build/wgpu_transfer_copy_e2e: build/wgpu_transfer_copy_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/wgpu_transfer_copy_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-wgpu-transfer-copy-e2e
test-wgpu-transfer-copy-e2e: build/wgpu_transfer_copy_e2e
	./build/wgpu_transfer_copy_e2e

# v3.2 TS.5 — wgpu bind+sample render path: create a sampleable texture, bind
# + sample it across a fullscreen quad, verify the RT is the sampled color.
build/wgpu_texture_sample_e2e: build/wgpu_texture_sample_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/wgpu_texture_sample_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-wgpu-texture-sample-e2e
test-wgpu-texture-sample-e2e: build/wgpu_texture_sample_e2e
	./build/wgpu_texture_sample_e2e

# v3.4 AA.3b — wgpu 2D-array sample: create an array, upload distinct layers,
# sample a chosen layer via a texture_2d_array WGSL FS, verify the RT.
build/wgpu_array_sample_e2e: build/wgpu_array_sample_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/wgpu_array_sample_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-wgpu-array-sample-e2e
test-wgpu-array-sample-e2e: build/wgpu_array_sample_e2e
	./build/wgpu_array_sample_e2e

# v3.4.1 AA.3c — wgpu draw-time layer selection: a FIXED array WGSL reads a
# @binding(2) uniform; gpu_render_pass_bind_texture_layer picks the slice per draw.
build/wgpu_array_layer_select_e2e: build/wgpu_array_layer_select_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/wgpu_array_layer_select_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-wgpu-array-layer-select-e2e
test-wgpu-array-layer-select-e2e: build/wgpu_array_layer_select_e2e
	./build/wgpu_array_layer_select_e2e

# v3.4 AA.5c — wgpu cubemap sample: create a cube, upload 6 faces, sample a face
# by direction via a texture_cube WGSL FS, verify the RT.
build/wgpu_cube_sample_e2e: build/wgpu_cube_sample_e2e.o deps/wgpu_main.o
	$(GCC) deps/wgpu_main.o build/wgpu_cube_sample_e2e.o \
		$(WGPU_DIR)/lib/libwgpu_native.a -lpthread -ldl -lm -o $@

.PHONY: test-wgpu-cube-sample-e2e
test-wgpu-cube-sample-e2e: build/wgpu_cube_sample_e2e
	./build/wgpu_cube_sample_e2e

.PHONY: test-phase0
test-phase0: build/phase0
	./build/phase0

.PHONY: test-compute-e2e
test-compute-e2e: build/compute_e2e
	./build/compute_e2e

.PHONY: test-render-e2e
test-render-e2e: build/render_e2e
	./build/render_e2e

.PHONY: test-spirv-e2e
test-spirv-e2e: build/spirv_e2e
	./build/spirv_e2e

.PHONY: test-f64-compute-e2e
test-f64-compute-e2e: build/f64_compute_e2e
	./build/f64_compute_e2e

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

# v4.0 Phase N1 — NVIDIA/nouveau device enum + masterless probe. Opens
# /dev/dri/renderD128, expects driver "nouveau", reads chipset + PCI ids
# via GETPARAM, and probes that VM_INIT is DRM_RENDER_ALLOW (no master).
# Requires nouveau-bound NVIDIA hardware; not in CI. Pure Cyrius.
build/nvidia_device_enum: programs/nvidia_device_enum.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_device_enum.cyr $@

.PHONY: test-nvidia-enum
test-nvidia-enum: build/nvidia_device_enum
	./build/nvidia_device_enum

# v4.0 Phase N2 — NVIDIA/nouveau GEM BO round-trip. Allocates a
# host-visible (GART|MAPPABLE|COHERENT) BO via GEM_NEW, mmaps the
# returned map_handle, writes a pattern, reads it back byte-identical.
# Masterless; no VM_INIT/VM_BIND (pure CPU path). Requires nouveau
# hardware; not in CI. Pure Cyrius.
build/nvidia_mem_roundtrip: programs/nvidia_mem_roundtrip.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_mem_roundtrip.cyr $@

.PHONY: test-nvidia-mem-roundtrip
test-nvidia-mem-roundtrip: build/nvidia_mem_roundtrip
	./build/nvidia_mem_roundtrip

# v4.0 Phase N3 — NVIDIA/nouveau submission setup. Exercises
# VM_INIT -> CHANNEL_ALLOC -> GEM_NEW -> VM_BIND(MAP/UNMAP) -> syncobj
# masterless, then tears down. No GPU work submitted (EXEC is N4).
# Requires nouveau hardware; not in CI. Pure Cyrius.
build/nvidia_channel_setup: programs/nvidia_channel_setup.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_channel_setup.cyr $@

.PHONY: test-nvidia-channel-setup
test-nvidia-channel-setup: build/nvidia_channel_setup
	./build/nvidia_channel_setup

# v4.0 Phase N4 — THE ARC GATE. Pure-Cyrius NVIDIA compute dispatch:
# VM_INIT -> CHANNEL_ALLOC -> NVIF(0xC5C0) -> GEM_NEW/VM_BIND -> build QMD
# + pushbuffer -> EXEC -> verify the GPU wrote 0xDEADBEEF (twice on one
# channel). Requires nouveau hardware; not in CI. Pure Cyrius.
build/nvidia_compute_store: programs/nvidia_compute_store.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_compute_store.cyr $@

.PHONY: test-nvidia-compute-store
test-nvidia-compute-store: build/nvidia_compute_store
	./build/nvidia_compute_store

# v4.0 Phase N4.7 — the same 0xDEADBEEF compute dispatch, but driven through
# the PUBLIC mabda API (gpu_context_new_native_nvidia + gpu_buffer_* +
# gpu_shader_module_* + gpu_compute_dispatch), proving the v0 Backend slots
# are functionally wired. Requires nouveau hardware; not in CI. Pure Cyrius.
build/nvidia_compute_api: programs/nvidia_compute_api.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_compute_api.cyr $@

.PHONY: test-nvidia-compute-api
test-nvidia-compute-api: build/nvidia_compute_api
	./build/nvidia_compute_api

# v4.0 Phase N6 — NVIDIA texture create/write/read roundtrip through the
# PUBLIC mabda API (gpu_texture_*). Host-visible linear RGBA8; CPU roundtrip
# (GPU sampling is a later N6 bite). Requires nouveau hardware; not in CI.
build/nvidia_texture_e2e: programs/nvidia_texture_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_texture_e2e.cyr $@

.PHONY: test-nvidia-texture-e2e
test-nvidia-texture-e2e: build/nvidia_texture_e2e
	./build/nvidia_texture_e2e

# v4.0 Phase N6.2c — NVIDIA native GPU texture SAMPLING end-to-end. Binds a
# TIC/TSC descriptor pool, runs a Turing TEX (bound-texture) compute dispatch
# that samples a 1x1 RGBA8 texel and stores the packed result, reads it back.
# Proves the native sampling path (TIC/TSC pools + SET_TEX_*_POOL + TEX SASS).
# Requires nouveau hardware; not in CI.
build/nvidia_texture_sample_e2e: programs/nvidia_texture_sample_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_texture_sample_e2e.cyr $@

.PHONY: test-nvidia-texture-sample-e2e
test-nvidia-texture-sample-e2e: build/nvidia_texture_sample_e2e
	./build/nvidia_texture_sample_e2e

# v4.0 Phase N7.1 — NVIDIA native render-target create/release through the
# PUBLIC mabda API (v2 render slots 120/128). Allocates two live RTs (distinct
# VAs), checks geometry, proves the BO mapping backs memory. The GPU
# draw-into-it (TURING_A 3D pipeline) is N7.2-N7.4. Requires nouveau hardware;
# not in CI.
build/nvidia_render_target: programs/nvidia_render_target.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_render_target.cyr $@

.PHONY: test-nvidia-render-target
test-nvidia-render-target: build/nvidia_render_target
	./build/nvidia_render_target

# v4.0 Phase N7.4 — THE NVIDIA RENDER GATE. Creates the Turing 3D class
# (0xC597), binds a linear RGBA8 color target, runs a vertex-less
# fullscreen-triangle draw (VS from VertexID + solid-color FS, SM75 SPH+SASS),
# and reads the rendered pixel back. Pure-Cyrius clc597 draw, no libdrm/GFX.
# Requires nouveau hardware; not in CI.
build/nvidia_render_e2e: programs/nvidia_render_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_render_e2e.cyr $@

.PHONY: test-nvidia-render-e2e
test-nvidia-render-e2e: build/nvidia_render_e2e
	./build/nvidia_render_e2e

# v4.0 Phase N7.5 — NVIDIA native render through the PUBLIC gpu_render_* API
# (v2 render slots 136..168). Same triangle as the N7.4 gate, but driven via
# gpu_render_pipeline_* / gpu_render_pass_* — the backend-agnostic consumer
# surface. Requires nouveau hardware; not in CI.
build/nvidia_render_api: programs/nvidia_render_api.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_render_api.cyr $@

.PHONY: test-nvidia-render-api
test-nvidia-render-api: build/nvidia_render_api
	./build/nvidia_render_api

# 4.0.7 — NVIDIA multi-size BO allocator gate (the "multi-BO / bigger-BO
# surfaces" roadmap item): 1 MiB buffer roundtrip, GPU STG 512 KiB deep into
# one BO, 512x512 texture + render target through the public API, and a
# mixed-size create/release churn. Requires nouveau hardware; not in CI.
build/nvidia_bigbo_e2e: programs/nvidia_bigbo_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_bigbo_e2e.cyr $@

.PHONY: test-nvidia-bigbo-e2e
test-nvidia-bigbo-e2e: build/nvidia_bigbo_e2e
	./build/nvidia_bigbo_e2e

# v4.0 Phase N8.1 — NVIDIA KMS topology probe. Confirms nouveau exposes
# standard DRM atomic KMS on its card node and walks the display topology
# (connectors / encoders / CRTCs / preferred modes). GETRESOURCES/GETCONNECTOR
# work in any session (no DRM master); needs read perm on /dev/dri/cardN.
build/nvidia_kms_summary: programs/nvidia_kms_summary.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_kms_summary.cyr $@

.PHONY: test-nvidia-kms-summary
test-nvidia-kms-summary: build/nvidia_kms_summary
	./build/nvidia_kms_summary

# v4.0 Phase N8.2 — nouveau scanout FB. Allocates a linear VRAM BO on the
# render node, PRIME-bridges it onto the KMS card fd, and ADDFB2's an XRGB8888
# scanout framebuffer over it. ADDFB2 needs the card-node open but no DRM
# master. Requires nouveau hardware + card-node read perm; not in CI.
build/nvidia_kms_scanout: programs/nvidia_kms_scanout.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_kms_scanout.cyr $@

.PHONY: test-nvidia-kms-scanout
test-nvidia-kms-scanout: build/nvidia_kms_scanout
	./build/nvidia_kms_scanout

# v4.0 Phase N8.3 — nouveau LIVE MODESET. SETCRTC a red scanout FB onto the
# first connected connector — the screen turns red for 3s, then restores.
# **Run from a tty** (Ctrl-Alt-F2): SETCRTC needs DRM master, which the
# compositor holds in a desktop session. Requires nouveau hardware.
build/nvidia_kms_modeset: programs/nvidia_kms_modeset.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_kms_modeset.cyr $@

.PHONY: test-nvidia-kms-modeset
test-nvidia-kms-modeset: build/nvidia_kms_modeset
	./build/nvidia_kms_modeset

# v4.0 Phase N8.4 — nouveau ANIMATED PRESENT. Double-buffered, vsync'd
# PAGE_FLIP loop: the screen shows ~2s of a scrolling blue->red gradient, then
# the console is restored (GETCRTC save/restore). **Run from a tty** (needs DRM
# master). Requires nouveau hardware.
build/nvidia_present_e2e: programs/nvidia_present_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_present_e2e.cyr $@

.PHONY: test-nvidia-present-e2e
test-nvidia-present-e2e: build/nvidia_present_e2e
	./build/nvidia_present_e2e

# v4.0 Phase N8.5 — nouveau ANIMATED PRESENT through the PUBLIC surface API.
# Same on-screen result as N8.4 (scrolling blue->red gradient, ~2s, console
# restored) but driven through gpu_surface_configure_native_kiosk / acquire /
# present / release — the backend-agnostic v3 surface slots a compositor uses.
# **Run from a tty** (needs DRM master). Requires nouveau hardware.
build/nvidia_surface_present_e2e: programs/nvidia_surface_present_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/nvidia_surface_present_e2e.cyr $@

.PHONY: test-nvidia-surface-present-e2e
test-nvidia-surface-present-e2e: build/nvidia_surface_present_e2e
	./build/nvidia_surface_present_e2e

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
# shader, builds a PM4 stream, submits via DRM_IOCTL_AMDGPU_CS (3-chunk
# inline-BO path), waits on a sync-obj. Exits 0 iff the CP ran the IB
# (WRITE_DATA 0xCAFEBABE in stub[0]) AND the context reports no reset
# (a hung dispatch is completed by a ring reset, which the marker alone
# cannot see).
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

build/native_f64_fma_e2e: programs/native_f64_fma_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_f64_fma_e2e.cyr $@

.PHONY: test-native-f64-fma-e2e
test-native-f64-fma-e2e: build/native_f64_fma_e2e
	./build/native_f64_fma_e2e

# N.5d: a SPIR-V kernel compiled in-tree (gfx9_compile) and dispatched on the
# AMD GPU — the SPIR-V→GFX9 compiler's hardware bring-up oracle.
build/native_spirv_compute_e2e: programs/native_spirv_compute_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_compute_e2e.cyr $@

.PHONY: test-native-spirv-compute-e2e
test-native-spirv-compute-e2e: build/native_spirv_compute_e2e
	./build/native_spirv_compute_e2e

# N.6: a novel 2-binding SAXPY-shape kernel compiled in-tree + dispatched on the
# GPU (multi-binding dispatch — USER_DATA s0:s1=x, s2:s3=y).
build/native_spirv_saxpy_e2e: programs/native_spirv_saxpy_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_saxpy_e2e.cyr $@

.PHONY: test-native-spirv-saxpy-e2e
test-native-spirv-saxpy-e2e: build/native_spirv_saxpy_e2e
	./build/native_spirv_saxpy_e2e

# v3.2 F.7e — the first COMPILED f64 on silicon: an f64 FMA SPIR-V kernel compiled in-tree
# (gfx9_compile) + dispatched on Cezanne, bit-exact vs the fused-FMA reference.
build/native_spirv_f64_fma_e2e: programs/native_spirv_f64_fma_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_fma_e2e.cyr $@

.PHONY: test-native-spirv-f64-fma-e2e
test-native-spirv-f64-fma-e2e: build/native_spirv_f64_fma_e2e
	./build/native_spirv_f64_fma_e2e

# v3.2 F.7f — compiled f64 arith breadth (FADD/FMUL/FMIN/FMAX) on Cezanne, bit-exact vs an
# in-process f64 reference.
build/native_spirv_f64_arith_e2e: programs/native_spirv_f64_arith_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_arith_e2e.cyr $@

.PHONY: test-native-spirv-f64-arith-e2e
test-native-spirv-f64-arith-e2e: build/native_spirv_f64_arith_e2e
	./build/native_spirv_f64_arith_e2e

build/native_spirv_loop_e2e: programs/native_spirv_loop_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_loop_e2e.cyr $@

.PHONY: test-native-spirv-loop-e2e
test-native-spirv-loop-e2e: build/native_spirv_loop_e2e
	./build/native_spirv_loop_e2e

build/native_spirv_f32_div_e2e: programs/native_spirv_f32_div_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f32_div_e2e.cyr $@

.PHONY: test-native-spirv-f32-div-e2e
test-native-spirv-f32-div-e2e: build/native_spirv_f32_div_e2e
	./build/native_spirv_f32_div_e2e

build/native_spirv_f64_div_e2e: programs/native_spirv_f64_div_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_div_e2e.cyr $@

.PHONY: test-native-spirv-f64-div-e2e
test-native-spirv-f64-div-e2e: build/native_spirv_f64_div_e2e
	./build/native_spirv_f64_div_e2e

build/native_spirv_f64_sqrt_e2e: programs/native_spirv_f64_sqrt_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_sqrt_e2e.cyr $@

.PHONY: test-native-spirv-f64-sqrt-e2e
test-native-spirv-f64-sqrt-e2e: build/native_spirv_f64_sqrt_e2e
	./build/native_spirv_f64_sqrt_e2e

# v3.2 F.7f.2 — compiled f32<->f64 CVT round-trip on Cezanne, bit-exact vs an in-process reference.
build/native_spirv_f64_cvt_e2e: programs/native_spirv_f64_cvt_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_cvt_e2e.cyr $@

.PHONY: test-native-spirv-f64-cvt-e2e
test-native-spirv-f64-cvt-e2e: build/native_spirv_f64_cvt_e2e
	./build/native_spirv_f64_cvt_e2e

# N.5g: a 2x2 box-filter downsample compiled in-tree + dispatched on the GPU,
# pixel-matched against a CPU box-filter (the named MVP-exit image kernel; 2
# bindings src/dst, power-of-2 dims so index math is shifts/masks).
build/native_spirv_downsample_e2e: programs/native_spirv_downsample_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_downsample_e2e.cyr $@

.PHONY: test-native-spirv-downsample-e2e
test-native-spirv-downsample-e2e: build/native_spirv_downsample_e2e
	./build/native_spirv_downsample_e2e

# N.6r: a compiled SPIR-V kernel run through the PUBLIC gpu_* API end-to-end —
# gpu_shader_module_create_spirv (native slot compiles + stages the ISA) +
# gpu_compute_dispatch (consumes the compiled RSRC/bindings/LocalSize).
build/native_spirv_public_api_e2e: programs/native_spirv_public_api_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_public_api_e2e.cyr $@

.PHONY: test-native-spirv-public-api-e2e
test-native-spirv-public-api-e2e: build/native_spirv_public_api_e2e
	./build/native_spirv_public_api_e2e

# N.6: a compiled SPIR-V kernel with a 2-D/3-D workgroup grid + 2-D LocalSize via
# gl_GlobalInvocationId.x/.y (TGID_Y + TIDIG_COMP_CNT), HW-verified on Cezanne.
build/native_spirv_2d_dispatch_e2e: programs/native_spirv_2d_dispatch_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_2d_dispatch_e2e.cyr $@

.PHONY: test-native-spirv-2d-dispatch-e2e
test-native-spirv-2d-dispatch-e2e: build/native_spirv_2d_dispatch_e2e
	./build/native_spirv_2d_dispatch_e2e

# N.6: a compiled SPIR-V kernel dispatched on a LOGICAL COMPUTE QUEUE (the queue's
# persistent timeline), waited via gpu_queue_wait_idle. HW-verified on Cezanne.
build/native_spirv_queue_dispatch_e2e: programs/native_spirv_queue_dispatch_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_queue_dispatch_e2e.cyr $@

.PHONY: test-native-spirv-queue-dispatch-e2e
test-native-spirv-queue-dispatch-e2e: build/native_spirv_queue_dispatch_e2e
	./build/native_spirv_queue_dispatch_e2e

# N.7b: a compiled SPIR-V kernel with a UNIFORM `if (wgid.x==0)` — the s_cmp +
# s_cbranch_scc0 path. Grid 2 → workgroup 1 is gated out. HW-verified on Cezanne.
build/native_spirv_uniform_if_e2e: programs/native_spirv_uniform_if_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_uniform_if_e2e.cyr $@

.PHONY: test-native-spirv-uniform-if-e2e
test-native-spirv-uniform-if-e2e: build/native_spirv_uniform_if_e2e
	./build/native_spirv_uniform_if_e2e

# N.7c: a compiled SPIR-V kernel with a DIVERGENT `if (gid.x<4)` — v_cmp → VCC +
# s_and_saveexec_b64 + s_cbranch_execz + s_or_b64 restore. One wave, lanes 4-7 masked
# out of the store (out[4..7] untouched). HW-verified on Cezanne.
build/native_spirv_divergent_if_e2e: programs/native_spirv_divergent_if_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_divergent_if_e2e.cyr $@

.PHONY: test-native-spirv-divergent-if-e2e
test-native-spirv-divergent-if-e2e: build/native_spirv_divergent_if_e2e
	./build/native_spirv_divergent_if_e2e

# N.8a: a compiled SPIR-V kernel multiplying by a NON-INLINE constant (`gid.x*100`) —
# v_mul_lo_u32 (VOP3a) has no literal form, so 100 is materialized via v_mov_b32 into a
# scratch VGPR first. HW-verified on Cezanne (the documented N.6 VOP3-literal carry).
build/native_spirv_mul_literal_e2e: programs/native_spirv_mul_literal_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_mul_literal_e2e.cyr $@

.PHONY: test-native-spirv-mul-literal-e2e
test-native-spirv-mul-literal-e2e: build/native_spirv_mul_literal_e2e
	./build/native_spirv_mul_literal_e2e

# N.8b: a compiled SPIR-V kernel using a GLSL.std.450 OpExtInst (`max(float(gid.x),
# 4.0)`) — the ext-instruction front end → v_max_f32. HW-verified on Cezanne.
build/native_spirv_glsl_max_e2e: programs/native_spirv_glsl_max_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_glsl_max_e2e.cyr $@

.PHONY: test-native-spirv-glsl-max-e2e
test-native-spirv-glsl-max-e2e: build/native_spirv_glsl_max_e2e
	./build/native_spirv_glsl_max_e2e

# N.8b-2: a compiled SPIR-V kernel using a GLSL.std.450 Fma (ternary OpExtInst) →
# v_fma_f32 (VOP3 3-src). `fma(gid,gid,gid)` = gid*gid+gid. HW-verified on Cezanne.
build/native_spirv_fma_e2e: programs/native_spirv_fma_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_fma_e2e.cyr $@

.PHONY: test-native-spirv-fma-e2e
test-native-spirv-fma-e2e: build/native_spirv_fma_e2e
	./build/native_spirv_fma_e2e

# N.8b-3: a compiled SPIR-V kernel using f32 INLINE constants (`fma(gid, 2.0, 1.0)`) —
# 2.0/1.0 pack into the VOP3 source fields (codes 244/242), no literal. HW-verified.
build/native_spirv_fma_const_e2e: programs/native_spirv_fma_const_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_fma_const_e2e.cyr $@

.PHONY: test-native-spirv-fma-const-e2e
test-native-spirv-fma-const-e2e: build/native_spirv_fma_const_e2e
	./build/native_spirv_fma_const_e2e

# N.8b-4: GLSL.std.450 FClamp via v_med3_f32 (median-of-3). clamp(float(gid.x),1.0,4.0).
build/native_spirv_fclamp_e2e: programs/native_spirv_fclamp_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_fclamp_e2e.cyr $@

.PHONY: test-native-spirv-fclamp-e2e
test-native-spirv-fclamp-e2e: build/native_spirv_fclamp_e2e
	./build/native_spirv_fclamp_e2e

# N.8b-4: a SIGNED compare (v_cmp_lt_i32) — `if (int(gid.x)-4 < 0)` selects gid 0..3
# (an unsigned compare would select none). HW-verified on Cezanne.
build/native_spirv_signed_if_e2e: programs/native_spirv_signed_if_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_signed_if_e2e.cyr $@

.PHONY: test-native-spirv-signed-if-e2e
test-native-spirv-signed-if-e2e: build/native_spirv_signed_if_e2e
	./build/native_spirv_signed_if_e2e

# N.8b-5: storing a CONSTANT value (`out[gid.x] = 0xCAFE`) — FLAT store data must be a
# VGPR, so the constant is materialized via v_mov_b32 first. HW-verified on Cezanne.
build/native_spirv_store_const_e2e: programs/native_spirv_store_const_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_store_const_e2e.cyr $@

.PHONY: test-native-spirv-store-const-e2e
test-native-spirv-store-const-e2e: build/native_spirv_store_const_e2e
	./build/native_spirv_store_const_e2e

# N.8b-6: a binary VOP2 op with a constant operand (`float(gid.x) * 2.0`) — the const
# rides src0 (inline float) and the VGPR rides vsrc1. HW-verified on Cezanne.
build/native_spirv_vop2_const_e2e: programs/native_spirv_vop2_const_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_vop2_const_e2e.cyr $@

.PHONY: test-native-spirv-vop2-const-e2e
test-native-spirv-vop2-const-e2e: build/native_spirv_vop2_const_e2e
	./build/native_spirv_vop2_const_e2e

# N.8b-7: GLSL.std.450 FAbs via v_and_b32 0x7FFFFFFF (clear the f32 sign bit).
# `abs(float(gid.x) - 4.0)`. HW-verified on Cezanne.
build/native_spirv_fabs_e2e: programs/native_spirv_fabs_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_fabs_e2e.cyr $@

.PHONY: test-native-spirv-fabs-e2e
test-native-spirv-fabs-e2e: build/native_spirv_fabs_e2e
	./build/native_spirv_fabs_e2e

# N.8b-8: a two-constant op const-folded at compile time (`gid.x + 6*7` → `gid.x + 42`).
# HW-verified on Cezanne.
build/native_spirv_const_fold_e2e: programs/native_spirv_const_fold_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_const_fold_e2e.cyr $@

.PHONY: test-native-spirv-const-fold-e2e
test-native-spirv-const-fold-e2e: build/native_spirv_const_fold_e2e
	./build/native_spirv_const_fold_e2e

# N.9b-2: u32 OpUDiv via the float-reciprocal macro (GFX9 has no integer divide).
# `out[gid] = a[gid] / b[gid]` over an edge matrix incl. b=0. HW-verified on Cezanne.
build/native_spirv_udiv_e2e: programs/native_spirv_udiv_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_udiv_e2e.cyr $@

.PHONY: test-native-spirv-udiv-e2e
test-native-spirv-udiv-e2e: build/native_spirv_udiv_e2e
	./build/native_spirv_udiv_e2e

# N.9c: u32 OpUMod via the udiv core (remainder select). `out[gid] = a[gid] % b[gid]`
# over an edge matrix incl. b=0 (→ N). HW-verified on Cezanne.
build/native_spirv_umod_e2e: programs/native_spirv_umod_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_umod_e2e.cyr $@

.PHONY: test-native-spirv-umod-e2e
test-native-spirv-umod-e2e: build/native_spirv_umod_e2e
	./build/native_spirv_umod_e2e

# N.9d: signed OpSDiv / OpSRem (sign-magnitude over the udiv core). HW-verified on Cezanne.
build/native_spirv_sdiv_e2e: programs/native_spirv_sdiv_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_sdiv_e2e.cyr $@

.PHONY: test-native-spirv-sdiv-e2e
test-native-spirv-sdiv-e2e: build/native_spirv_sdiv_e2e
	./build/native_spirv_sdiv_e2e

build/native_spirv_srem_e2e: programs/native_spirv_srem_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_srem_e2e.cyr $@

.PHONY: test-native-spirv-srem-e2e
test-native-spirv-srem-e2e: build/native_spirv_srem_e2e
	./build/native_spirv_srem_e2e

build/native_spirv_smod_e2e: programs/native_spirv_smod_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_smod_e2e.cyr $@

.PHONY: test-native-spirv-smod-e2e
test-native-spirv-smod-e2e: build/native_spirv_smod_e2e
	./build/native_spirv_smod_e2e

build/native_spirv_vector_add_e2e: programs/native_spirv_vector_add_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_vector_add_e2e.cyr $@

.PHONY: test-native-spirv-vector-add-e2e
test-native-spirv-vector-add-e2e: build/native_spirv_vector_add_e2e
	./build/native_spirv_vector_add_e2e

build/native_spirv_vector_load_store_e2e: programs/native_spirv_vector_load_store_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_vector_load_store_e2e.cyr $@

.PHONY: test-native-spirv-vector-load-store-e2e
test-native-spirv-vector-load-store-e2e: build/native_spirv_vector_load_store_e2e
	./build/native_spirv_vector_load_store_e2e

build/native_spirv_vector_const_e2e: programs/native_spirv_vector_const_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_vector_const_e2e.cyr $@

.PHONY: test-native-spirv-vector-const-e2e
test-native-spirv-vector-const-e2e: build/native_spirv_vector_const_e2e
	./build/native_spirv_vector_const_e2e

# v3.2 T.8 — native block-compressed texture STORAGE round-trip (BC1 + BC7
# write -> read byte-identical on Cezanne; block-aware n guard). HW-gated.
build/native_compressed_store_e2e: programs/native_compressed_store_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_compressed_store_e2e.cyr $@

.PHONY: test-native-compressed-store
test-native-compressed-store: build/native_compressed_store_e2e
	./build/native_compressed_store_e2e

# v3.1 Q.3c — native multi-queue compute: dispatch on a logical COMPUTE
# queue (async, timeline-signalled), wait via gpu_queue_wait_idle, verify
# 0xDEADBEEF; second dispatch proves the persistent timeline (point 1->2).
# HW-gated (requires the AMD render node).
build/native_queue_compute_e2e: programs/native_queue_compute_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_queue_compute_e2e.cyr $@

.PHONY: test-native-queue-compute-e2e
test-native-queue-compute-e2e: build/native_queue_compute_e2e
	./build/native_queue_compute_e2e

# v3.1 Q.4 — cross-ring barrier: compute (COMPUTE ring) -> gpu_queue_barrier
# -> compute on the GRAPHICS queue (GFX ring) whose submit carries an in-CS
# SYNCOBJ_TIMELINE_WAIT on the compute point. Proves the kernel accepts +
# completes a CS with a timeline-wait chunk. HW-gated.
build/native_queue_barrier_e2e: programs/native_queue_barrier_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_queue_barrier_e2e.cyr $@

.PHONY: test-native-queue-barrier-e2e
test-native-queue-barrier-e2e: build/native_queue_barrier_e2e
	./build/native_queue_barrier_e2e

# v3.1 Q.5 — SDMA COPY_LINEAR on the DMA ring (AMDGPU_HW_IP_DMA): copy a
# 4 KiB page src->dst and verify byte-identical. Proves the SDMA packet
# format + DMA-ring submit on Cezanne. HW-gated. (The TRANSFER queue's
# DMA-ring flip + public copy API land in 3.1.2; this is the foundation.)
build/native_sdma_copy_e2e: programs/native_sdma_copy_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_sdma_copy_e2e.cyr $@

.PHONY: test-native-sdma-copy-e2e
test-native-sdma-copy-e2e: build/native_sdma_copy_e2e
	./build/native_sdma_copy_e2e

# v3.1 Q.6 — headline multi-queue demo: compute (COMPUTE ring) -> barrier
# -> graphics (GFX ring) + SDMA consume (DMA ring), all three rings
# timeline-ordered, every result CPU-verified. HW-gated.
build/native_multiqueue_e2e: programs/native_multiqueue_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_multiqueue_e2e.cyr $@

.PHONY: test-native-multiqueue-e2e
test-native-multiqueue-e2e: build/native_multiqueue_e2e
	./build/native_multiqueue_e2e

# v3.2 TS.5 — native RGBA8 sampling MVP: T#/image_load sample a texture across
# a fullscreen quad, verify RT[x,y]==tex[x,y]. HW-gated (AMD render node).
build/native_texture_sample_e2e: programs/native_texture_sample_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_texture_sample_e2e.cyr $@

.PHONY: test-native-texture-sample-e2e
test-native-texture-sample-e2e: build/native_texture_sample_e2e
	./build/native_texture_sample_e2e

# ⚠ -D MABDA_PNG is REQUIRED: gpu_texture_load_png is opt-in (it pulls
# [deps.chitra] + thread/sankoch), so without the flag it compiles out and this
# gate fails with a bare "FAIL: load_png" that looks like a decoder bug. The flag
# was missing from this recipe from the target's introduction until v4.0.11, when
# `make test-native-all` surfaced the gate as red — it had simply never been run.
build/native_load_png_e2e: programs/native_load_png_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build -D MABDA_PNG programs/native_load_png_e2e.cyr $@

.PHONY: test-native-load-png-e2e
test-native-load-png-e2e: build/native_load_png_e2e
	./build/native_load_png_e2e

build/native_array_store_e2e: programs/native_array_store_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_array_store_e2e.cyr $@

.PHONY: test-native-array-store-e2e
test-native-array-store-e2e: build/native_array_store_e2e
	./build/native_array_store_e2e

build/native_array_sample_e2e: programs/native_array_sample_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_array_sample_e2e.cyr $@

.PHONY: test-native-array-sample-e2e
test-native-array-sample-e2e: build/native_array_sample_e2e
	./build/native_array_sample_e2e

build/native_cube_store_e2e: programs/native_cube_store_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_cube_store_e2e.cyr $@

.PHONY: test-native-cube-store-e2e
test-native-cube-store-e2e: build/native_cube_store_e2e
	./build/native_cube_store_e2e

build/native_cube_sample_e2e: programs/native_cube_sample_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_cube_sample_e2e.cyr $@

.PHONY: test-native-cube-sample-e2e
test-native-cube-sample-e2e: build/native_cube_sample_e2e
	./build/native_cube_sample_e2e

build/native_array_cube_load_e2e: programs/native_array_cube_load_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_array_cube_load_e2e.cyr $@

.PHONY: test-native-array-cube-load-e2e
test-native-array-cube-load-e2e: build/native_array_cube_load_e2e
	./build/native_array_cube_load_e2e

# v3.4.1 AA.4 — native BC1 compressed/tiled array: per-slice L2T + array image_sample.
build/native_bc_array_e2e: programs/native_bc_array_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_bc_array_e2e.cyr $@

.PHONY: test-native-bc-array-e2e
test-native-bc-array-e2e: build/native_bc_array_e2e
	./build/native_bc_array_e2e

# v3.2 TS.6 — SDMA tiling probe: L2T->T2L round-trip proves the SW_64KB_S
# COPY_TILED_SUB_WINDOW path works on Cezanne. HW-gated.
build/native_sdma_tiled_roundtrip: programs/native_sdma_tiled_roundtrip.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_sdma_tiled_roundtrip.cyr $@

.PHONY: test-native-sdma-tiled-roundtrip
test-native-sdma-tiled-roundtrip: build/native_sdma_tiled_roundtrip
	./build/native_sdma_tiled_roundtrip

# v3.2 TS.7c-3 — tiled BC1 texture write(L2T)/read(T2L) round-trip through the
# wired public gpu_texture_* path on a non-block_w-aligned surface. HW-gated.
build/native_tiled_texture_roundtrip: programs/native_tiled_texture_roundtrip.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_tiled_texture_roundtrip.cyr $@

.PHONY: test-native-tiled-texture-roundtrip
test-native-tiled-texture-roundtrip: build/native_tiled_texture_roundtrip
	./build/native_tiled_texture_roundtrip

# v3.2 TS.7c-4 — sample a TILED BC1 texture in an FS and verify the TA decode
# pixel-exact vs a CPU decode (where SDMA tiling meets the texture unit). HW-gated.
build/native_compressed_sample_e2e: programs/native_compressed_sample_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_compressed_sample_e2e.cyr $@

.PHONY: test-native-compressed-sample-e2e
test-native-compressed-sample-e2e: build/native_compressed_sample_e2e
	./build/native_compressed_sample_e2e

# v3.2 TS.8b — observable bilinear: a small texture over a larger RT (scale<1),
# POINT (exact texels) vs BILINEAR (blends). HW-gated.
build/native_bilinear_sample_e2e: programs/native_bilinear_sample_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_bilinear_sample_e2e.cyr $@

.PHONY: test-native-bilinear-sample-e2e
test-native-bilinear-sample-e2e: build/native_bilinear_sample_e2e
	./build/native_bilinear_sample_e2e

# v3.2 X.7 — public buffer-copy e2e: gpu_buffer_copy round-trip + compute
# -> barrier -> gpu_queue_transfer_copy consume on the SDMA ring, every
# result CPU-verified. HW-gated (needs an AMD render node).
build/native_transfer_copy_e2e: programs/native_transfer_copy_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_transfer_copy_e2e.cyr $@

.PHONY: test-native-transfer-copy-e2e
test-native-transfer-copy-e2e: build/native_transfer_copy_e2e
	./build/native_transfer_copy_e2e

# v3 rc.2 — radv_capture Phase 2 helper. Builds the same PM4 stream
# that the live compute_store dispatch produces, but writes the
# dword stream to stdout instead of submitting it. CI-safe (no GPU
# access). Pair with programs/diagnostics/radv_capture/Makefile's
# `compare` target to byte-diff against RADV's --dump=ibs output.
build/native_pm4_dump: programs/native_pm4_dump.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_pm4_dump.cyr $@

.PHONY: dump-native-pm4
dump-native-pm4: build/native_pm4_dump
	./build/native_pm4_dump

build/native_texture_e2e: programs/native_texture_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_texture_e2e.cyr $@

.PHONY: test-native-texture-e2e
test-native-texture-e2e: build/native_texture_e2e
	./build/native_texture_e2e

# v3.1 M.7 — native mipmap generation e2e. Creates a mipped texture,
# writes level 0, GPU-downsamples the chain, verifies each level against a
# CPU box-filter reference. Requires amdgpu render node; renderD128 only
# (no DRM master), so it runs in any session.
build/native_mipmap_e2e: programs/native_mipmap_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_mipmap_e2e.cyr $@

.PHONY: test-native-mipmap-e2e
test-native-mipmap-e2e: build/native_mipmap_e2e
	./build/native_mipmap_e2e

# v3 Step 6.9(b) — native render end-to-end (mirror of render_e2e
# on the native graphics ring). HW-gated; runs the full 6.x chain
# (shader bytes → PM4 composer → GFX dispatch → CPU readback).
build/native_render_e2e: programs/native_render_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_render_e2e.cyr $@

.PHONY: test-native-render-e2e
test-native-render-e2e: build/native_render_e2e
	./build/native_render_e2e

# v3.4.2 RT allocator regression — odd-window (1260x682) RT exercises the
# 64 KiB va_map rounding, a 2nd live RT exercises the per-context VA bump.
# HW-gated (needs the amdgpu render node; not master-gated).
build/native_rt_alloc_e2e: programs/native_rt_alloc_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_rt_alloc_e2e.cyr $@

.PHONY: test-native-rt-alloc-e2e
test-native-rt-alloc-e2e: build/native_rt_alloc_e2e
	./build/native_rt_alloc_e2e

# v3.4.3 va_map 64 KiB sweep — odd-window (1260x682) texture + non-64KiB buffer +
# mipped chain over the public API; pre-fix these EINVAL'd. HW-gated (amdgpu
# render node; not master-gated).
build/native_texture_alloc_e2e: programs/native_texture_alloc_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_texture_alloc_e2e.cyr $@

.PHONY: test-native-texture-alloc-e2e
test-native-texture-alloc-e2e: build/native_texture_alloc_e2e
	./build/native_texture_alloc_e2e

build/native_render_graph_mq_e2e: programs/native_render_graph_mq_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_render_graph_mq_e2e.cyr $@

.PHONY: test-native-render-graph-mq-e2e
test-native-render-graph-mq-e2e: build/native_render_graph_mq_e2e
	./build/native_render_graph_mq_e2e

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

# v3 Step 7.7 — Phase D end-to-end. 120-frame double-buffered
# animated present using the v3 public surface API. Requires DRM
# master — run from a tty (Ctrl-Alt-F2 + stop the compositor)
# (this program uses the kiosk path). The in-session alternative is
# gpu_surface_configure_native_logind: -D MABDA_LOGIND plus an initialized samvada.
build/native_present_e2e: programs/native_present_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_present_e2e.cyr $@

.PHONY: test-native-present-e2e
test-native-present-e2e: build/native_present_e2e
	./build/native_present_e2e

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
# (13 benches). Reports human-readable lines plus one `CSV:name,ns` row per
# benchmark. No in-tree script appends those rows to bench-history.csv
# (timestamp,commit,branch,benchmark,estimate_ns): the Rust-era
# scripts/bench-history.sh that appended criterion results to it moved out of the
# tree in 08f6307. (Until 4.1.3 this comment named scripts/bench-record.sh, which
# never existed.)
.PHONY: bench-gpu
bench-gpu: build/benchmarks
	./build/benchmarks

# Developer gate: run every GPU integration program in sequence.
.PHONY: test-gpu
test-gpu: test-phase0 test-compute-e2e test-render-e2e test-render-graph-e2e

# ---------------------------------------------------------------------------
# test-native-all — run every `test-native-*` gate and report a TALLY.
#
# ⛔ WHY THIS EXISTS: `test-native-spirv-saxpy-e2e` was RED from 4.0.5 through
# 4.0.10 and nobody noticed, because it is a standalone target belonging to no
# aggregate. There are 71 `test-native-*` targets and, before this, no way to ask
# "does the native HW suite pass?" in one command — so the honest answer was
# always "whichever ones I happened to run".
#
# Keeps going after a failure (that is the point — one red gate must not hide the
# state of the other 70) and exits non-zero if any failed, naming each.
#
# Requires AMD hardware (/dev/dri/renderD128). CI cannot run this; it is a
# developer gate, which is exactly why it needs to be one command.
# GNU make runs this recipe even under `make -n` / `-t` / `-q` (it references $(MAKE)),
# where every inner gate would "pass" (-n, -t) or "fail" (-q) without running, so the
# roll-up starts with $(RECURSION_GUARD) instead of printing a tally of gates that
# never ran (see RECURSION_GUARD and `make check-make-dry-run`).
.PHONY: test-native-all
test-native-all:
	@$(RECURSION_GUARD) \
	targets=$$(grep -oE '^test-native[a-z0-9-]*:' $(MAKEFILE_LIST) | sed 's/://' | sort -u | grep -v '^test-native-all$$'); \
	total=0; pass=0; failed=""; skipped=""; known=""; \
	log=$$(mktemp -t mabda-native-XXXXXX.log); \
	for t in $$targets; do \
		total=$$((total+1)); \
		printf '%-46s ' "$$t"; \
		case " $(NATIVE_NEEDS_MASTER) " in *" $$t "*) \
			printf 'SKIP  (needs DRM master)\n'; skipped="$$skipped $$t"; continue;; esac; \
		case " $(NATIVE_KNOWN_FAIL) " in *" $$t "*) \
			printf 'KNOWN-FAIL (filed - see docs/development/issues/)\n'; known="$$known $$t"; continue;; esac; \
		if $(MAKE) --no-print-directory $$t > "$$log" 2>&1; then \
			printf 'PASS\n'; pass=$$((pass+1)); \
		else \
			printf 'FAIL\n'; failed="$$failed $$t"; \
			tail -3 "$$log" | sed 's/^/      /'; \
		fi; \
	done; \
	rm -f "$$log"; \
	echo; \
	echo "native HW gates: $$pass passed, $$(echo $$failed | wc -w) failed, \
$$(echo $$skipped | wc -w) skipped, $$(echo $$known | wc -w) known-fail  (of $$total)"; \
	if [ -n "$$skipped" ]; then echo "SKIPPED (run from a tty/kiosk session, or wire samvada+logind):$$skipped"; fi; \
	if [ -n "$$known" ]; then echo "KNOWN-FAIL:$$known"; fi; \
	if [ -n "$$failed" ]; then echo "FAILED:$$failed"; exit 1; fi

# Gates that cannot pass in an ordinary desktop session because the compositor
# holds DRM master. NOT hidden — the roll-up prints them as SKIP with the reason,
# because a test you deleted is worse than a skip you can read.
NATIVE_NEEDS_MASTER = test-native-kms-modeset test-native-present-e2e

# Gates failing for a filed, understood reason. Every entry MUST have an issue
# file; this list is not a place to park inconvenient red. It is empty when no
# gate is in that state — keep the variable defined (the roll-up reads it).
#   (test-native-compute-spike was listed here until 4.1.3; repaired, see
#    docs/development/issues/2026-08-19-native-compute-spike-stale.md)
NATIVE_KNOWN_FAIL =

# CI gate: syntax + semantic check every programs/*.cyr without needing
# wgpu-native on the runner. Fails on any cyrius warning/error coming from
# mabda-owned source (programs/ or src/). Warnings whose path begins with
# `lib/` originate in the cyrius stdlib and are filtered out — they are
# tracked upstream, not here.
# Closes the Issue-2-class bug (missing includes compiling silently) from
# docs/issues/2026-04-19-phase0-build-broken.md.
.PHONY: build-gpu-programs
build-gpu-programs: check-program-write-lengths
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

# v3.2 F.7f.3 — compiled f64 sub+abs (source-modifier ops) on Cezanne, bit-exact vs in-process ref.
build/native_spirv_f64_subabs_e2e: programs/native_spirv_f64_subabs_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_subabs_e2e.cyr $@

.PHONY: test-native-spirv-f64-subabs-e2e
test-native-spirv-f64-subabs-e2e: build/native_spirv_f64_subabs_e2e
	./build/native_spirv_f64_subabs_e2e

# v3.2 F.7f.4 — compiled array<dvec2> add on Cezanne (verifies the F.7e f64 vec stride/offset).
build/native_spirv_f64_vec_e2e: programs/native_spirv_f64_vec_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_vec_e2e.cyr $@

.PHONY: test-native-spirv-f64-vec-e2e
test-native-spirv-f64-vec-e2e: build/native_spirv_f64_vec_e2e
	./build/native_spirv_f64_vec_e2e

build/native_spirv_f64_i32_cvt_e2e: programs/native_spirv_f64_i32_cvt_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_i32_cvt_e2e.cyr $@

.PHONY: test-native-spirv-f64-i32-cvt-e2e
test-native-spirv-f64-i32-cvt-e2e: build/native_spirv_f64_i32_cvt_e2e
	./build/native_spirv_f64_i32_cvt_e2e

build/native_spirv_f64_const_e2e: programs/native_spirv_f64_const_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_const_e2e.cyr $@

.PHONY: test-native-spirv-f64-const-e2e
test-native-spirv-f64-const-e2e: build/native_spirv_f64_const_e2e
	./build/native_spirv_f64_const_e2e

build/native_spirv_f64_ldexp_e2e: programs/native_spirv_f64_ldexp_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_ldexp_e2e.cyr $@

.PHONY: test-native-spirv-f64-ldexp-e2e
test-native-spirv-f64-ldexp-e2e: build/native_spirv_f64_ldexp_e2e
	./build/native_spirv_f64_ldexp_e2e

build/native_spirv_f64_select_e2e: programs/native_spirv_f64_select_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_select_e2e.cyr $@

.PHONY: test-native-spirv-f64-select-e2e
test-native-spirv-f64-select-e2e: build/native_spirv_f64_select_e2e
	./build/native_spirv_f64_select_e2e

build/native_spirv_f64_layernorm_e2e: programs/native_spirv_f64_layernorm_e2e.cyr src/*.cyr
	@mkdir -p build
	$(CYRIUS) build programs/native_spirv_f64_layernorm_e2e.cyr $@

.PHONY: test-native-spirv-f64-layernorm-e2e
test-native-spirv-f64-layernorm-e2e: build/native_spirv_f64_layernorm_e2e
	./build/native_spirv_f64_layernorm_e2e
