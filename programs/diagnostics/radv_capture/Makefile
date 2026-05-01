CC      ?= cc
CFLAGS  ?= -O2 -Wall -Wextra -Wno-unused-parameter
LIBS    := -lvulkan
GLSLANG ?= glslangValidator

# Mabda paths (this Makefile is at programs/diagnostics/radv_capture/;
# the mabda repo root is three levels up).
MABDA_ROOT := ../../..
MABDA_DUMP := $(MABDA_ROOT)/build/native_pm4_dump
EXTRACT    := ./extract_dispatch.sh

.PHONY: all run dump compare diff-tail clean

all: radv_capture shader.spv

radv_capture: main.c
	$(CC) $(CFLAGS) -o $@ main.c $(LIBS)

shader.spv: shader.comp
	$(GLSLANG) -V $< -o $@

# Run the dispatch + readback. Exits 0 if slot[0] == 0xDEADBEEF.
run: all
	./radv_capture

# Run with RADV_DEBUG=ibs so Mesa's RADV driver dumps the indirect
# buffer contents to stderr. Capture into radv.ib.txt for byte-diff
# against mabda's PM4 composer output.
dump: all
	RADV_DEBUG=ibs ./radv_capture 2>radv.ib.txt 1>radv.stdout.txt
	@echo "wrote radv.ib.txt ($$(wc -l < radv.ib.txt) lines)"
	@echo "  next: 'make compare' to diff against mabda"

# Phase 2 — byte-diff reduction. Builds the mabda PM4 dumper if not
# already built, runs both extractors over the RADV + mabda dumps,
# and shows the diff (focused on compute-state register writes +
# DISPATCH_DIRECT). RADV's full IB is much longer than mabda's
# (init/state/barrier preamble), so the extractor filters down to
# the compute-relevant packets — that's where actual semantic
# divergence shows up.
$(MABDA_DUMP):
	$(MAKE) -C $(MABDA_ROOT) build/native_pm4_dump

mabda.dump.txt: $(MABDA_DUMP)
	$(MABDA_DUMP) > $@

mabda.tail.txt: mabda.dump.txt
	$(EXTRACT) < $< > $@

radv.tail.txt: radv.ib.txt
	$(EXTRACT) < $< > $@

compare: mabda.tail.txt radv.tail.txt
	@echo "=== compute-state diff (RADV vs mabda) ==="
	@diff -u radv.tail.txt mabda.tail.txt || true
	@echo
	@echo "=== mabda compute-state tail ==="
	@cat mabda.tail.txt

# Convenience: just dump mabda's PM4 tail without running RADV.
diff-tail: mabda.tail.txt
	@cat mabda.tail.txt

clean:
	rm -f radv_capture shader.spv radv.ib.txt radv.stdout.txt \
	      mabda.dump.txt mabda.tail.txt radv.tail.txt
