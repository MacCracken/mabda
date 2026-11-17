#!/usr/bin/env python3
"""gen-ktx2-sc-fixtures.py — supercompressed KTX2 test files for mabda 4.5.0.

Builds KTX2 files whose levels are Zstandard (scheme 2) or ZLIB (scheme 3) supercompressed,
with compressors independent of sankoch, the codec mabda decodes them with: the `zstd` CLI
and Python's zlib. It writes:

    tests/tcyr/ktx2_sc_fixtures.cyr   every case as bytes, for tests/tcyr/asset_load.tcyr
    tests/assets/sc_*.ktx2            the cases the hardware gate loads
                                      (programs/native_ktx2_sc_load_e2e.cyr)

Each level's uncompressed bytes come from a formula, payload_byte(seed, level, i), that the
CPU test and the hardware gate recompute in Cyrius. So what they check does not depend on
either compressor, and a decoder that returned the right length with the wrong bytes fails.

The files are KTX2-shaped: the identifier, the header, the level index and the levels (stored
smallest first, as the spec orders them). There is no data format descriptor, which mabda
does not read.

Needs `zstd` (the CLI) only to REGENERATE; the outputs are checked in. zstd's output can
differ between zstd versions, so there is no byte-for-byte --check; the CPU test is the check.

USAGE
    scripts/gen-ktx2-sc-fixtures.py
"""
import os
import struct
import subprocess
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT_CYR = os.path.join(ROOT, "tests", "tcyr", "ktx2_sc_fixtures.cyr")
ASSETS = os.path.join(ROOT, "tests", "assets")
KTX2_ID = bytes([0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A])
VK_RGBA8 = 37
VK_BC7 = 145


def payload_byte(seed, level, i):
    """Half patterned (compresses), half scrambled (literal-heavy), in 64-byte runs."""
    if ((i >> 6) & 1) == 0:
        return (i * 7 + level * 31 + seed) & 0xFF
    v = (i * 2654435761 + level * 40503 + seed * 977) & 0xFFFFFFFF
    return (v >> 13) & 0xFF


def payload(seed, level, n):
    return bytes(payload_byte(seed, level, i) for i in range(n))


def zstd(data):
    r = subprocess.run(["zstd", "-q", "-c", "-19", "--no-progress"], input=data,
                       stdout=subprocess.PIPE, check=True)
    return r.stdout


