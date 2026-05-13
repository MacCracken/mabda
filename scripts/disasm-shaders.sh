#!/bin/bash
# Disassemble mabda's pre-compiled GFX9 ISA shaders against the
# Cezanne (gfx90c) target. Verifies that every instruction in the
# fullscreen-triangle VS and solid-red FS is a valid GFX9 encoding —
# a single bit error in a hand-encoded VOP1/VOP2/EXP/SOPP would
# show up here as a decoder error or as a different mnemonic than
# the source comment.
set -eu

# Dwords from src/backend_native_shaders.cyr — keep in sync if the
# shader builders change. To LE bytes for llvm-mc --disassemble.
dwords_to_bytes() {
    python3 - "$@" <<'PYEOF'
import sys
for d in sys.argv[1:]:
    v = int(d, 16)
    print(f"0x{v & 0xFF:02x} 0x{(v >> 8) & 0xFF:02x} 0x{(v >> 16) & 0xFF:02x} 0x{(v >> 24) & 0xFF:02x}")
PYEOF
}

echo "=== VS (fullscreen_triangle_vs, 116 bytes) ==="
dwords_to_bytes \
    24040081 24000082 26000084 26040484 \
    680000C1 680404C1 7E000B00 7E020B02 \
    7E040280 7E0602F2 C40008CF 03020100 \
    BF810000 \
    BF800000 BF800000 BF800000 BF800000 \
    BF800000 BF800000 BF800000 BF800000 \
    BF800000 BF800000 BF800000 BF800000 \
    BF800000 BF800000 BF800000 BF800000 \
    | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c

echo
echo "=== FS (solid_red, 92 bytes) ==="
dwords_to_bytes \
    7E0002F2 7E020280 7E040280 7E0602F2 \
    C400180F 03020100 BF810000 \
    BF800000 BF800000 BF800000 BF800000 \
    BF800000 BF800000 BF800000 BF800000 \
    BF800000 BF800000 BF800000 BF800000 \
    BF800000 BF800000 BF800000 \
    | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
