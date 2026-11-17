#!/usr/bin/env python3
"""gen-spirv-ranga-fixtures.py — build tests/tcyr/spirv_ranga_kernels.cyr from tests/spirv/ranga/.

ranga (the AGNOS image-processing library) emits its twelve per-pixel compute kernels as SPIR-V
(`ranga/src/gpu_kernels.cyr` through `ranga/src/gpu_spirv.cyr`) and hands them to
`gpu_shader_module_create_spirv`. The first eleven are N9's done-when corpus. resize_bilinear
joined when SM75 got a 1024-id MIR space (4.4.0, the fourth N9 follow-on), and GFX9 has run it
since its 256-id cap went the same way (4.4.0). Their exact bytes are vendored
in tests/spirv/ranga/<name>.spv, with tests/spirv/ranga/PROVENANCE naming the ranga commit they
came from, so mabda's CPU suite and hardware gates run them without a ranga checkout.

The .spv files are ranga's bytes, not a disassembly: ranga writes SPIR-V 1.3 with storage
buffers in the OpEntryPoint interface (a 1.4 rule), which spirv-val rejects and mabda accepts,
and a spirv-dis / spirv-as round trip does not reproduce the bytes.

THE ORACLE. This script also RUNS each module, in a small SPIR-V interpreter written here
(the ops ranga's kernels use, nothing more), over the gate's seeded input, and writes the
FNV-1a 64 of each output into the fixture as its golden. Every f32 operation is rounded to f32
(IEEE round-to-nearest-even; an f32 add / sub / mul computed in f64 and then rounded is exactly
the f32 result, because 53 >= 2 * 24 + 2). The interpreter shares nothing with mabda's compiler
(no spirv_lower, no MIR), so a lowering bug common to the AMD and NVIDIA back ends still shows
as a mismatch on both. ranga's own GPU suites pin the same semantics on hand-picked pixels.

The gate's inputs live here too, and are written into the fixture so the program and the oracle
cannot drift: a 61 x 67 image (4087 pixels: 16 workgroups of 256, the last one partial),
xorshift32-seeded pixels (and destinations for the two-buffer kernels), each kernel's params
block as ranga's gpu_pipeline.cyr packs it, and each kernel's output raster.

Output: tests/tcyr/spirv_ranga_kernels.cyr, included by tests/tcyr/sm75_compile.tcyr and
programs/ranga_kernels_e2e.cyr: rk_build(k, buf), rk_name(k), rk_shape(k), rk_params(k, p),
rk_out_w(k) / rk_out_h(k), rk_golden(k), and the RK_* input constants.

USAGE
    scripts/gen-spirv-ranga-fixtures.py                     write the fixture from tests/spirv/ranga/
    scripts/gen-spirv-ranga-fixtures.py --check             exit 1 if the checked-in fixture is stale
    scripts/gen-spirv-ranga-fixtures.py --from-ranga DIR    re-dump the .spv files from a ranga
                                                            checkout at DIR (its lib/ resolved, the
                                                            cyrius toolchain on PATH), then write
"""
import math
import os
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
SRC = os.path.join(ROOT, "tests", "spirv", "ranga")
OUT = os.path.join(ROOT, "tests", "tcyr", "spirv_ranga_kernels.cyr")

# The gate's input (programs/ranga_kernels_e2e.cyr reads these from the fixture).
W, H = 61, 67
SEED_SRC, SEED_DST = 0x2545F491, 0x9E3779B9
SENT = 16
SENT_V = 0x5A5A5A5A
WG = 256