def level_size(vk, w, h):
    if vk == VK_RGBA8:
        return w * h * 4
    if vk == VK_BC7:
        return ((w + 3) // 4) * ((h + 3) // 4) * 16
    raise SystemExit("unknown vk")


# name, vk, w, h, levels, layers, faces, scheme, seed, mutation, expect
#   mutation: "" | "short" (a level decodes one byte short) | "long" (64 bytes long)
#             | "truncated" (the stream is cut 6 bytes early) | "ulen" (uncompressedByteLength
#             one past the level's size) | "basislz" (scheme 1 in the header)
#   expect: "ok" or the GPU_ERR_* the load must fail with
CASES = [
    ("zstd_rgba8_mips", VK_RGBA8, 64, 64, 3, 0, 1, 2, 11, "", "ok"),
    ("zlib_rgba8_mips", VK_RGBA8, 64, 64, 3, 0, 1, 3, 12, "", "ok"),
    ("zstd_bc7", VK_BC7, 64, 64, 1, 0, 1, 2, 13, "", "ok"),
    ("zstd_array4", VK_RGBA8, 32, 32, 1, 4, 1, 2, 14, "", "ok"),
    ("zlib_cube", VK_RGBA8, 16, 16, 1, 0, 6, 3, 15, "", "ok"),
    ("zstd_short", VK_RGBA8, 16, 16, 1, 0, 1, 2, 16, "short", "GPU_ERR_CONTAINER_PARSE"),
    ("zlib_long", VK_RGBA8, 16, 16, 1, 0, 1, 3, 17, "long", "GPU_ERR_CONTAINER_PARSE"),
    ("zstd_truncated", VK_RGBA8, 16, 16, 1, 0, 1, 2, 18, "truncated", "GPU_ERR_CONTAINER_PARSE"),
    ("zlib_truncated", VK_RGBA8, 16, 16, 1, 0, 1, 3, 19, "truncated", "GPU_ERR_CONTAINER_PARSE"),
    ("zstd_ulen", VK_RGBA8, 16, 16, 1, 0, 1, 2, 20, "ulen", "GPU_ERR_CONTAINER_PARSE"),
    ("basislz", VK_RGBA8, 16, 16, 1, 0, 1, 1, 21, "basislz", "GPU_ERR_FORMAT_UNSUPPORTED"),
]
HW = ["zstd_rgba8_mips", "zlib_rgba8_mips", "zstd_array4", "zlib_cube"]


def build(case):
    name, vk, w, h, levels, layers, faces, scheme, seed, mut, _ = case
    images = max(layers, 1) * faces
    raw, comp = [], []
    for lv in range(levels):
        n = level_size(vk, max(w >> lv, 1), max(h >> lv, 1)) * images
        data = payload(seed, lv, n)
        src = data
        if mut == "short":
            src = data[:-1]
        if mut == "long":
            src = data + payload(seed + 1, lv, 64)
        c = src if scheme in (0, 1) else (zstd(src) if scheme == 2 else zlib.compress(src, 9))
        if mut == "truncated":
            c = c[:-6]
        raw.append(data)
        comp.append(c)
    hdr = bytearray(80)
    hdr[0:12] = KTX2_ID
    struct.pack_into("<IIIIIIIII", hdr, 12, vk, 1, w, h, 0, layers, faces, levels, scheme)
    index_end = 80 + 24 * levels
    body = bytearray()
    offs = [0] * levels
    for lv in reversed(range(levels)):      # smallest level first, as the spec stores them
        offs[lv] = index_end + len(body)
        body += comp[lv]
    index = bytearray()
    for lv in range(levels):
        ulen = len(raw[lv]) + (1 if mut == "ulen" else 0)
        index += struct.pack("<QQQ", offs[lv], len(comp[lv]), ulen)
    return bytes(hdr + index + body)


def main():
    files = [(c, build(c)) for c in CASES]
    for c, data in files:
        if c[0] in HW:
            with open(os.path.join(ASSETS, "sc_%s.ktx2" % c[0]), "wb") as f:
                f.write(data)
    maxlen = max(len(d) for _, d in files)
    o = []
    o.append("# tests/tcyr/ktx2_sc_fixtures.cyr — GENERATED by scripts/gen-ktx2-sc-fixtures.py; do not edit.")
    o.append("# Supercompressed KTX2 files (4.5.0): Zstandard levels from the zstd CLI, ZLIB levels from")
    o.append("# Python's zlib, so the decoder under test (sankoch) never produced them. Each level's")
    o.append("# uncompressed bytes are ktx2sc_payload_byte(seed, level, i); ktx2sc_build(k, buf) writes")
    o.append("# case k's file into buf (KTX2SC_MAX_BYTES) and returns its length.")
    o.append("var KTX2SC_COUNT = %d;" % len(files))
    o.append("var KTX2SC_MAX_BYTES = %d;" % ((maxlen + 3) // 4 * 4))
    o.append("")
    o.append("fn _ktx2sc_w4(b, i, n, w0, w1, w2, w3): i64 {")
    o.append("    store32(b + i * 4, w0);")
    o.append("    if (n > 1) { store32(b + i * 4 + 4, w1); }")
    o.append("    if (n > 2) { store32(b + i * 4 + 8, w2); }")
    o.append("    if (n > 3) { store32(b + i * 4 + 12, w3); }")
    o.append("    return 0;")
    o.append("}")
    o.append("")
    o.append("fn ktx2sc_payload_byte(seed, level, i): i64 {")
    o.append("    if (((i >> 6) & 1) == 0) { return (i * 7 + level * 31 + seed) & 0xFF; }")
    o.append("    var v = (i * 2654435761 + level * 40503 + seed * 977) & 0xFFFFFFFF;")
    o.append("    return (v >> 13) & 0xFF;")
    o.append("}")
    o.append("")
    for k, (c, data) in enumerate(files):
        padded = data + bytes((4 - len(data) % 4) % 4)
        words = struct.unpack("<%dI" % (len(padded) // 4), padded)
        o.append("fn _ktx2sc_%s(b): i64 {" % c[0])
        for i in range(0, len(words), 4):
            chunk = list(words[i:i + 4]) + [0] * (4 - len(words[i:i + 4]))
            o.append("    _ktx2sc_w4(b, %d, %d, 0x%08X, 0x%08X, 0x%08X, 0x%08X);" %
                     (i, min(4, len(words) - i), chunk[0], chunk[1], chunk[2], chunk[3]))
        o.append("    return %d;" % len(data))
        o.append("}")
        o.append("")
    o.append("fn ktx2sc_build(k, b): i64 {")
    for k, (c, _) in enumerate(files):
        o.append("    if (k == %d) { return _ktx2sc_%s(b); }" % (k, c[0]))
    o.append("    return 0;")
    o.append("}")
    o.append("")
    for field, idx in (("name", 0), ("seed", 8)):
        o.append("fn ktx2sc_%s(k): i64 {" % field)
        for k, c in enumerate(CASES):
            v = '"%s"' % c[0] if idx == 0 else str(c[8])
            o.append("    if (k == %d) { return %s; }" % (k, v))
        o.append("    return 0;")
        o.append("}")
        o.append("")
    o.append("# 0 when case k must load, else the GPU_ERR_* the load must fail with.")
    o.append("fn ktx2sc_expect(k): i64 {")
    for k, c in enumerate(CASES):
        o.append("    if (k == %d) { return %s; }" % (k, "0" if c[10] == "ok" else c[10]))
    o.append("    return 0;")
    o.append("}")
    with open(OUT_CYR, "w") as f:
        f.write("\n".join(o) + "\n")
    print("wrote %s (%d cases) and %d assets" % (OUT_CYR, len(files), len(HW)))


if __name__ == "__main__":
    main()
