#!/usr/bin/env python3
# decode_render.py — mabda N7.2a render-capture walker.
#
# Builds per-subchannel Turing method-name maps from open-gpu-doc class headers,
# parses the NVIF class-creates from capture.log, and walks the DRAW EXEC
# pushbuffer dumping (subc, mthd_offset_hex, op, args_hex...) for every method.
#
# Turing dma-pusher header: (op<<29)|(count<<16)|(subc<<13)|(mthd>>2)
#   op 1=INCR 3=NON_INCR 4=IMMD(13-bit imm in count slot) 5=INCR_ONCE
# Subchannel->class binding (confirmed via SET_OBJECT in the queue-init push +
# the standard NVK/nouveau assignment): subc0=TURING_A(3D 0xC597),
# subc1=TURING_COMPUTE_A(0xC5C0), subc4=TURING_DMA_COPY_A(CE 0xC5B5).
import os, re, struct, sys, glob

HERE = os.path.dirname(os.path.abspath(__file__))
CAPDIR = os.path.join(HERE, "nvcaprender")
DOC = "/home/macro/Repos/open-gpu-doc/classes"
H_3D = DOC + "/3d/clc597.h"
H_COMP = DOC + "/compute/clc5c0.h"

CLASS_NAMES = {
    0xC597: "TURING_A (3D)",
    0xC5C0: "TURING_COMPUTE_A",
    0xC5B5: "TURING_DMA_COPY_A (CE)",
    0x902D: "FERMI_TWOD_A (2D)",
    0xA140: "KEPLER_INLINE_TO_MEMORY_B (I2M)",
}

# ---- generic class-header parser (simple authoritative, arrays on-demand) ----
# A method is "#define NV<PFX>_<NAME>  0xHHHH" with HHHH 1-4 hex digits (no
# parens) -> excludes 8-hex enum constants. Array form is
# "#define NV<PFX>_<NAME>(j)  (0xBASE+(j)*STRIDE)".
def load_class(path, pfx):
    simple, arrays = {}, []
    if not os.path.exists(path):
        return simple, arrays
    rs = re.compile(r'#define\s+' + pfx + r'_([A-Z0-9_]+)\s+0x([0-9a-fA-F]{1,4})\s*$')
    ra = re.compile(r'#define\s+' + pfx + r'_([A-Z0-9_]+)\(\w+\)\s+\(0x([0-9a-fA-F]+)\+\(\w+\)\*(0x[0-9a-fA-F]+|\d+)\)')
    for line in open(path):
        a = ra.search(line)
        if a:
            name, base, stride = a.group(1), int(a.group(2), 16), a.group(3)
            stride = int(stride, 16) if stride.startswith("0x") else int(stride)
            arrays.append((base, stride, name))
            continue
        s = rs.search(line)
        if s:
            off = int(s.group(2), 16)
            if off <= 0x3fff:
                simple.setdefault(off, s.group(1))
    return simple, arrays

S3D, A3D = load_class(H_3D, "NVC597")
SCOMP, ACOMP = load_class(H_COMP, "NVC5C0")

# Copy-engine (clc5b5) — small fixed set covering the methods NVK emits here.
COPY = {
    0x0100: "NO_OPERATION", 0x0300: "LAUNCH_DMA",
    0x0400: "OFFSET_IN_UPPER", 0x0404: "OFFSET_IN_LOWER",
    0x0408: "OFFSET_OUT_UPPER", 0x040c: "OFFSET_OUT_LOWER",
    0x0410: "PITCH_IN", 0x0414: "PITCH_OUT",
    0x0418: "LINE_LENGTH_IN", 0x041c: "LINE_COUNT",
    0x0700: "SET_REMAP_CONST_A", 0x0704: "SET_REMAP_CONST_B",
    0x0708: "SET_REMAP_COMPONENTS",
    0x0728: "SET_SRC_BLOCK_SIZE", 0x072c: "SET_SRC_WIDTH",
    0x0730: "SET_SRC_HEIGHT", 0x0734: "SET_SRC_DEPTH",
    0x0738: "SET_SRC_LAYER", 0x073c: "SET_SRC_ORIGIN",
    0x0744: "SRC_ORIGIN_X", 0x0748: "SRC_ORIGIN_Y",
}

def lookup(simple, arrays, off):
    if off in simple:
        return simple[off]
    best = None
    for base, stride, name in arrays:
        if off >= base and (off - base) % stride == 0:
            idx = (off - base) // stride
            if 0 <= idx < 64 and (best is None or base > best[0]):
                best = (base, idx, name)
    if best:
        return "%s(%d)" % (best[2], best[1])
    return "?"