def f32bits(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


# (name, ranga emitter, binding shape, params words after `count`, output raster). The shape is
# how ranga's gpu_pipeline.cyr binds it:
#   1buf: [pixels, params(u32 view), params(f32 view)]          (in place)
#   2buf: [src, dst, params, params]                             (dst read and written)
#   geom: [src, out, params, params]                             (out has its own shape, zeroed)
KERNELS = [
    ("invert", "gk_invert", "1buf", [], (W, H)),
    ("grayscale", "gk_grayscale", "1buf", [], (W, H)),
    ("fade", "gk_fade", "1buf", [f32bits(0.37)], (W, H)),
    ("brightness_contrast", "gk_brightness_contrast", "1buf", [f32bits(0.1), f32bits(1.25)], (W, H)),
    ("saturation", "gk_saturation", "1buf", [f32bits(1.7)], (W, H)),
    ("dissolve", "gk_dissolve", "2buf", [f32bits(0.42)], (W, H)),
    ("wipe", "gk_wipe", "2buf", [W, H, f32bits(0.6)], (W, H)),
    ("crop", "gk_crop", "geom", [W, 40, 30, 7, 11], (40, 30)),
    ("flip_horizontal", "gk_flip_horizontal", "geom", [W, H], (W, H)),
    ("flip_vertical", "gk_flip_vertical", "geom", [W, H], (W, H)),
    ("resize_nearest", "gk_resize_nearest", "geom", [W, H, 97, 53], (97, 53)),
    # bilinear: params[5..6]: the host's reciprocals 1/max(dst_w-1,1), 1/max(dst_h-1,1) (ranga's layout)
    ("resize_bilinear", "gk_resize_bilinear", "geom", [W, H, 97, 53, f32bits(1.0 / 96), f32bits(1.0 / 52)],
     (97, 53)),
]
SHAPE = {"1buf": 1, "2buf": 2, "geom": 3}
# Kernels the GFX9 back end refuses, with the stage's code (the AMD gate and the CPU suite
# expect exactly that code). Empty since 4.4.0: resize_bilinear was MIR_ERR_ID_OOR (25) at the
# old 256-id cap.
GFX9_REFUSE = {}


# ---------------------------------------------------------------- ranga → .spv
def from_ranga(rdir):
    rdir = os.path.abspath(rdir)
    for f in ("lib/mabda.cyr", "src/error.cyr", "src/gpu_spirv.cyr", "src/gpu_kernels.cyr"):
        if not os.path.exists(os.path.join(rdir, f)):
            raise SystemExit(f"{rdir}: no {f} (a ranga checkout with lib/ resolved by `cyrius deps`?)")
    os.makedirs(SRC, exist_ok=True)
    with tempfile.TemporaryDirectory() as td:
        prog = [
            'include "lib/mabda.cyr"',
            'include "src/error.cyr"',
            'include "src/gpu_spirv.cyr"',
            'include "src/gpu_kernels.cyr"',
            "fn dump(path, k): i64 {",
            "    var s = spv_new();",
        ]
        for k, kern in enumerate(KERNELS):
            prog.append(f"    if (k == {k}) {{ {kern[1]}(s); }}")
        prog += [
            "    var buf[8];",
            "    var n = spv_finish(s, &buf);",
            "    var fd = sys_open(path, 577, 420);",
            "    syscall(1, fd, load64(&buf), n);",
            "    sys_close(fd);",
            "    return n;",
            "}",
            "fn main(): i64 {",
            "    alloc_init();",
        ]
        for k, kern in enumerate(KERNELS):
            prog.append(f'    dump("{os.path.join(SRC, kern[0] + ".spv")}", {k});')
        prog += ["    return 0;", "}", "var rc = main();", "syscall(60, rc);", ""]
        src = os.path.join(td, "dump.cyr")
        exe = os.path.join(td, "dump")
        open(src, "w").write("\n".join(prog))
        subprocess.run(["cyrius", "build", src, exe], cwd=rdir, check=True, stdout=subprocess.DEVNULL)
        subprocess.run([exe], cwd=rdir, check=True)
    git = ["git", "-C", rdir]
    head = subprocess.run(git + ["rev-parse", "HEAD"], capture_output=True, text=True, check=True).stdout.strip()
    date = subprocess.run(git + ["log", "-1", "--format=%cd", "--date=short"], capture_output=True, text=True,
                          check=True).stdout.strip()
    dirty = subprocess.run(git + ["status", "--porcelain", "--", "src"], capture_output=True, text=True,
                           check=True).stdout.strip()
    with open(os.path.join(SRC, "PROVENANCE"), "w") as f:
        f.write(f"ranga {head} ({date}){' + uncommitted src/ changes' if dirty else ''}\n")
        f.write("emitted by ranga/src/gpu_kernels.cyr via scripts/gen-spirv-ranga-fixtures.py --from-ranga\n")


def words_of(name):
    data = open(os.path.join(SRC, name + ".spv"), "rb").read()
    if len(data) % 4 != 0 or len(data) < 20:
        raise SystemExit(f"{name}.spv: {len(data)} bytes is not a SPIR-V module")
    w = list(struct.unpack("<%dI" % (len(data) // 4), data))
    if w[0] != 0x07230203:
        raise SystemExit(f"{name}.spv: bad magic {w[0]:#x}")
    return w


# ---------------------------------------------------------------- the oracle
M32 = 0xFFFFFFFF


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def bits_to_f32(b):
    return struct.unpack("<f", struct.pack("<I", b & M32))[0]


class Module:
    """Just enough of a SPIR-V module to run ranga's kernels: types, constants, the buffer and
    builtin variables, and the entry function's instructions."""

    def __init__(self, words):
        self.types, self.consts, self.binding, self.builtin = {}, {}, {}, {}
        self.vars, self.code, self.labels, self.typeof = {}, [], {}, {}
        self.glsl = None
        i, infn = 5, False
        while i < len(words):
            wc, op = words[i] >> 16, words[i] & 0xFFFF
            a = words[i + 1:i + wc]
            if wc == 0:
                raise SystemExit("zero word count")
            if op == 11:
                self.glsl = a[0]
            elif op == 71:                                    # OpDecorate
                if a[1] == 33:
                    self.binding[a[0]] = a[2]
                if a[1] == 11:
                    self.builtin[a[0]] = a[2]
            elif op == 19:
                self.types[a[0]] = ("void",)
            elif op == 20:
                self.types[a[0]] = ("bool",)
            elif op == 21:
                self.types[a[0]] = ("int", a[1], a[2])
            elif op == 22:
                self.types[a[0]] = ("float", a[1])
            elif op == 23:
                self.types[a[0]] = ("vec", a[1], a[2])
            elif op == 29:
                self.types[a[0]] = ("rtarray", a[1])
            elif op == 30:
                self.types[a[0]] = ("struct",) + tuple(a[1:])
            elif op == 32:
                self.types[a[0]] = ("ptr", a[1], a[2])
            elif op == 33:
                self.types[a[0]] = ("fn",)
            elif op == 43:                                    # OpConstant
                t = self.types[a[0]]
                self.consts[a[1]] = bits_to_f32(a[2]) if t[0] == "float" else a[2] & M32
                self.typeof[a[1]] = a[0]
            elif op == 59 and not infn:                       # OpVariable (module scope)
                self.vars[a[1]] = a[2]
            elif op == 54:
                infn = True
            elif op == 56:
                infn = False
            elif infn:
                if op == 59:
                    raise SystemExit("function-scope OpVariable: not in ranga's op set")
                if op == 248:
                    self.labels[a[0]] = len(self.code)
                self.code.append((op, a))
            i += wc

    def is_float(self, tid):
        return self.types[tid][0] == "float"


def run(mod, bufs, invocations):
    """Run every invocation in order. ranga's kernels have no cross-invocation dependence (each
    reads its own pixel, or a source buffer nothing writes), so sequential is exact."""
    code, consts, labels = mod.code, mod.consts, mod.labels
    for gid in range(invocations):
        env = dict(consts)
        pc = 0
        while True:
            op, a = code[pc]
            pc += 1
            if op == 248 or op == 247:                        # OpLabel / OpSelectionMerge
                continue
            if op == 253:                                     # OpReturn
                break
            if op == 249:                                     # OpBranch
                pc = labels[a[0]]
                continue
            if op == 250:                                     # OpBranchConditional
                pc = labels[a[1]] if env[a[0]] else labels[a[2]]
                continue
            if op == 61:                                      # OpLoad
                t, r, p = a[0], a[1], a[2]
                if p in mod.builtin:
                    env[r] = (gid, 0, 0)                      # GlobalInvocationId, 1-D dispatch
                else:
                    b, idx = env[p]
                    w = bufs[b][idx]
                    env[r] = bits_to_f32(w) if mod.is_float(t) else w
                mod.typeof[r] = t
                continue
            if op == 62:                                      # OpStore
                b, idx = env[a[0]]
                v = env[a[1]]
                bufs[b][idx] = f32bits(v) if isinstance(v, float) else v & M32
                continue
            if op == 65:                                      # OpAccessChain base 0 idx
                r, base = a[1], a[2]
                if len(a) != 5 or env[a[3]] != 0:
                    raise SystemExit("access chain shape not in ranga's set")
                env[r] = (mod.binding[base], env[a[4]])
                continue
            if op == 81:                                      # OpCompositeExtract
                env[a[1]] = env[a[2]][a[3]]
                continue
            t, r = a[0], a[1]
            if op == 12:                                      # OpExtInst GLSL.std.450
                if a[2] != mod.glsl:
                    raise SystemExit("OpExtInst outside GLSL.std.450")
                inst, ops = a[3], [env[o] for o in a[4:]]
                if inst == 8:
                    env[r] = f32(math.floor(ops[0]))
                elif inst == 37:
                    env[r] = min(ops[0], ops[1])
                elif inst == 40:
                    env[r] = max(ops[0], ops[1])
                else:
                    raise SystemExit(f"GLSL.std.450 {inst}: not in ranga's op set")
                continue
            x = env[a[2]] if len(a) > 2 else None
            y = env[a[3]] if len(a) > 3 else None
            if op == 112:
                v = f32(float(x))                             # OpConvertUToF
            elif op == 110:
                v = int(x) & M32                              # OpConvertFToS (trunc toward 0)
            elif op == 128:
                v = (x + y) & M32
            elif op == 130:
                v = (x - y) & M32
            elif op == 132:
                v = (x * y) & M32
            elif op == 134:
                v = x // y
            elif op == 137:
                v = x % y
            elif op == 129:
                v = f32(x + y)
            elif op == 131:
                v = f32(x - y)
            elif op == 133:
                v = f32(x * y)
            elif op == 172:
                v = x > y                                     # OpUGreaterThan
            elif op == 194:
                v = x >> (y & 31)
            elif op == 196:
                v = (x << (y & 31)) & M32
            elif op == 197:
                v = x | y
            elif op == 199:
                v = x & y
            else:
                raise SystemExit(f"opcode {op}: not in ranga's op set")
            env[r] = v


def xorshift_fill(n, seed):
    out, s = [], seed
    for _ in range(n):
        s ^= (s << 13) & M32
        s ^= s >> 17
        s ^= (s << 5) & M32
        out.append(s)
    return out + [SENT_V] * SENT


def fnv(words):
    h = 0xCBF29CE484222325
    for w in words:
        for byte in struct.pack("<I", w):
            h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def golden(words, shape, params, outdim):
    mod = Module(words)
    nin, nout = W * H, outdim[0] * outdim[1]
    prm = [nout] + params + [0] * (8 - 1 - len(params))
    src = xorshift_fill(nin, SEED_SRC)
    if shape == "1buf":
        bufs = {0: src, 1: prm, 2: prm}
        outbuf = src
    else:
        dst = xorshift_fill(nout, SEED_DST) if shape == "2buf" else [0] * nout + [SENT_V] * SENT
        bufs = {0: src, 1: dst, 2: prm, 3: prm}
        outbuf = dst
    run(mod, bufs, (nout + WG - 1) // WG * WG)
    if outbuf[nout:] != [SENT_V] * SENT:
        raise SystemExit("oracle wrote past count")
    return fnv(outbuf[:nout])


# ---------------------------------------------------------------- the fixture
def build():
    prov = open(os.path.join(SRC, "PROVENANCE")).read().splitlines()
    mods = [(k[0], k[2], k[3], k[4], words_of(k[0])) for k in KERNELS]
    maxb = max(len(m[4]) for m in mods) * 4
    o = []
    o.append("# GENERATED by scripts/gen-spirv-ranga-fixtures.py from tests/spirv/ranga/ — do not edit.")
    o.append("# Regenerate after refreshing the .spv files; `--check` (in `make check-ranga-fixtures`)")
    o.append("# catches drift.")
    o.append("#")
    o.append("# ranga's twelve per-pixel compute kernels, byte for byte (N9's done-when corpus and")
    o.append("# resize_bilinear):")
    for line in prov:
        o.append(f"#   {line}")
    o.append("# rk_build(k, buf) writes kernel k's SPIR-V into buf (>= RK_MAX_BYTES) and returns its byte")
    o.append("# length; rk_shape(k) is how ranga binds it (RK_SHAPE_*). The gate's input: an RK_W x RK_H")
    o.append("# image seeded by xorshift32 from RK_SEED_SRC (two-buffer destinations from RK_SEED_DST),")
    o.append("# RK_SENT sentinel words of RK_SENT_V after every buffer's pixels, rk_params(k, p) (32 bytes,")
    o.append("# ranga's [count, a, b, c, d, e] block) and an rk_out_w(k) x rk_out_h(k) output.")
    o.append("# rk_golden(k) is the FNV-1a 64 of that output as the generator's SPIR-V interpreter computes it.")
    o.append("# rk_gfx9_refuse(k): the MIR code GFX9 refuses kernel k with (0: it compiles).")
    o.append("")
    o.append(f"var RK_COUNT = {len(mods)};")
    o.append(f"var RK_MAX_BYTES = {maxb};")
    o.append("var RK_SHAPE_1BUF = 1;   # [pixels, params, params]")
    o.append("var RK_SHAPE_2BUF = 2;   # [src, dst, params, params]")
    o.append("var RK_SHAPE_GEOM = 3;   # [src, out, params, params]")
    o.append(f"var RK_W = {W};")
    o.append(f"var RK_H = {H};")
    o.append(f"var RK_SEED_SRC = 0x{SEED_SRC:08X};")
    o.append(f"var RK_SEED_DST = 0x{SEED_DST:08X};")
    o.append(f"var RK_SENT = {SENT};")
    o.append(f"var RK_SENT_V = 0x{SENT_V:08X};")
    o.append("")
    o.append("fn _rk_w4(b, i, n, w0, w1, w2, w3): i64 {")
    o.append("    store32(b + i * 4, w0);")
    o.append("    if (n > 1) { store32(b + i * 4 + 4, w1); }")
    o.append("    if (n > 2) { store32(b + i * 4 + 8, w2); }")
    o.append("    if (n > 3) { store32(b + i * 4 + 12, w3); }")
    o.append("    return 0;")
    o.append("}")
    o.append("")
    for name, _, _, _, w in mods:
        o.append(f"fn _rk_{name}(b): i64 {{")
        for i in range(0, len(w), 4):
            chunk = w[i:i + 4]
            pad = chunk + [0] * (4 - len(chunk))
            args = ", ".join(f"0x{x:08X}" for x in pad)
            o.append(f"    _rk_w4(b, {i}, {len(chunk)}, {args});")
        o.append(f"    return {len(w) * 4};")
        o.append("}")
        o.append("")

    def table(fn, args, vals, default):
        o.append(f"fn {fn}({args}): i64 {{")
        for k, v in enumerate(vals):
            o.append(f"    if (k == {k}) {{ return {v}; }}")
        o.append(f"    return {default};")
        o.append("}")
        o.append("")

    table("rk_build", "k, b", [f"_rk_{m[0]}(b)" for m in mods], "0")
    table("rk_name", "k", [f'"{m[0]}"' for m in mods], '"?"')
    table("rk_shape", "k", [SHAPE[m[1]] for m in mods], "0")
    table("rk_out_w", "k", [m[3][0] for m in mods], "0")
    table("rk_out_h", "k", [m[3][1] for m in mods], "0")
    table("rk_gfx9_refuse", "k", [GFX9_REFUSE.get(m[0], 0) for m in mods], "0")
    o.append("# ranga's params block for kernel k: [count, a, b, c, d, e] into p (32 bytes, zeroed).")
    o.append("fn rk_params(k, p): i64 {")
    o.append("    memset(p, 0, 32);")
    o.append("    store32(p, rk_out_w(k) * rk_out_h(k));")
    for k, m in enumerate(mods):
        if m[2]:
            o.append(f"    if (k == {k}) {{   # {m[0]}")
            for j, v in enumerate(m[2]):
                o.append(f"        store32(p + {4 * (j + 1)}, 0x{v:08X});")
            o.append("    }")
    o.append("    return 0;")
    o.append("}")
    o.append("")
    golds = [golden(m[4], m[1], m[2], m[3]) for m in mods]
    table("rk_golden", "k", [f"0x{g:016X}" for g in golds], "0")
    return "\n".join(o).rstrip("\n") + "\n"


def main():
    if "--from-ranga" in sys.argv:
        i = sys.argv.index("--from-ranga")
        if i + 1 >= len(sys.argv):
            raise SystemExit("--from-ranga needs a ranga checkout directory")
        from_ranga(sys.argv[i + 1])
    text = build()
    if "--check" in sys.argv:
        cur = open(OUT).read() if os.path.exists(OUT) else ""
        if cur != text:
            print(f"{OUT} is stale — run scripts/gen-spirv-ranga-fixtures.py")
            return 1
        print("spirv_ranga_kernels.cyr up to date")
        return 0
    open(OUT, "w").write(text)
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
