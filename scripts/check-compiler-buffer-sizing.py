#!/usr/bin/env python3
"""check-compiler-buffer-sizing.py — static gate for the compiler's scratch-buffer contract.

THE CONTRACT (mabda 4.4.0)
--------------------------
Every SPIR-V->GFX9 compiler stage that writes a caller buffer takes that buffer
WITH ITS LENGTH IN BYTES, and derives its capacity from the length:

    mir_mod_init(m, vals, vals_len, instrs, instrs_len, ptrs, ptrs_len,
                 blocks, blocks_len, id_bound)                     src/mir.cyr
    spirv_build_{type,const,decoration}_table(p, n, out, out_len)  src/spirv_parse.cyr
    _spirv_resolve_builtins(p, n, idb, out_bi, out_bi_len)
    spirv_lower_module(p, n, types, consts, deco, m, out_bi, out_bi_len)
                                                                   src/spirv_lower.cyr
    gfx9_isel(m, out, out_len)                                     src/gfx9_isel.cyr
    gfx9_regalloc(m, isel, n, sb, vb, sc, vc, alloc, alloc_len, hw) src/gfx9_regalloc.cyr
    gfx9_waitcnt(m, in, n, out, out_len)                           src/gfx9_waitcnt.cyr
    gfx9_emit_program(m, isel, n, alloc, abi, buf, pos, buf_len)
    gfx9_compile(cc, spirv, n, isa, isa_len, desc)
    gfx9_cc_buf(cc, CC_X, buf, len)                                src/gfx9_compile.cyr

So a callee can no longer write past a buffer whose length it was told. What it
cannot check is whether the length is TRUE: a caller that declares `var vals[1024]`
and passes 2048 still overruns. That mismatch is two integer literals a few lines
apart, so this gate checks it: for every (&buf, LEN) pair at a contract call, the
enclosing function's `var buf[N]` must satisfy LEN <= N.

⛔ WHY IT IS STRICT ABOUT LITERALS. The 4.0.x gate this replaces parsed only call
sites whose arguments were all integer literals, and skipped the rest without a
word. The one call site that compiles untrusted SPIR-V (`_native_shader_compile_spirv`,
src/backend_native.cyr) passed named values, so it was skipped — and it declared
`ptrs[2048]` (64 records) while telling mir_mod_init it held 128. The old gate also
read only the FIRST store on each line, so programs that packed three
`store64(&cc + CC_*, ..)` per line had `isel[1024]` (21 records) checked against
nothing while gfx9_compile assumed 256. Both fixed in 4.4.0. Hence:
  * a LOCAL array (`&name` with a `var name[N]` in the same fn) passed with a
    non-literal length is a FAILURE — write the literal so it can be checked —
    unless the length is a plain name the same function bounds first with
    `if (name > M)`, M <= N (that guard is the proof);
  * a buffer that is not a local array (a parameter, a computed pointer) is the
    caller's business and is skipped; that caller's own call is checked.

Also checked: mir_mod_init's id_bound must fit vals_len / MIR_VAL_REC when both are
literals (the init rejects it at runtime; this names the site).

USAGE
    scripts/check-compiler-buffer-sizing.py [--verbose]
Exit 0 = every pair adequately sized. Exit 1 = at least one problem.
"""
import glob
import os
import re
import sys

# Mirrored from src/mir.cyr; test_compiler_buffer_sizing_recs in
# tests/tcyr/compiler_lower.tcyr asserts it so the two cannot drift.
MIR_VAL_REC = 40
MIR_MAX_CAP_IDS = 256

# fn name -> (arg count, [(buf arg index, len arg index), ...])
CONTRACT = {
    "mir_mod_init": (10, [(1, 2), (3, 4), (5, 6), (7, 8)]),
    "spirv_build_type_table": (4, [(2, 3)]),
    "spirv_build_const_table": (4, [(2, 3)]),
    "spirv_build_decoration_table": (4, [(2, 3)]),
    "_spirv_resolve_builtins": (5, [(3, 4)]),
    "spirv_lower_module": (8, [(6, 7)]),
    "gfx9_isel": (3, [(1, 2)]),
    "gfx9_regalloc": (10, [(7, 8)]),
    "gfx9_waitcnt": (5, [(3, 4)]),
    "gfx9_emit_program": (8, [(5, 7)]),
    "gfx9_compile": (6, [(3, 4)]),
    "gfx9_cc_buf": (4, [(2, 3)]),
}

DECL = re.compile(r"\bvar\s+(\w+)\s*\[\s*(\d+)\s*\]")
FN_START = re.compile(r"^fn\s+\w+\s*\(", re.M)
CALL = re.compile(r"\b(" + "|".join(re.escape(k) for k in CONTRACT) + r")\s*\(")
LOCAL_REF = re.compile(r"&\s*(\w+)")
INT = re.compile(r"\d+")


def sources():
    out = []
    for root in ("src", "programs", "tests", "fuzz"):
        for ext in ("*.cyr", "*.tcyr", "*.bcyr", "*.fcyr"):
            out += glob.glob(os.path.join(root, "**", ext), recursive=True)
    return sorted(set(out))


