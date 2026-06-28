#!/bin/bash
# disasm-sass.sh — one-directional verify oracle for mabda's embedded
# SM75 SASS (the NVIDIA analogue of scripts/disasm-shaders.sh).
#
# NVIDIA publishes no open assembler and nvdisasm is disassemble-only, so
# the oracle is one-way: take the dwords embedded in
# src/backend_nvidia_sass.cyr, lay them out as little-endian bytes, and
# disassemble with `nvdisasm -b SM75`. A single wrong bit in a captured
# (or, later, hand-encoded) instruction shows up here as a decode error
# or a different mnemonic than the source comment.
#
# Keep the dwords below IN SYNC with native_nv_sass_store_deadbeef in
# src/backend_nvidia_sass.cyr. Needs the CUDA toolkit (nvdisasm) on PATH
# or at /opt/cuda/bin; build-time only, no GPU required.
set -eu

NVDISASM="$(command -v nvdisasm || echo /opt/cuda/bin/nvdisasm)"
if [ ! -x "$NVDISASM" ]; then
    echo "nvdisasm not found (install: sudo pacman -S cuda)"; exit 2
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
BIN="$TMP/sass.bin"

# Each 128-bit instruction = two little-endian u64 [opcode_word, control_word],
# memory order. Mirror of native_nv_sass_store_deadbeef.
u64s_to_bin() {
    python3 - "$@" <<'PYEOF'
import sys, struct
out = bytearray()
for h in sys.argv[1:]:
    out += struct.pack('<Q', int(h, 16))
sys.stdout.buffer.write(out)
PYEOF
}

echo "=== store_deadbeef (SM75, 128 bytes / 8 instructions) ==="
u64s_to_bin \
    0x00000a0000017a02 0x000fe40000000f00 \
    0xdeadbeef00007802 0x000fe20000000f00 \
    0x0000580000047ab9 0x000fce0000000a00 \
    0x00000000ff007986 0x000fe2000c114904 \
    0x000000000000794d 0x000fea0003800000 \
    0xfffffff000007947 0x000fc0000383ffff \
    0x0000000000007918 0x000fc00000000000 \
    0x0000000000007918 0x000fc00000000000 \
    > "$BIN"

"$NVDISASM" -b SM75 "$BIN"

echo
echo "Expected (source comments in src/backend_nvidia_sass.cyr):"
echo "  MOV R1, c[0x0][0x28] / MOV R0, 0xdeadbeef / ULDC.64 UR4, c[0x0][0x160]"
echo "  STG.E.SYS [UR4], R0 / EXIT / BRA 0x50 / NOP / NOP"