SUBC_CLASS = {0: 0xC597, 1: 0xC5C0, 4: 0xC5B5}

def mname(subc, off):
    cls = SUBC_CLASS.get(subc)
    if cls == 0xC597:
        return lookup(S3D, A3D, off), "3D"
    if cls == 0xC5C0:
        return lookup(SCOMP, ACOMP, off), "COMPUTE"
    if cls == 0xC5B5:
        return COPY.get(off, "?"), "COPY"
    return "?", "subc%d?" % subc

# ---- parse NVIF class-creates from capture.log ----
def parse_nvif():
    log = os.path.join(CAPDIR, "capture.log")
    out, lines, i = [], open(log, errors="replace").read().splitlines(), 0
    while i < len(lines):
        mh = re.match(r'\[(\d+)\] ioctl NVIF .*size (\d+)', lines[i])
        if mh and i + 1 < len(lines) and "nvif [" in lines[i + 1]:
            seq, size = int(mh.group(1)), int(mh.group(2))
            by, j = [], i + 2
            while j < len(lines) and re.match(r'\s+([0-9a-f]{2} )+', lines[j]):
                by += [int(x, 16) for x in lines[j].split()]; j += 1
            if len(by) >= 54 and by[1] == 0x02 and size == 56:  # type 0x02 == NEW
                token = struct.unpack_from("<Q", bytes(by), 32)[0]
                oclass = struct.unpack_from("<H", bytes(by), 52)[0]
                out.append((seq, token, oclass))
            i = j
        else:
            i += 1
    return out

# ---- dma-pusher walk ----
OPS = {1: "INCR", 3: "NINC", 4: "IMMD", 5: "INC1"}

def walk(path):
    data = open(path, "rb").read()
    n = len(data) // 4
    dw = struct.unpack("<%dI" % n, data[:n * 4])
    rows, i = [], 0
    while i < n:
        h, hidx = dw[i], i; i += 1
        if h == 0:
            rows.append((hidx * 4, None, None, "NOP", "NOP", "-", []))
            continue
        op = (h >> 29) & 0x7
        subc = (h >> 13) & 0x7
        mthd = (h & 0x1fff) << 2
        name, cls = mname(subc, mthd)
        if op in (1, 3, 5):
            count = (h >> 16) & 0x1fff
            args = list(dw[i:i + count]); i += count
            rows.append((hidx * 4, subc, mthd, OPS[op], name, cls, args))
        elif op == 4:
            imm = (h >> 16) & 0x1fff
            rows.append((hidx * 4, subc, mthd, "IMMD", name, cls, [imm]))
        else:
            rows.append((hidx * 4, subc, mthd, "op%d?" % op, "UNKNOWN h=0x%08x" % h, cls, []))
    return rows

def fmt_rows(rows):
    o = []
    for hoff, subc, mthd, op, name, cls, args in rows:
        if op == "NOP":
            o.append("  hdr@0x%04x  NOP" % hoff); continue
        ah = " ".join("0x%08x" % a for a in args)
        if op == "IMMD":
            o.append("  hdr@0x%04x  subc%d[%s]  mthd 0x%04x  IMMD     imm=0x%-5x         %s" %
                     (hoff, subc, cls, mthd, args[0], name))
        else:
            o.append("  hdr@0x%04x  subc%d[%s]  mthd 0x%04x  %s x%-2d [%s]   %s" %
                     (hoff, subc, cls, mthd, op, len(args), ah, name))
    return "\n".join(o)

if __name__ == "__main__":
    draw = sys.argv[1] if len(sys.argv) > 1 else None
    if not draw:
        cands = sorted(glob.glob(os.path.join(CAPDIR, "*_exec_*len1648.bin")))
        draw = cands[-1] if cands else None
    print("# clc597(3D) methods: %d  clc5c0(compute) methods: %d" % (len(S3D), len(SCOMP)))
    print("\n# === NVIF class-creates (NEW, type=0x02) from capture.log ===")
    seen = set()
    for seq, token, oclass in parse_nvif():
        print("  [seq %d] token=0x%016x  oclass=0x%04x  %s%s" %
              (seq, token, oclass, CLASS_NAMES.get(oclass, "?"),
               "  (dup)" if oclass in seen else ""))
        seen.add(oclass)
    print("\n# === DRAW EXEC pushbuffer: %s ===" % os.path.basename(draw))
    rows = walk(draw)
    print("# %d method records (subc0=3D / subc1=COMPUTE / subc4=COPY)\n" %
          sum(1 for r in rows if r[3] != "NOP"))
    print(fmt_rows(rows))
