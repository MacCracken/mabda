#!/usr/bin/env python3
"""gen-f64-fixtures.py — build tests/tcyr/f64_cases.cyr: inputs and IEEE-exact expected outputs
for the f64 compiler corpus (the _spv_build_f64_* kernels in tests/tcyr/compiler_common.cyr).

4.4.0, the fourth N9 follow-on: f64 (and OpFDiv) on SM75. The AMD gates
(programs/native_spirv_f64_*_e2e.cyr) each check 8 hand-picked lanes against an in-process
Cyrius oracle. This fixture is the cross-vendor oracle: Python floats are IEEE-754 binary64 with
correctly rounded + - * / and sqrt, and math.fma is fused (an exact rational fma where Python < 3.13
lacks it), so every expected value here is the correctly rounded result.
programs/f64_kernels_e2e.cyr runs every kernel on four input sets of
eight lanes on either native backend and must match bit for bit (a NaN only by class: payloads
differ between vendors).

The inputs are chosen per kernel: special values (signed zeros, subnormals, the largest finite,
infinities, NaN) and seeded random ones, keeping out of what SPIR-V leaves undefined:
  - GLSL FMin / FMax on a NaN, or on +0 against -0 (arith);
  - an f32-subnormal OpFConvert result (Vulkan may flush it) (cvt);
  - an f64 -> i32 conversion that does not fit, or of a NaN (i32_cvt);
  - an f32 subnormal input or result (f32_div): flushing them is allowed, and GFX9 does.

USAGE
    scripts/gen-f64-fixtures.py            write the fixture
    scripts/gen-f64-fixtures.py --check    exit 1 if the checked-in fixture is stale
"""
import math
import os
from fractions import Fraction
import random
import struct
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
OUT = os.path.join(ROOT, "tests", "tcyr", "f64_cases.cyr")
SETS, LANES = 4, 8
NAN = float("nan")
INF = float("inf")
MAXF = 1.7976931348623157e308
MIN_SUB = 5e-324
MIN_NORM = 2.2250738585072014e-308