def strip_comments(text):
    """Blank `#` comments (outside strings) so a call quoted in a comment is not a call.
    Keeps offsets and newlines intact."""
    out = []
    in_str = False
    in_cmt = False
    i = 0
    while i < len(text):
        c = text[i]
        if in_cmt:
            if c == "\n":
                in_cmt = False
                out.append(c)
            else:
                out.append(" ")
        elif in_str:
            out.append(c)
            if c == "\\" and i + 1 < len(text):
                out.append(text[i + 1])
                i += 1
            elif c == '"':
                in_str = False
        elif c == '"':
            in_str = True
            out.append(c)
        elif c == "#":
            in_cmt = True
            out.append(" ")
        else:
            out.append(c)
        i += 1
    return "".join(out)


def split_args(text, open_paren):
    """Top-level comma split of the call whose '(' is at open_paren. Returns
    (args, index of the matching ')') or (None, None) if unbalanced."""
    depth = 0
    args = []
    cur = open_paren + 1
    in_str = False
    i = open_paren
    while i < len(text):
        c = text[i]
        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
        elif c == '"':
            in_str = True
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                args.append(text[cur:i].strip())
                return args, i
        elif c == "," and depth == 1:
            args.append(text[cur:i].strip())
            cur = i + 1
        i += 1
    return None, None


def fn_bounds(starts, pos, end_of_text):
    lo, hi = 0, end_of_text
    for s in starts:
        if s <= pos:
            lo = s
        else:
            hi = s
            break
    return lo, hi


def main():
    verbose = "--verbose" in sys.argv
    problems = []
    pairs = 0
    for path in sources():
        try:
            raw = open(path).read()
        except (OSError, UnicodeDecodeError):
            continue
        text = strip_comments(raw)
        if not CALL.search(text):
            continue
        starts = [m.start() for m in FN_START.finditer(text)]
        for m in CALL.finditer(text):
            name = m.group(1)
            line_start = text.rfind("\n", 0, m.start()) + 1
            if text[line_start:m.start()].lstrip().startswith("fn "):
                continue  # the definition
            nargs, plist = CONTRACT[name]
            args, _close = split_args(text, m.end() - 1)
            line = text.count("\n", 0, m.start()) + 1
            where = f"{path}:{line}: {name}"
            if args is None:
                problems.append(f"{where}: could not parse the call")
                continue
            if len(args) != nargs:
                problems.append(f"{where}: {len(args)} args, the contract signature takes {nargs}")
                continue
            lo, hi = fn_bounds(starts, m.start(), len(text))
            decls = {}
            for d in DECL.finditer(text, lo, hi):
                decls[d.group(1)] = max(int(d.group(2)), decls.get(d.group(1), 0))
            # A call that is itself an ASSERTION ARGUMENT may pass an incoherent id_bound
            # or a zero length on purpose — test_mir_mod_init_contract proves the init
            # rejects them. That exemption never covers a length past its buffer: that is
            # a real overrun whether or not an assertion wraps the call.
            stmt = text[max(text.rfind(";", 0, m.start()), text.rfind("{", 0, m.start()),
                            text.rfind("}", 0, m.start())) + 1:m.start()]
            in_assert = "assert" in stmt
            for bi, li in plist:
                buf, ln = args[bi], args[li]
                ref = re.fullmatch(r"&\s*(\w+)", buf)
                if not ref or ref.group(1) not in decls:
                    continue  # not a local array: the caller that owns it is checked
                have = decls[ref.group(1)]
                pairs += 1
                if not INT.fullmatch(ln) and re.fullmatch(r"\w+", ln):
                    # A named length is accepted when the SAME function bounds it first:
                    # an `if (NAME > N)` at or below the declared size, before the call
                    # (a test helper that shortens one buffer at a time does this).
                    guard = re.compile(r"\bif\s*\(\s*" + re.escape(ln) + r"\s*>\s*(\d+)\s*\)")
                    bound = [int(g.group(1)) for g in guard.finditer(text, lo, m.start())]
                    if bound and min(bound) <= have:
                        if verbose:
                            print(f"  ok {where} {ref.group(1)}[{have}] >= {ln} (guarded <= {min(bound)})")
                        continue
                if not INT.fullmatch(ln):
                    problems.append(
                        f"{where}: local {ref.group(1)}[{have}] passed with non-literal length "
                        f"'{ln}' — write the byte count so this gate can check it")
                    continue
                need = int(ln)
                # ⚠ A zero length on a call that is meant to work is almost always two
                # arguments swapped — gfx9_emit_program's (pos, buf_len) is the trap: the
                # 4.4.0 conversion first wrote `&built, 128, 0` (pos 128, length 0), which
                # this gate's `<=` passed.
                if need == 0 and not in_assert:
                    problems.append(
                        f"{where}: {ref.group(1)}[{have}] passed with length 0 — swapped arguments?")
                    continue
                if need > have:
                    problems.append(
                        f"{where}: {ref.group(1)}[{have}] passed as {need} bytes — "
                        f"OVERRUN {need - have} B")
                elif verbose:
                    print(f"  ok {where} {ref.group(1)}[{have}] >= {need}")
            if (name == "mir_mod_init" and not in_assert
                    and INT.fullmatch(args[2]) and INT.fullmatch(args[9])):
                cap = min(int(args[2]) // MIR_VAL_REC, MIR_MAX_CAP_IDS)
                if int(args[9]) > cap:
                    problems.append(
                        f"{where}: id_bound {args[9]} > cap_ids {cap} (vals_len {args[2]} / "
                        f"{MIR_VAL_REC}) — mir_mod_init rejects it")
    for p in problems:
        print(p)
    print(f"\n{pairs} (buffer, length) pair(s) checked, {len(problems)} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    sys.exit(main())
