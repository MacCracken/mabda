CC      ?= cc
CFLAGS  ?= -O2 -Wall -Wextra -Wno-unused-parameter
LIBS    := -lvulkan
GLSLANG ?= glslangValidator

MABDA_ROOT := ../../..
MABDA_DUMP := $(MABDA_ROOT)/build/dump_render_pm4

.PHONY: all run dump snoop snoop-decode compare clean

all: radv_capture_triangle triangle.vert.spv triangle.frag.spv

radv_capture_triangle: main.c
	$(CC) $(CFLAGS) -o $@ main.c $(LIBS)

triangle.vert.spv: triangle.vert
	$(GLSLANG) -V $< -o $@

triangle.frag.spv: triangle.frag
	$(GLSLANG) -V $< -o $@

# Run the draw + readback. Exits 0 only if EVERY pixel of the 256x256 RT is
# (0xFF,0x00,0x00,0xFF).
run: all
	./radv_capture_triangle

# Run with RADV_DEBUG=dumpibs so the Mesa RADV driver dumps the indirect
# buffer contents to stderr. Capture into radv-triangle.ib.txt for
# byte-diff against mabda's render PM4 composer output.
dump: all
	RADV_DEBUG=dumpibs ./radv_capture_triangle 2>radv-triangle.ib.txt 1>radv-triangle.stdout.txt
	@echo "wrote radv-triangle.ib.txt ($$(wc -l < radv-triangle.ib.txt) lines)"
	@echo "  next: 'make compare' to byte-diff against mabda"

# `make dump` (RADV_DEBUG=dumpibs) prints radv's preamble IB but NOT the IB it
# jumps to: radv's gfx_init state (CONTEXT_CONTROL + CLEAR_STATE + the
# radv_emit_graphics registers such as PA_CL_VTE_CNTL). `make snoop` LD_PRELOADs
# a read-only amdgpu CS snoop that saves EVERY submitted IB, following nested
# INDIRECT_BUFFER packets, as snoop/cs<seq>_<chunk>[-ibN]_ip<ip>_fl<flags>_<va>.bin
# (+ snoop/index.txt). `make snoop-decode GFX9_JSON=/path/gfx9.json` decodes them.
ibsnoop.so: ibsnoop.c
	$(CC) -O1 -Wall -shared -fPIC -o $@ ibsnoop.c -ldl -lpthread

snoop: all ibsnoop.so
	rm -rf snoop && mkdir -p snoop
	IBSNOOP_DIR=$$PWD/snoop LD_PRELOAD=$$PWD/ibsnoop.so ./radv_capture_triangle
	@cat snoop/index.txt

GFX9_JSON ?= gfx9.json
snoop-decode:
	@for f in snoop/*.bin; do echo "=== $$f"; GFX9_JSON=$(GFX9_JSON) python3 decode_ib.py $$f; done

$(MABDA_DUMP):
	$(MAKE) -C $(MABDA_ROOT) build/dump_render_pm4

mabda-triangle.dump.txt: $(MABDA_DUMP)
	$(MABDA_DUMP) > $@

compare: radv-triangle.ib.txt mabda-triangle.dump.txt
	@echo "=== radv triangle IB head ==="
	@head -80 radv-triangle.ib.txt
	@echo
	@echo "=== mabda triangle PM4 head ==="
	@head -60 mabda-triangle.dump.txt

# Removes only untracked outputs. radv_capture_triangle, triangle.*.spv,
# radv-triangle.ib.txt and radv-triangle.stdout.txt are tracked reference
# artifacts (the ib/stdout pair is the `make dump` capture) and are never
# deleted by a make target; rebuild them explicitly (`make -B all`, `make dump`).
clean:
	rm -f mabda-triangle.dump.txt ibsnoop.so
	rm -rf snoop