def bits(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def f32(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]


def rnd(r, lo=-1e6, hi=1e6):
    return r.uniform(lo, hi)


def rbits_normal(r):
    # a random finite normal f64 of any exponent
    while True:
        b = r.getrandbits(64)
        e = (b >> 52) & 0x7FF
        if 0 < e < 0x7FF:
            return struct.unpack("<d", struct.pack("<Q", b))[0]


def _fma_exact(a, b, c):
    """a * b + c rounded once (finite inputs), for a Python without math.fma (< 3.13, as on CI):
    the exact rational, then int / int true division, which CPython rounds correctly."""
    exact = Fraction(a) * Fraction(b) + Fraction(c)
    if exact == 0:
        # IEEE: an exact zero is +0 under round-to-nearest, except (+-0 product) + (-0) of one sign
        pneg = math.copysign(1.0, a) * math.copysign(1.0, b) < 0
        if (a == 0.0 or b == 0.0) and c == 0.0 and pneg and math.copysign(1.0, c) < 0:
            return -0.0
        return 0.0
    try:
        return exact.numerator / exact.denominator
    except OverflowError:
        return INF if exact > 0 else -INF


def fma(a, b, c):
    """IEEE fused multiply-add (math.fma raises where IEEE returns inf / NaN)."""
    if not (math.isfinite(a) and math.isfinite(b) and math.isfinite(c)):
        return a * b + c          # inf / NaN rules are the unfused ones here
    if not hasattr(math, "fma"):
        return _fma_exact(a, b, c)
    try:
        return math.fma(a, b, c)
    except OverflowError:
        exact = Fraction(a) * Fraction(b) + Fraction(c)
        return INF if exact > 0 else -INF


def ldexp(a, n):
    if not math.isfinite(a):
        return a
    try:
        return math.ldexp(a, n)
    except OverflowError:
        return math.copysign(INF, a)


def div64(a, b):
    """IEEE f64 division (Python's is correctly rounded; it raises where IEEE returns inf / NaN)."""
    if math.isnan(a) or math.isnan(b):
        return NAN
    if b == 0.0:
        if a == 0.0:
            return NAN
        return math.copysign(INF, a) * math.copysign(1.0, b)
    if math.isinf(a) and math.isinf(b):
        return NAN
    try:
        return a / b
    except OverflowError:
        return math.copysign(INF, a) * math.copysign(1.0, b)


def fmin(x, y):   # GLSL FMin: y if y < x, else x (inputs avoid NaN and +-0 ties)
    return y if y < x else x


def fmax(x, y):
    return y if y > x else x


def cvt_ok(x):
    """Only inputs whose f32 round trip is defined: no f32-subnormal result."""
    if math.isnan(x) or math.isinf(x):
        return True
    y = f32(x) if abs(x) < 3.4e38 else (INF if x > 0 else -INF)
    return y == 0.0 or abs(y) >= 1.1754943508222875e-38 or math.isinf(y)


def to_f32_f64(x):
    if math.isnan(x):
        return NAN
    try:
        return f32(x)
    except OverflowError:
        return INF if x > 0 else -INF


KERNELS = []


def kernel(name, builder, nin, width, cases, oracle, esize=8):
    KERNELS.append((name, builder, nin, width, cases, oracle, esize))


def bits32(x):
    return struct.unpack("<I", struct.pack("<f", x))[0]


def div32(a, b):
    """IEEE f32 division of f32 values: the f64 quotient rounded to f32 is correctly rounded
    (53 >= 2*24 + 2); Python raises where IEEE returns inf / NaN."""
    if math.isnan(a) or math.isnan(b):
        return NAN
    if b == 0.0:
        if a == 0.0:
            return NAN
        return math.copysign(INF, a) * math.copysign(1.0, b)
    if math.isinf(a) and math.isinf(b):
        return NAN
    return to_f32_f64(a / b)


def specials():
    return [0.0, -0.0, MIN_SUB, -MIN_SUB, 2.5e-310, MIN_NORM, 1.0, -1.0, 1.0 / 3.0, math.pi,
            1e300, -1e300, MAXF, INF, -INF, NAN]


def sets_of(values, r, fill):
    """Exactly SETS*LANES inputs: the listed values first, then fill(r) values."""
    v = list(values)[:SETS * LANES]
    while len(v) < SETS * LANES:
        v.append(fill(r))
    return v


def build_cases():
    r = random.Random(0x4D414244)   # "MABD"
    n = SETS * LANES
    # fma: fused; every class, then random any-exponent normals
    a = sets_of(specials() + [1e-300, 3e-200], r, rbits_normal)
    b = sets_of([1.0, 2.0, -0.5, 1e10, 3.0, 7.0, 1e-10, 1e300, 0.5, 0.0, 1.0, -1.0, 2.0, 1.0, 1.0, 1.0],
                r, rbits_normal)
    c = sets_of([0.0, -0.0, 1.0, -1.0, MIN_SUB, 0.1, -1e300, 1e300, 1.0, 1.0, MAXF, -MAXF, 1.0, -INF, 0.0, 1.0],
                r, rbits_normal)
    kernel("fma", "_spv_build_f64_fma", 3, 1, list(zip(a, b, c)), lambda a, b, c: fma(a, b, c))
    # arith: max(min(a*b + c, b), a), no NaN or +-0 ties in min/max
    ar = []
    while len(ar) < n:
        x, y, z = rnd(r), rnd(r), rnd(r)
        t = x * y + z
        if len({t, y, x}) == 3:
            ar.append((x, y, z))
    kernel("arith", "_spv_build_f64_arith", 3, 1, ar, lambda a, b, c: fmax(fmin(a * b + c, b), a))
    # cvt: f64 -> f32 -> f64
    cv = [x for x in specials() + [0.1, 1e38, 3.5e38, -3.5e38, 1.0000001, 16777217.0, 1e-40] if cvt_ok(x)]
    while len(cv) < n:
        x = rbits_normal(r) * r.choice([1.0, 1e-280, 1e280])
        if math.isfinite(x) and cvt_ok(x):
            cv.append(x)
    kernel("cvt", "_spv_build_f64_cvt", 1, 1, [(x,) for x in cv[:n]], lambda a: to_f32_f64(a))
    # subabs: |a - b|
    sa = sets_of(specials() + [MIN_NORM, 1.5], r, rbits_normal)
    sb = sets_of([0.0, 0.0, -MIN_SUB, MIN_SUB, 2.5e-310, MIN_SUB, 1.0, 2.0, 1.0 / 3.0, 3.0, 1e300, 1e300,
                  -MAXF, INF, -INF, 1.0, MIN_NORM * 0.5, 1.5], r, rbits_normal)
    kernel("subabs", "_spv_build_f64_subabs", 2, 1, list(zip(sa, sb)), lambda a, b: abs(a - b))
    # vec: dvec2 a + b per component
    va = [(rbits_normal(r), rbits_normal(r)) for _ in range(n)]
    vb = [(rbits_normal(r) if i % 3 else -va[i][0], rbits_normal(r)) for i in range(n)]
    vcases = [(va[i], vb[i]) for i in range(n)]
    for i, (x, y) in enumerate([(MAXF, MAXF), (INF, -INF), (MIN_SUB, MIN_SUB), (-0.0, -0.0)]):
        vcases[i] = ((x, 1.0), (y, -1.0))
    kernel("vec", "_spv_build_f64_vec", 2, 2, vcases, lambda a, b: a + b)
    # sqrt: subnormals, zeros, inf, negatives (NaN), random positives of any exponent
    sq = [0.0, -0.0, MIN_SUB, 2.5e-310, 1.5e-308, MIN_NORM, 1.0, 2.0, 4.0, MAXF, INF, -1.0, -MIN_SUB,
          NAN, 1e-300, 9.0]
    while len(sq) < n:
        sq.append(abs(rbits_normal(r)))
    kernel("sqrt", "_spv_build_f64_sqrt", 1, 1, [(x,) for x in sq], lambda a: math.sqrt(a) if a >= 0 or math.isnan(a) else NAN)
    # i32_cvt: (double)(int)trunc(a), in range only
    ic = [0.0, -0.0, 0.5, -0.5, 1.9999, -1.9999, 2147483647.0, -2147483648.0, 2147483647.99, -2147483648.99,
          123456.789, -98765.4321, MIN_SUB, -MIN_SUB, 1e9 + 0.5, -1e9 - 0.5]
    while len(ic) < n:
        ic.append(r.uniform(-2147483648.0, 2147483647.0))
    kernel("i32_cvt", "_spv_build_f64_i32_cvt", 1, 1, [(x,) for x in ic], lambda a: float(math.trunc(a)))
    # const: fma(fma(a, 0.5, 1.5), a, 2.5)
    co = sets_of(specials(), r, rbits_normal)
    kernel("const", "_spv_build_f64_const", 1, 1, [(x,) for x in co],
           lambda a: fma(fma(a, 0.5, 1.5), a, 2.5))
    # ldexp: a * 2^3 (subnormals scale exactly; near the top overflows)
    lx = specials() + [MAXF / 8.0, MAXF / 4.0, 2.0 ** 1020, 2.0 ** -1077, 2.0 ** -1072, 3 * MIN_SUB]
    lx = sets_of(lx, r, rbits_normal)
    kernel("ldexp", "_spv_build_f64_ldexp", 1, 1, [(x,) for x in lx], lambda a: ldexp(a, 3))
    # select: a > b ? a : b (ordered: a NaN picks b)
    se_a = sets_of(specials(), r, rbits_normal)
    se_b = sets_of([0.0, 0.0, 0.0, MIN_SUB, 1.0, 1.0, 0.5, 1.0, 0.0, 3.0, 1e301, 1e300, 1.0, 1.0, 0.0, 1.0],
                   r, rbits_normal)
    kernel("select", "_spv_build_f64_select", 2, 1, list(zip(se_a, se_b)), lambda a, b: a if a > b else b)
    # f32_div (F.b): f32 a / b — zeros, infinities, NaN, the largest finite, overflow, and random
    # pairs of any exponent. No f32 subnormal input or result: Vulkan lets an implementation flush
    # them (absent the DenormPreserve execution mode), GFX9 does here, the SM75 path keeps them.
    F32MAX = 3.4028234663852886e38
    F32MIN = 1.1754943508222875e-38
    D1 = 1.401298464324817e-45      # the smallest f32 subnormal
    fa = [0.0, -0.0, 1.0, -1.0, 0.0, INF, -INF, NAN, 1.0, 22.0, F32MAX, F32MIN, D1, 1e-40, 1.0, -3.0,
          2.0, F32MAX, 6.0, 1.0, INF, 0.0, 7.0, F32MIN]
    fb = [1.0, 3.0, 0.0, -0.0, 0.0, INF, 2.0, 1.0, NAN, 7.0, 0.5, 4.0, 2.0, 3.0, F32MAX, INF,
          F32MAX, F32MIN, 3.0, 3.0, 1.0, -5.0, -INF, 3.0]
    while len(fa) < n:
        ea, eb = r.randint(-120, 120), r.randint(-120, 120)
        fa.append(f32(r.uniform(1.0, 2.0) * 2.0 ** ea * r.choice([1, -1])))
        fb.append(f32(r.uniform(1.0, 2.0) * 2.0 ** eb * r.choice([1, -1])))
    fa = [f32(x) if math.isfinite(x) else x for x in fa]
    fb = [f32(x) if math.isfinite(x) else x for x in fb]

    def sub32(x):   # an f32 subnormal: Vulkan lets an implementation flush it (GFX9 here does)
        return x != 0.0 and math.isfinite(x) and abs(x) < F32MIN

    pairs = [(x, y) for x, y in zip(fa, fb) if not (sub32(x) or sub32(y) or sub32(div32(x, y)))]
    while len(pairs) < n:
        x = f32(r.uniform(1.0, 2.0) * 2.0 ** r.randint(-60, 60))
        y = f32(r.uniform(1.0, 2.0) * 2.0 ** r.randint(-60, 60))
        pairs.append((x, y))
    kernel("f32_div", "_spv_build_f32_div", 2, 1, pairs[:n], div32, 4)
    # div (F.c): f64 a / b over the whole range. Subnormal results include exact ties (3*2^-1074 / 2)
    # and near-ties; overflow and underflow edges; every special pair; random pairs of any exponent.
    T = 2.0 ** -1074
    dcase = [(1.0, 3.0), (22.0, 7.0), (2.0, 3.0), (355.0, 113.0), (-7.0, 4.0), (1e10, 3.0), (1.0, 1e6),
             (10.0, 4.0),
             (0.0, 1.0), (-0.0, 5.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 0.0), (INF, 2.0), (2.0, INF),
             (INF, INF), (NAN, 1.0), (1.0, NAN), (-INF, -0.0), (-0.0, -INF),
             (3 * T, 2.0), (5 * T, 2.0), (T, 2.0), (T, 3.0), (2.0 ** -1022, 3.0), (2.0 ** -1022, 1.5),
             (MIN_SUB, 0.5), (2.5e-310, 7.0), (1.0, 2.0 ** 1023), (3.0, MAXF), (MAXF, 0.5), (MAXF, 2.0),
             (MAXF, MAXF), (2.0 ** -1000, 2.0 ** 60), (1e-300, 1e300), (1e300, 1e-300), (MIN_SUB, MAXF),
             (MAXF, MIN_SUB), (2.0 ** -1060, 2.0 ** -5), (7 * T, 3.0)]
    while len(dcase) < 2 * n:
        x, y = rbits_normal(r), rbits_normal(r)
        if r.random() < 0.3:
            y = y * 2.0 ** r.randint(-600, 600) if math.isfinite(y * 2.0 ** 600) else y
        dcase.append((x, y))
    # two kernels' worth of sets: 64 cases
    kernel("div", "_spv_build_f64_div", 2, 1, dcase[:n], div64)
    kernel("div_b", "_spv_build_f64_div", 2, 1, dcase[n:2 * n], div64)
    # div2 (F.c): (a / b) / c, two divisions in one kernel (past the 256-id default space).
    d2 = [(1.0, 3.0, 7.0), (MAXF, 0.5, 2.0), (MAXF, 0.5, 0.25), (3 * T, 2.0, 0.5), (1.0, 0.0, INF),
          (0.0, 0.0, 1.0), (INF, 2.0, INF), (-1.0, 3.0, -0.0), (2.0 ** -1022, 3.0, 2.0 ** -60),
          (1e300, 1e-300, 1e-10), (22.0, 7.0, 355.0 / 113.0), (NAN, 1.0, 1.0)]
    while len(d2) < n:
        d2.append((rbits_normal(r), rbits_normal(r), rbits_normal(r)))
    kernel("div2", "_spv_build_f64_div2", 3, 1, d2[:n], lambda a, b, c: div64(div64(a, b), c))
    # store_const: out = 2.5
    kernel("store_const", "_spv_build_f64_store_const", 0, 1, [() for _ in range(n)], lambda: 2.5)


def comps(v, width):
    return list(v) if width == 2 else [v]


def build():
    build_cases()
    o = []
    o.append("# GENERATED by scripts/gen-f64-fixtures.py — do not edit. `--check` (make check-f64-fixtures)")
    o.append("# catches drift.")
    o.append("#")
    o.append("# The f64 corpus's cross-vendor oracle (4.4.0, the fourth N9 follow-on): per kernel, F64K_SETS")
    o.append("# input sets of F64K_LANES lanes (each lane F64K_width(k) f64s) and the correctly rounded expected")
    o.append("# outputs, from Python's IEEE binary64. f64k_fill(k, set, in0, in1, in2, want) writes them;")
    o.append("# f64k_build(k, buf) builds the kernel (tests/tcyr/compiler_common.cyr). A NaN want is the")
    o.append("# canonical quiet NaN and matches any NaN. f64k_esize(k): 8 (f64) or 4 (f32_div: F.b); the")
    o.append("# expected values sit in 8-byte slots either way.")
    o.append("")
    o.append(f"var F64K_COUNT = {len(KERNELS)};")
    o.append(f"var F64K_SETS = {SETS};")
    o.append(f"var F64K_LANES = {LANES};")
    o.append("")

    def table(fn, args, vals, default):
        o.append(f"fn {fn}({args}): i64 {{")
        for k, v in enumerate(vals):
            o.append(f"    if (k == {k}) {{ return {v}; }}")
        o.append(f"    return {default};")
        o.append("}")
        o.append("")

    table("f64k_name", "k", [f'"{kk[0]}"' for kk in KERNELS], '"?"')
    table("f64k_build", "k, b", [f"{kk[1]}(b)" for kk in KERNELS], "0")
    table("f64k_nin", "k", [kk[2] for kk in KERNELS], "0")
    table("f64k_width", "k", [kk[3] for kk in KERNELS], "1")
    table("f64k_esize", "k", [kk[6] for kk in KERNELS], "8")
    for k, (name, _, nin, width, cases, oracle, esize) in enumerate(KERNELS):
        st_op = "store64" if esize == 8 else "store32"
        enc = bits if esize == 8 else bits32
        qnan = 0x7FF8000000000000 if esize == 8 else 0x7FC00000
        hexw = 16 if esize == 8 else 8
        for st in range(SETS):
            o.append(f"fn _f64k_{name}_{st}(i0, i1, i2, w): i64 {{")
            for lane in range(LANES):
                case = cases[st * LANES + lane]
                if width == 2:
                    outs = [oracle(*[case[b][cmp] for b in range(nin)]) for cmp in range(2)]
                else:
                    outs = [oracle(*case)]
                for b in range(nin):
                    for cmp, v in enumerate(comps(case[b], width)):
                        o.append(f"    {st_op}(i{b} + {(lane * width + cmp) * esize}, 0x{enc(v):0{hexw}X});")
                for cmp, v in enumerate(outs):
                    vb = qnan if math.isnan(v) else enc(v)
                    o.append(f"    {st_op}(w + {(lane * width + cmp) * 8}, 0x{vb:0{hexw}X});")
            o.append("    return 0;")
            o.append("}")
            o.append("")
    o.append("fn f64k_fill(k, s, i0, i1, i2, w): i64 {")
    for k, kk in enumerate(KERNELS):
        for st in range(SETS):
            o.append(f"    if (k == {k}) {{ if (s == {st}) {{ return _f64k_{kk[0]}_{st}(i0, i1, i2, w); }} }}")
    o.append("    return 0 - 1;")
    o.append("}")
    return "\n".join(o) + "\n"


def main():
    text = build()
    if "--check" in sys.argv:
        cur = open(OUT).read() if os.path.exists(OUT) else ""
        if cur != text:
            print(f"{OUT} is stale — run scripts/gen-f64-fixtures.py")
            return 1
        print("f64_cases.cyr up to date")
        return 0
    open(OUT, "w").write(text)
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
