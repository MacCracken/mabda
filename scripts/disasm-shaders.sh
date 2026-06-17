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

echo
echo "=== FS (textured_load, image_load + TS.8 scale, 60 bytes instr; v3.2 TS.4/TS.8) ==="
dwords_to_bytes \
    C00E0000 00000000 C0060300 00000030 BF8CC07F \
    0A04040C 0A06060D 7E081102 7E0A1103 \
    F0001F00 00000004 BF8C0F70 C400180F 03020100 \
    BF810000 \
    | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c

echo
echo "=== FS (textured_sample, image_sample/BC + TS.8 scale, unnormalized S#, 60 bytes instr; v3.2 TS.7/TS.8) ==="
dwords_to_bytes \
    C00A0200 00000020 C00E0000 00000000 C0060300 00000030 BF8CC07F \
    0A08040C 0A0A060D F0800F00 00400004 BF8C0F70 C400180F 03020100 \
    BF810000 \
    | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c

# ================================================================
# Phase N.0 — per-FORM encoder round-trip (src/gfx9_encode.cyr)
# ================================================================
# One representative dword (or dword pair) per GFX9 instruction format the
# operand-parameterized encoders in gfx9_encode.cyr emit. These are the SAME
# bytes the gfx9_encode unit oracle (tests/tcyr/compiler.tcyr) asserts the
# encoders produce — disassembling them here is the INDEPENDENT llvm-mc check
# that each format is a valid gfx90c encoding decoding to the expected
# mnemonic (per feedback_verify_gfx9_shader_bytes_with_llvm_mc). If an encoder
# bit-field is one off, the Cyrius oracle catches the byte and THIS catches
# the meaning. Compute-only formats (EXP/MIMG are out of the N compiler scope).
echo
echo "=== Phase N.0 per-form encoder round-trip ==="
echo "--- VOP1: v_mov_b32_e32 v0, s0 (0x7E000200) ---"
dwords_to_bytes 7E000200 | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
echo "--- VOP2: v_lshlrev_b32_e32 v2, 1, v0 (0x24040081) ---"
dwords_to_bytes 24040081 | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
echo "--- VOP3a: v_add3_u32 v8, v8, v9, v10 (0xD1FF0008 0x042A1308) ---"
dwords_to_bytes D1FF0008 042A1308 | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
echo "--- SOP2: s_mul_i32 s8, s7, s4 (0x92080407) ---"
dwords_to_bytes 92080407 | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
echo "--- SOPP: s_waitcnt vmcnt(0) lgkmcnt(0) (0xBF8C0070) ---"
dwords_to_bytes BF8C0070 | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
echo "--- SMEM: s_load_dwordx8 s[0:7], s[0:1], 0x0 (0xC00E0000 0x00000000) ---"
dwords_to_bytes C00E0000 00000000 | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
echo "--- FLAT(global): global_load_dwordx2 v[4:5], v[0:1], off (0xDC548000 0x047F0000) ---"
dwords_to_bytes DC548000 047F0000 | llvm-mc --disassemble --arch=amdgcn --mcpu=gfx90c
