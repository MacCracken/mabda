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

echo
echo "=== CS (downsample_2x2, 316 bytes instr; v3.1 M.4) ==="
dwords_to_bytes \
    92080407 8E098408 8E0A8306 81090A09 800C0900 820D8001 \
    8E0B8304 800E0B0C 820F800D 8E108208 8E118206 81101110 \
    80121002 82138003 7E00020C 7E02020D 7E04020E 7E06020F \
    DC548000 047F0000 DC548000 067F0002 BF8C0F70 261008FF \
    000000FF 26120AFF 000000FF 26140CFF 000000FF 26160EFF \
    000000FF D1FF0008 042A1308 68101708 20101082 D1C80009 \
    02211104 D1C8000A 02211105 D1C8000B 02211106 D1C8000C \
    02211107 D1FF0009 042E1509 68121909 20121282 24121288 \
    D1C8000A 02212104 D1C8000B 02212105 D1C8000C 02212106 \
    D1C8000D 02212107 D1FF000A 0432170A 68141B0A 20141482 \
    24141490 20160898 20180A98 201A0C98 201C0E98 D1FF000B \
    0436190B 68161D0B 20161682 24161698 D2020008 042A1308 \
    28101708 7E000212 7E020213 DC708000 007F0800 BF8C0070 \
    BF810000 \
    | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
