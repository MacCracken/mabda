#!/usr/bin/env python3
"""gen-spirv-cf-fixtures.py — build tests/tcyr/spirv_cf_kernels.cyr from tests/spirv/control_flow/.

The v4.4.0 control-flow kernels (divergent if / else, nested ifs, divergent loops, int-compare
OpSelect, conditions whose flag a later instruction overwrote) are written in SPIR-V assembly so
their instruction order is exact. This script assembles each one (`spirv-as`), checks it with
`spirv-val`, and writes a Cyrius fixture both the CPU suite (tests/tcyr/compiler_compile.tcyr)
and the hardware gate (programs/native_spirv_control_flow_e2e.cyr) include.

Each kernel is head.spvasm + <name>.body (or head_<h>.spvasm with a `; head: <h>` line). A
body's first lines say what it must do:
    ; want: <python expression over lane i>   (D = 0xDEADBEEF, the untouched seed; max, min
                                              and lcg(x, steps) are available)
    ; refuse: <mabda error constant>          (the compile must fail with exactly that code)
and may override either for the NVIDIA (SM75) compiler, where the hardware differs:
    ; nv-want: <python expression over lane i>
    ; nv-refuse: <mabda error constant>
(e.g. an OpReturn inside a divergent loop is refused on AMD and legal on NVIDIA).
Three kernels are generated here rather than written: if_nest8 (eight nested divergent ifs,
the GISEL_CF_MAX_DEPTH limit), if_nest9 (one past it), and oob_label (if_div with its merge
label renumbered past id_bound — the crafted module that crashed the compile frame before 4.4.0).

Needs spirv-as / spirv-val (SPIRV-Tools) only to REGENERATE; the generated file is checked in.

USAGE
    scripts/gen-spirv-cf-fixtures.py           write the fixture
    scripts/gen-spirv-cf-fixtures.py --check   exit 1 if the checked-in fixture is stale
"""
import os
import struct
import subprocess
import sys
import tempfile

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
SRC = os.path.join(ROOT, "tests", "spirv", "control_flow")
OUT = os.path.join(ROOT, "tests", "tcyr", "spirv_cf_kernels.cyr")
D = 0xDEADBEEF
LANES = 64


def nest_body(depth):
    """`depth` nested divergent ifs; level k's then-block stores k, then opens level k+1."""
    consts = {1: "%c1", 2: "%c2", 3: "%c3", 4: "%c4", 5: "%c5", 6: "%c6", 7: "%c7", 8: "%c8", 9: "%c9"}
    lines = []
    for k in range(1, depth + 1):
        lines += [
            f"        %n{k} = OpUGreaterThanEqual %bool %av {consts[k]}",
            f"               OpSelectionMerge %m{k} None",
            f"               OpBranchConditional %n{k} %t{k} %m{k}",
            f"        %t{k} = OpLabel",
            f"               OpStore %po {consts[k]}",
        ]
    for k in range(depth, 0, -1):
        lines += [f"               OpBranch %m{k}", f"        %m{k} = OpLabel"]
    lines += ["               OpReturn", "               OpFunctionEnd"]
    return "\n".join(lines) + "\n"


def lcg(x, steps):
    """`steps` of x = x * 5 + 7 mod 2^32 (loop_lcg's chain)."""
    for _ in range(steps):
        x = (x * 5 + 7) & 0xFFFFFFFF
    return x


