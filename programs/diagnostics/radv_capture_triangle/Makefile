CC      ?= cc
CFLAGS  ?= -O2 -Wall -Wextra -Wno-unused-parameter
LIBS    := -lvulkan
GLSLANG ?= glslangValidator

MABDA_ROOT := ../../..
MABDA_DUMP := $(MABDA_ROOT)/build/dump_render_pm4

.PHONY: all run dump compare clean

all: radv_capture_triangle triangle.vert.spv triangle.frag.spv

radv_capture_triangle: main.c
	$(CC) $(CFLAGS) -o $@ main.c $(LIBS)

triangle.vert.spv: triangle.vert
	$(GLSLANG) -V $< -o $@

triangle.frag.spv: triangle.frag
	$(GLSLANG) -V $< -o $@

# Run the draw + readback. Exits 0 if pixel(0,0) == (0xFF,0x00,0x00,0xFF).
run: all
	./radv_capture_triangle

# Run with RADV_DEBUG=dumpibs so the Mesa RADV driver dumps the indirect
# buffer contents to stderr. Capture into radv-triangle.ib.txt for
# byte-diff against mabda's render PM4 composer output.
dump: all
	RADV_DEBUG=dumpibs ./radv_capture_triangle 2>radv-triangle.ib.txt 1>radv-triangle.stdout.txt
	@echo "wrote radv-triangle.ib.txt ($$(wc -l < radv-triangle.ib.txt) lines)"
	@echo "  next: 'make compare' to byte-diff against mabda"

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

clean:
	rm -f radv_capture_triangle triangle.vert.spv triangle.frag.spv \
	      radv-triangle.ib.txt radv-triangle.stdout.txt \
	      mabda-triangle.dump.txt
