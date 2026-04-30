CC      ?= cc
CFLAGS  ?= -O2 -Wall -Wextra -Wno-unused-parameter
LIBS    := -lvulkan
GLSLANG ?= glslangValidator

.PHONY: all run dump clean

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
	@echo "  compare against mabda: see README.md > Compare workflow"

clean:
	rm -f radv_capture shader.spv radv.ib.txt radv.stdout.txt