def assemble(text):
    with tempfile.TemporaryDirectory() as td:
        src = os.path.join(td, "k.spvasm")
        out = os.path.join(td, "k.spv")
        open(src, "w").write(text)
        subprocess.run(["spirv-as", "--target-env", "vulkan1.1", src, "-o", out], check=True)
        subprocess.run(["spirv-val", "--target-env", "vulkan1.1", out], check=True)
        data = open(out, "rb").read()
    return list(struct.unpack("<%dI" % (len(data) // 4), data))


def head_of(body):
    for line in body.splitlines():
        s = line.strip()
        if s.startswith("; head:"):
            return s[len("; head:"):].strip()
    return ""


def meta_of(body):
    kind, val = None, None
    for line in body.splitlines():
        s = line.strip()
        if s.startswith("; want:"):
            kind, val = "want", s[len("; want:"):].strip()
        if s.startswith("; refuse:"):
            kind, val = "refuse", s[len("; refuse:"):].strip()
    if kind is None:
        raise SystemExit("kernel body has no '; want:' or '; refuse:' line")
    return kind, val


def nv_meta_of(body):
    for line in body.splitlines():
        s = line.strip()
        if s.startswith("; nv-want:"):
            return "want", s[len("; nv-want:"):].strip()
        if s.startswith("; nv-refuse:"):
            return "refuse", s[len("; nv-refuse:"):].strip()
    return None


def renumber_label(words, old, new):
    """Rewrite label <id> `old` to `new` wherever OpLabel / OpBranch / OpBranchConditional /
    OpSelectionMerge name it (the header's id_bound is left alone)."""
    w = words[:]
    i = 5
    while i < len(w):
        wc, op = w[i] >> 16, w[i] & 0xFFFF
        if op in (247, 248, 249, 250):
            for j in range(1, wc):
                if w[i + j] == old:
                    w[i + j] = new
        i += wc
    return w


def labels_of(words):
    out, i = [], 5
    while i < len(words):
        wc, op = words[i] >> 16, words[i] & 0xFFFF
        if op == 248:
            out.append(words[i + 1])
        i += wc
    return out


def w4_lines(fn_off_var, words, start):
    lines = []
    for j in range(0, len(words), 4):
        chunk = words[j:j + 4] + [0] * (4 - len(words[j:j + 4]))
        n = min(4, len(words) - j)
        vals = ", ".join("0x%08X" % x for x in chunk)
        lines.append(f"    _cfk_w4({fn_off_var}, {start + j}, {n}, {vals});")
    return lines


def head_text(h):
    return open(os.path.join(SRC, "head_%s.spvasm" % h if h else "head.spvasm")).read()


def build():
    kernels = []   # (name, head group, assembled text, meta)
    nv = {}        # name -> NVIDIA override (kind, val) or None
    for fn in sorted(os.listdir(SRC)):
        if fn.endswith(".body"):
            body = open(os.path.join(SRC, fn)).read()
            h = head_of(body)
            kernels.append((fn[:-5], h, head_text(h) + body, meta_of(body)))
            nv[fn[:-5]] = nv_meta_of(body)
    kernels.append(("if_nest8", "", head_text("") + nest_body(8), ("want", "D if i == 0 else min(i, 8)")))
    kernels.append(("if_nest9", "", head_text("") + nest_body(9), ("refuse", "GISEL_ERR_NESTED_SELECTION")))
    # NVIDIA keeps a PC per thread: no saved-EXEC pairs, so no nesting limit.
    nv["if_nest9"] = ("want", "D if i == 0 else min(i, 9)")
    words = {}
    for name, _, text, _ in kernels:
        words[name] = assemble(text)
    # oob_label: if_div with its merge label renumbered to 5000 while the header still says id_bound.
    base = words["if_div"]
    words["oob_label"] = renumber_label(base, labels_of(base)[-1], 5000)
    kernels.append(("oob_label", "", None, ("refuse", "MIR_ERR_ID_OOR")))

    # Per head group, the shared prefix (bound word zeroed) every kernel of the group starts with.
    names = [k[0] for k in kernels]
    groups = sorted(set(k[1] for k in kernels))
    prefix = {}
    for g in groups:
        members = [k[0] for k in kernels if k[1] == g]
        norm = {n: [0 if i == 3 else x for i, x in enumerate(words[n])] for n in members}
        pre = norm[members[0]]
        for n in members[1:]:
            k = 0
            while k < min(len(pre), len(norm[n])) and pre[k] == norm[n][k]:
                k += 1
            pre = pre[:k]
        prefix[g] = pre

    o = []
    o.append("# GENERATED by scripts/gen-spirv-cf-fixtures.py from tests/spirv/control_flow/ — do not edit.")
    o.append("# Regenerate after changing a kernel; `--check` (in `make check-cf-fixtures`) catches drift.")
    o.append("#")
    o.append("# The v4.4.0 control-flow kernels. cfk_build(k, buf) writes kernel k's SPIR-V into buf")
    o.append(f"# (>= {max(len(words[n]) for n in names) * 4} bytes) and returns its byte length;")
    o.append("# cfk_want(k, out) fills out[64] (u32) with the lane values a run must produce, or")
    o.append("# cfk_refuse(k) is the NEGATED error the compile must return (0 = must compile).")
    o.append("")
    o.append(f"var CFK_COUNT = {len(names)};")
    o.append(f"var CFK_MAX_BYTES = {max(len(words[n]) for n in names) * 4};")
    o.append("")
    o.append("fn _cfk_w4(b, i, n, w0, w1, w2, w3): i64 {")
    o.append("    store32(b + i * 4, w0);")
    o.append("    if (n > 1) { store32(b + i * 4 + 4, w1); }")
    o.append("    if (n > 2) { store32(b + i * 4 + 8, w2); }")
    o.append("    if (n > 3) { store32(b + i * 4 + 12, w3); }")
    o.append("    return 0;")
    o.append("}")
    o.append("")
    for g in groups:
        pre = prefix[g]
        hname = "head_%s.spvasm" % g if g else "head.spvasm"
        o.append(f"# {hname} + the entry block prologue ({len(pre)} words; word 3, id_bound, is per kernel).")
        o.append(f"fn _cfk_head{'_' + g if g else ''}(b): i64 {{")
        o += w4_lines("b", pre, 0)
        o.append(f"    return {len(pre)};")
        o.append("}")
        o.append("")
    for idx, (name, g, _, (kind, val)) in enumerate(kernels):
        w = words[name]
        plen = len(prefix[g])
        o.append(f"# [{idx}] {name} — {kind}: {val}")
        o.append(f"fn _cfk_{name}(b): i64 {{")
        o.append(f"    _cfk_head{'_' + g if g else ''}(b);")
        o.append(f"    store32(b + 12, {w[3]});   # id_bound")
        o += w4_lines("b", w[plen:], plen)
        o.append(f"    return {len(w) * 4};")
        o.append("}")
        o.append("")
    o.append("fn cfk_build(k, b): i64 {")
    for idx, (name, _, _, _) in enumerate(kernels):
        o.append(f"    if (k == {idx}) {{ return _cfk_{name}(b); }}")
    o.append("    return 0;")
    o.append("}")
    o.append("")
    o.append("fn cfk_name(k): i64 {")
    for idx, (name, _, _, _) in enumerate(kernels):
        o.append(f'    if (k == {idx}) {{ return "{name}"; }}')
    o.append('    return "?";')
    o.append("}")
    o.append("")
    o.append("fn cfk_refuse(k): i64 {")
    for idx, (name, _, _, (kind, val)) in enumerate(kernels):
        if kind == "refuse":
            o.append(f"    if (k == {idx}) {{ return 0 - {val}; }}")
    o.append("    return 0;")
    o.append("}")
    o.append("")
    def want_fn(fname, pick):
        o.append(f"fn {fname}(k, out): i64 {{")
        for idx, (name, _, _, meta) in enumerate(kernels):
            kind, val = pick(name, meta)
            if kind != "want":
                continue
            vals = [eval(val, {"D": D, "max": max, "min": min, "lcg": lcg}, {"i": i}) & 0xFFFFFFFF
                    for i in range(LANES)]
            o.append(f"    if (k == {idx}) {{")
            o.extend(["    " + line for line in w4_lines("out", vals, 0)])
            o.append("        return 1;")
            o.append("    }")
        o.append("    return 0;")
        o.append("}")

    want_fn("cfk_want", lambda name, meta: meta)
    o.append("")
    o.append("# The NVIDIA (SM75) expectations: the shared ones, except where a body says `; nv-want:` /")
    o.append("# `; nv-refuse:` (an OpReturn in a divergent region and deep nesting are legal there; f64 is")
    o.append("# not selected for NVIDIA yet).")
    o.append("fn cfk_nv_refuse(k): i64 {")
    for idx, (name, _, _, (kind, val)) in enumerate(kernels):
        ov = nv.get(name)
        if ov is not None:
            kind, val = ov
        if kind == "refuse":
            o.append(f"    if (k == {idx}) {{ return 0 - {val}; }}")
    o.append("    return 0;")
    o.append("}")
    o.append("")
    want_fn("cfk_nv_want", lambda name, meta: nv.get(name) or meta)
    return "\n".join(o) + "\n"


def main():
    text = build()
    if "--check" in sys.argv:
        cur = open(OUT).read() if os.path.exists(OUT) else ""
        if cur != text:
            print(f"{OUT} is stale — run scripts/gen-spirv-cf-fixtures.py")
            return 1
        print("spirv_cf_kernels.cyr up to date")
        return 0
    open(OUT, "w").write(text)
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
