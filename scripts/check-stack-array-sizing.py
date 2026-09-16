#!/usr/bin/env python3
"""check-stack-array-sizing.py — static gate: no write may land outside a `var NAME[N]`.

WHY THIS EXISTS
---------------
A function-local `var NAME[N]` is N BYTES (a module-global one is N * 8). A
stack array written past N bytes silently corrupts whatever the compiler placed
after it. Nothing fails at compile time, the tests usually still pass, and the
damage depends on the stack layout of the toolchain in use.

⛔ Not hypothetical. mabda shipped four of these
(docs/development/issues/2026-09-16-test-stack-array-overruns.md):

    tests/tcyr/texture.tcyr               var be[256]   <- memset(BACKEND_SIZE = 328) via _t5_wire
    tests/tcyr/queue.tcyr (9 tests)       var be[248]   <- memset(BACKEND_SIZE = 328)
    programs/native_array_sample_e2e.cyr  var px[20] / var py[20] <- 4 x store64 (32 B)
    programs/wgpu_texture_sample_e2e.cyr  var probes[3] <- 3 x store64 (24 B)

The texture.tcyr one is the "leading NUL byte" the docs warned about for months:
while array locals lived in .bss (cyrius <= 6.3.14) the 72-byte overrun zeroed
the "\\n" literal assert_summary() prints first. cyrius 6.3.15 moved array
locals to the stack and the symptom vanished; the overrun did not. Every one of
these is a compile-time fact (declared size and written extent are constants),
hence a static gate.

WHAT IT CHECKS
--------------
Objects: every `var NAME[N]` / `var NAME: T[N]` (local or global) and every
scalar local `var NAME = ...` (8 B) in src/ programs/ tests/ fuzz/ examples/.
Writes that reach `&NAME`:

  * memset / memcpy / memmove (dst, _, n)            -> n bytes at dst
  * store8 / store16 / store32 / store64 (p, v)      -> 1 / 2 / 4 / 8 bytes at p
  * syscall read / pread64 (buf, count), getrandom (buf, len),
    clock_gettime / gettimeofday (16 B), nanosleep's rem (16 B), and ioctl
    (argp) with a constant request whose direction has _IOC_READ
    -> _IOC_SIZE(request) bytes (drm_ioctl copies that many back)
  * a call to ANY function (same file first, then src/, then lib/) that
    writes through a parameter, transitively (helper -> helper), with the
    callee's offsets / sizes re-expressed in the caller's arguments.

Pointer forms: `&NAME`, `(&NAME)`, `&NAME + off`, `&NAME - off`, and
single-assignment locals initialised to one of those. Values resolve through
integer literals, `var C = <expr>;` globals never reassigned in any function
(the file itself, src/, lib/), enum members, single-assignment locals, and loop
counters (`var i = C; while (i < E) { ... i = i + K; }`, and the `for` form).

Reachability (sound, not heuristic): a callee write is skipped for a call whose
constant arguments make it unreachable — an earlier top-level
`if (COND) { ... return ...; }` whose COND is definitely true, or an enclosing
`if (COND) {` / `else {` whose COND is definitely false / true. That is what
lets a negative test pass `ctx = 0` or `n = 0` into a validator without being
flagged. Anything the gate cannot decide counts as reachable.

WHAT IT DOES NOT CLAIM
----------------------
It skips what it cannot resolve: a write whose offset or size depends on a
runtime value, a pointer stored in memory, or a reassigned pointer. A clean run
means "no PROVABLE out-of-bounds write", not "none at all". A flagged site that
is only safe because of a guard the gate cannot evaluate (a loaded field, a
function's return value) goes in ALLOWLIST with its reason; an entry that stops
matching fails the gate, so the list cannot rot.

USAGE
    scripts/check-stack-array-sizing.py [--root DIR] [--verbose]
    scripts/check-stack-array-sizing.py --self-test
Exit 0 = clean. Exit 1 = a finding, a stale ALLOWLIST entry, or a failed
self-test case. Exit 2 = setup error (lib/ not populated: run `cyrius deps`).
"""
import glob
import math
import os
import sys
import tempfile

# ---------------------------------------------------------------------------
# Allowlist: (file, function, variable, first callee in the chain) -> reason.
# Every entry must match at least one finding or the gate fails as stale.
# ---------------------------------------------------------------------------
ALLOWLIST = {}

SCAN_ROOTS = ("src", "programs", "tests", "fuzz", "examples")
SCAN_EXTS = (".cyr", ".tcyr", ".bcyr", ".fcyr")
# lib/ peers for other targets redefine the syscall enum with other numbers; the
# gate reasons about the x86_64 Linux build.
LIB_SKIP = ("syscalls_aarch64_linux.cyr", "syscalls_macos.cyr", "syscalls_windows.cyr",
            "syscalls_x86_64_agnos.cyr")
KEYWORDS = {"fn", "var", "if", "else", "while", "for", "return", "break", "continue",
            "enum", "struct", "include", "extern", "object"}
STORE_WIDTH = {"store8": 1, "store16": 2, "store32": 4, "store64": 8}
TYPE_WIDTH = {"i8": 1, "u8": 1, "i16": 2, "u16": 2, "i32": 4, "u32": 4, "f32": 4,
              "i64": 8, "u64": 8, "f64": 8, "ptr": 8}
MAX_DEPTH = 14
MAX_ENTRIES_PER_PARAM = 64
M64 = (1 << 64) - 1
ADDR = (1, 1 << 62)   # `&x`: some non-zero address


# ---------------------------------------------------------------------------
# Lexer — comments and string contents never produce tokens
# ---------------------------------------------------------------------------
class Tok:
    __slots__ = ("k", "v", "line")

    def __init__(self, k, v, line):
        self.k, self.v, self.line = k, v, line

    def __repr__(self):
        return str(self.v)


PUNCT2 = ("<<", ">>", "==", "!=", "<=", ">=", "&&", "||", "->")


def lex(text):
    toks = []
    i, n, line = 0, len(text), 1
    while i < n:
        c = text[i]
        if c == "\n":
            line += 1
            i += 1
        elif c in " \t\r":
            i += 1
        elif c == "#":
            while i < n and text[i] != "\n":
                i += 1
        elif c == '"':
            start = line
            j = i + 1
            while j < n and text[j] != '"':
                if text[j] == "\\":
                    j += 1
                elif text[j] == "\n":
                    line += 1
                j += 1
            toks.append(Tok("str", '""', start))
            i = j + 1
        elif c.isdigit():
            j = i
            if text.startswith(("0x", "0X"), i):
                j = i + 2
                while j < n and text[j] in "0123456789abcdefABCDEF_":
                    j += 1
                raw = text[i + 2:j].replace("_", "")
                val = int(raw, 16) if raw else None
            else:
                while j < n and (text[j].isdigit() or text[j] == "_"):
                    j += 1
                val = int(text[i:j].replace("_", ""))
                if j + 1 < n and text[j] == "." and text[j + 1].isdigit():
                    j += 1
                    while j < n and text[j].isalnum():
                        j += 1
                    val = None
            toks.append(Tok("num", val, line))
            i = j
        elif c.isalpha() or c == "_":
            j = i
            while j < n and (text[j].isalnum() or text[j] == "_"):
                j += 1
            toks.append(Tok("id", text[i:j], line))
            i = j
        else:
            two = text[i:i + 2]
            if two in PUNCT2:
                toks.append(Tok("p", two, line))
                i += 2
            else:
                toks.append(Tok("p", c, line))
                i += 1
    return toks


def match_close(toks, i, open_, close):
    """toks[i] is `open_`; the index of its matching `close` (or the last token)."""
    depth = 0
    for j in range(i, len(toks)):
        v = toks[j].v
        if v == open_:
            depth += 1
        elif v == close:
            depth -= 1
            if depth == 0:
                return j
    return len(toks) - 1


def split_top(toks, sep):
    out, cur, depth = [], [], 0
    for t in toks:
        if t.v in ("(", "["):
            depth += 1
        elif t.v in (")", "]"):
            depth -= 1
        if depth == 0 and t.k == "p" and t.v == sep:
            out.append(cur)
            cur = []
        else:
            cur.append(t)
    out.append(cur)
    return out


def strip_parens(toks):
    while len(toks) >= 2 and toks[0].v == "(" and match_close(toks, 0, "(", ")") == len(toks) - 1:
        toks = toks[1:-1]
    return toks


def num(v):
    return Tok("num", v, 0)


def paren(toks):
    return [Tok("p", "(", 0)] + list(toks) + [Tok("p", ")", 0)]


def key_of(toks):
    return tuple(t.v for t in toks)


# ---------------------------------------------------------------------------
# Structure
# ---------------------------------------------------------------------------
class Fn:
    def __init__(self, name, params, body, file, line):
        self.name, self.params, self.body, self.file, self.line = name, params, body, file, line
        self.decls = []            # (tok_index, name, size_toks, width, kind)
        self.assign_count = {}     # name -> assignments (a var initialiser counts as one)
        self.inits = {}            # name -> initialiser tokens
        self.plain_assigned = set()
        self.loops = []            # (body_open, body_close, cond_toks)
        self.ifs = []              # (then_open, then_close, else_open, else_close, cond_toks)
        self.guards = []           # (tok_index, cond_toks): top-level `if (C) { ... return }`
        self.calls = []            # (tok_index, callee, [arg toks], line)
        self.summary = {}          # param index -> [(off_toks, width_toks, how, conds)]

    def path_conds(self, site):
        """[(cond_toks, required_truth)] that must hold for control to reach `site`."""
        out = []
        for (k, cond) in self.guards:
            if k < site:
                out.append((cond, False))
        for (t_open, t_close, e_open, e_close, cond) in self.ifs:
            if t_open < site < t_close:
                out.append((cond, True))
            elif e_open is not None and e_open <= site <= e_close:
                out.append((cond, False))
        return out


class Module:
    def __init__(self, rel, is_lib):
        self.rel, self.is_lib = rel, is_lib
        self.fns = {}
        self.gconst_toks = {}
        self.garrays = {}       # name -> (size toks, width, line)
        self.enum_toks = {}     # name -> (toks or None, previous member, offset)
        self.assigned = set()


def parse_module(path, rel, is_lib):
    m = Module(rel, is_lib)
    toks = lex(open(path, encoding="utf-8", errors="replace").read())
    i, n, depth = 0, len(toks), 0
    while i < n:
        t = toks[i]
        if t.v == "{":
            depth += 1
        elif t.v == "}":
            depth -= 1
        if depth == 0 and t.v == "fn" and i + 2 < n and toks[i + 1].k == "id" and toks[i + 2].v == "(":
            pclose = match_close(toks, i + 2, "(", ")")
            params = []
            for part in split_top(toks[i + 3:pclose], ","):
                ids = [x.v for x in part if x.k == "id"]
                if ids:
                    params.append(ids[0])
            j = pclose + 1
            while j < n and toks[j].v not in ("{", ";"):
                j += 1
            if j >= n or toks[j].v == ";":
                i = j + 1
                continue
            bclose = match_close(toks, j, "{", "}")
            fn = Fn(toks[i + 1].v, params, toks[j + 1:bclose], rel, t.line)
            analyse_fn(fn)
            m.fns.setdefault(fn.name, fn)
            i = bclose + 1
            continue
        if depth == 0 and t.v == "enum":
            j = i + 1
            while j < n and toks[j].v != "{":
                j += 1
            if j >= n:
                break
            eclose = match_close(toks, j, "{", "}")
            members, cur = [], []
            for x in toks[j + 1:eclose]:
                if x.v in (";", ","):
                    members.append(cur)
                    cur = []
                else:
                    cur.append(x)
            members.append(cur)
            prev, off = None, 0
            for mem in members:
                if not mem or mem[0].k != "id":
                    continue
                if len(mem) >= 3 and mem[1].v == "=":
                    m.enum_toks[mem[0].v] = (mem[2:], None, 0)
                    prev, off = mem[0].v, 1
                else:
                    m.enum_toks[mem[0].v] = (None, prev, off)
                    off += 1
            i = eclose + 1
            continue
        if depth == 0 and t.v == "var" and i + 1 < n and toks[i + 1].k == "id":
            name = toks[i + 1].v
            end = i + 2
            while end < n and toks[end].v != ";":
                end += 1
            rest = toks[i + 2:end]
            if rest and rest[0].v == "=":
                m.gconst_toks[name] = rest[1:]
            elif rest and rest[0].v == "[":
                c = match_close(rest, 0, "[", "]")
                m.garrays[name] = (rest[1:c], 8, t.line)
            elif len(rest) > 2 and rest[0].v == ":" and rest[2].v == "[":
                c = match_close(rest, 2, "[", "]")
                m.garrays[name] = (rest[3:c], TYPE_WIDTH.get(rest[1].v, 8), t.line)
            i = end + 1
            continue
        i += 1
    for fn in m.fns.values():
        m.assigned |= fn.plain_assigned
    return m


def if_extent(b, k):
    """`b[k]` is `if`: (cond, then_open, then_close, else_open, else_close, end)."""
    if k + 1 >= len(b) or b[k + 1].v != "(":
        return None
    cclose = match_close(b, k + 1, "(", ")")
    t_open = cclose + 1
    if t_open >= len(b) or b[t_open].v != "{":
        return None
    t_close = match_close(b, t_open, "{", "}")
    cond = b[k + 2:cclose]
    if t_close + 1 < len(b) and b[t_close + 1].v == "else":
        e = t_close + 2
        if e < len(b) and b[e].v == "{":
            e_close = match_close(b, e, "{", "}")
            return cond, t_open, t_close, e, e_close, e_close
        if e < len(b) and b[e].v == "if":
            inner = if_extent(b, e)
            if inner is not None:
                return cond, t_open, t_close, e, inner[5], inner[5]
    return cond, t_open, t_close, None, None, t_close


def analyse_fn(fn):
    b = fn.body
    n = len(b)
    depth = 0
    k = 0
    while k < n:
        t = b[k]
        if t.v == "{":
            depth += 1
        elif t.v == "}":
            depth -= 1
        if t.v == "var" and k + 1 < n and b[k + 1].k == "id":
            names = []
            j = k + 1
            while j < n and b[j].k == "id":
                names.append(b[j].v)
                if j + 1 < n and b[j + 1].v == ",":
                    j += 2
                else:
                    j += 1
                    break
            end = j
            while end < n and b[end].v != ";":
                end += 1
            rest = b[j:end]
            if len(names) == 1 and rest and rest[0].v == "[":
                c = match_close(rest, 0, "[", "]")
                fn.decls.append((k, names[0], rest[1:c], 1, "array"))
                fn.assign_count[names[0]] = fn.assign_count.get(names[0], 0) + 2
            elif len(names) == 1 and len(rest) > 2 and rest[0].v == ":" and rest[2].v == "[":
                c = match_close(rest, 2, "[", "]")
                fn.decls.append((k, names[0], rest[3:c], TYPE_WIDTH.get(rest[1].v, 8), "array"))
                fn.assign_count[names[0]] = fn.assign_count.get(names[0], 0) + 2
            else:
                eq = next((x for x in range(len(rest)) if rest[x].v == "="), None)
                init = rest[eq + 1:] if eq is not None else None
                for nm in names:
                    fn.decls.append((k, nm, None, 8, "scalar"))
                    fn.assign_count[nm] = fn.assign_count.get(nm, 0) + (1 if len(names) == 1 and init else 2)
                    if len(names) == 1 and init:
                        fn.inits[nm] = init
            k = j   # keep scanning the initialiser: its calls are writes too
            continue
        if t.k == "id" and k + 1 < n and b[k + 1].v == "=" and (k == 0 or b[k - 1].v != "."):
            fn.assign_count[t.v] = fn.assign_count.get(t.v, 0) + 1
            fn.plain_assigned.add(t.v)
        if t.v in ("while", "for") and k + 1 < n and b[k + 1].v == "(":
            cclose = match_close(b, k + 1, "(", ")")
            header = b[k + 2:cclose]
            j = cclose + 1
            if j < n and b[j].v == "{":
                bclose = match_close(b, j, "{", "}")
                if t.v == "for":
                    parts = split_top(header, ";")
                    fn.loops.append((j, bclose, parts[1] if len(parts) == 3 else []))
                else:
                    fn.loops.append((j, bclose, header))
        if t.v == "if" and (k == 0 or b[k - 1].v != "else"):
            ext = if_extent(b, k)
            if ext is not None:
                register_if(fn, b, k, ext, depth)
        if t.k == "id" and t.v not in KEYWORDS and k + 1 < n and b[k + 1].v == "(":
            cclose = match_close(b, k + 1, "(", ")")
            inner = b[k + 2:cclose]
            fn.calls.append((k, t.v, split_top(inner, ",") if inner else [], t.line))
        k += 1


def register_if(fn, b, k, ext, depth):
    cond, t_open, t_close, e_open, e_close, _end = ext
    fn.ifs.append((t_open, t_close, e_open, e_close, cond))
    if depth == 0 and e_open is None:
        bd = 0
        for x in range(t_open + 1, t_close):
            if b[x].v == "{":
                bd += 1
            elif b[x].v == "}":
                bd -= 1
            elif bd == 0 and b[x].v == "return":
                fn.guards.append((k, cond))
                break
    if e_open is not None and b[e_open].v == "if":
        inner = if_extent(b, e_open)
        if inner is not None:
            register_if(fn, b, e_open, inner, depth + 1)


# ---------------------------------------------------------------------------
# Evaluation over integer intervals
# ---------------------------------------------------------------------------
def truth(v):
    """True / False when an interval's truthiness is decided, else None."""
    if v is None:
        return None
    if v == (0, 0):
        return False
    if v[0] > 0 or v[1] < 0:
        return True
    return None


def tdiv(a, c):
    q = abs(a) // abs(c)
    return q if (a >= 0) == (c > 0) else -q


def combine(op, a, b):
    if op == "&&":
        ta, tb = truth(a), truth(b)
        if ta is False or tb is False:
            return (0, 0)
        return (1, 1) if ta and tb else None
    if op == "||":
        ta, tb = truth(a), truth(b)
        if ta or tb:
            return (1, 1)
        return (0, 0) if ta is False and tb is False else None
    if a is None or b is None:
        return None
    (al, ah), (bl, bh) = a, b
    if op == "+":
        return (al + bl, ah + bh)
    if op == "-":
        return (al - bh, ah - bl)
    if op == "*":
        ps = (al * bl, al * bh, ah * bl, ah * bh)
        return (min(ps), max(ps))
    if op in ("<", "<=", ">", ">=", "==", "!="):
        if op == "<":
            return (1, 1) if ah < bl else (0, 0) if al >= bh else None
        if op == "<=":
            return (1, 1) if ah <= bl else (0, 0) if al > bh else None
        if op == ">":
            return (1, 1) if al > bh else (0, 0) if ah <= bl else None
        if op == ">=":
            return (1, 1) if al >= bh else (0, 0) if ah < bl else None
        if al == ah == bl == bh:
            return (1, 1) if op == "==" else (0, 0)
        if ah < bl or bh < al:
            return (0, 0) if op == "==" else (1, 1)
        return None
    if op == "&" and al >= 0 and bl >= 0 and (al != ah or bl != bh):
        return (0, min(ah, bh))
    if bl != bh:
        if al >= 0 and bl >= 0:
            if op == "<<":
                return (al << bl, ah << bh)
            if op == ">>":
                return (al >> bh, ah >> bl)
            if op == "/" and bl > 0:
                return (al // bh, ah // bl)
        return None
    c = bl
    if op == "/":
        return None if c == 0 else tuple(sorted((tdiv(al, c), tdiv(ah, c))))
    if op == "%":
        if c <= 0 or al < 0:
            return None
        return (al % c, al % c) if al == ah else (0, c - 1)
    if op == "<<":
        return (al << c, ah << c) if al >= 0 and c >= 0 else None
    if op == ">>":
        if c < 0:
            return None
        if al == ah:
            return (((al & M64) >> c),) * 2
        return (al >> c, ah >> c) if al >= 0 else None
    if al != ah:
        return None
    r = {"&": al & c, "|": al | c, "^": al ^ c}.get(op)
    return None if r is None else (r, r)


class Ctx:
    """Identifier resolution inside one function (fn None = a module's top level)."""

    def __init__(self, world, module, fn, site):
        self.world, self.module, self.fn, self.site = world, module, fn, site

    def eval(self, toks, depth=0):
        if depth > MAX_DEPTH or not toks:
            return None
        return Parser(toks, self, depth).parse()

    def lookup(self, name, depth):
        fn = self.fn
        if fn is not None:
            cnt = fn.assign_count.get(name)
            if name in fn.params and cnt is None:
                return None
            if cnt == 1 and name in fn.inits:
                return self.eval(fn.inits[name], depth + 1)
            if cnt is not None:
                return self.loop_counter(name, depth)
        return self.world.const_value(self.module, name, depth + 1)

    def loop_counter(self, name, depth):
        """`var i = C; while (i < E) { ... i = i + K; }` at a site inside the loop body."""
        fn = self.fn
        if self.site is None:
            return None
        b = fn.body
        for (b_open, b_close, cond) in fn.loops:
            if not (b_open < self.site < b_close):
                continue
            bound = None
            for conj in split_top(cond, "&&"):
                conj = strip_parens(conj)
                if len(conj) >= 3 and conj[0].v == name and conj[1].v in ("<", "<="):
                    e = self.eval(conj[2:], depth + 1)
                    if e is not None:
                        bound = e[1] - 1 if conj[1].v == "<" else e[1]
            if bound is None:
                continue
            inits, steps, before, ok = [], [], 0, True
            for k in range(len(b) - 1):
                if not (b[k].v == name and b[k + 1].v == "=" and (k == 0 or b[k - 1].v != ".")):
                    continue
                end = k + 2
                while end < len(b) and b[end].v not in (";", ")"):
                    end += 1
                rhs = b[k + 2:end]
                if k > 0 and b[k - 1].v == "var":
                    v = self.eval(rhs, depth + 1)
                    if v is None:
                        ok = False
                        break
                    inits.append(v)
                elif len(rhs) == 3 and rhs[0].v == name and rhs[1].v == "+" and rhs[2].k == "num" \
                        and rhs[2].v:
                    steps.append(rhs[2].v)
                    if b_open < k < self.site:
                        before += rhs[2].v
                else:
                    ok = False
                    break
            if not ok or len(inits) != 1 or not steps or min(steps) <= 0:
                continue
            lo, hi0 = inits[0]
            g = 0
            for s in steps:
                g = math.gcd(g, s)
            if lo == hi0 and bound >= lo:
                bound = lo + ((bound - lo) // g) * g
            return (lo, bound + before)
        return None


class Parser:
    PREC = [("||",), ("&&",), ("|",), ("^",), ("&",), ("==", "!="), ("<", "<=", ">", ">="),
            ("<<", ">>"), ("+", "-"), ("*", "/", "%")]

    def __init__(self, toks, ctx, depth):
        self.t, self.i, self.ctx, self.depth = toks, 0, ctx, depth

    def parse(self):
        try:
            v = self.binary(0)
        except (IndexError, ValueError, ZeroDivisionError, OverflowError):
            return None
        return v if self.i == len(self.t) else None

    def binary(self, level):
        if level == len(self.PREC):
            return self.unary()
        lhs = self.binary(level + 1)
        while self.i < len(self.t) and self.t[self.i].k == "p" and self.t[self.i].v in self.PREC[level]:
            op = self.t[self.i].v
            self.i += 1
            lhs = combine(op, lhs, self.binary(level + 1))
        return lhs

    def unary(self):
        t = self.t[self.i]
        if t.v == "-":
            self.i += 1
            v = self.unary()
            return None if v is None else (-v[1], -v[0])
        if t.v == "!":
            self.i += 1
            tv = truth(self.unary())
            return None if tv is None else ((0, 0) if tv else (1, 1))
        if t.v == "&" and self.i + 1 < len(self.t) and self.t[self.i + 1].k == "id":
            self.i += 2
            return ADDR
        if t.v == "(":
            close = match_close(self.t, self.i, "(", ")")
            v = self.ctx.eval(self.t[self.i + 1:close], self.depth + 1)
            self.i = close + 1
            return v
        if t.k == "num":
            self.i += 1
            return None if t.v is None else (t.v, t.v)
        if t.k == "id":
            self.i += 1
            if self.i < len(self.t) and self.t[self.i].v == "(":
                self.i = match_close(self.t, self.i, "(", ")") + 1
                return None
            return self.ctx.lookup(t.v, self.depth + 1)
        if t.k == "str":
            self.i += 1
            return ADDR
        raise ValueError(t.v)


def eval_width(ctx, toks):
    """Like ctx.eval, plus the `__IOC_OUT_SIZE (req)` marker an ioctl write carries."""
    if toks and toks[0].v == "__IOC_OUT_SIZE":
        req = ctx.eval(toks[1:])
        if req is None or req[0] != req[1]:
            return None
        r = req[0] & 0xFFFFFFFF
        if (r >> 30) & 2 == 0:
            return (0, 0)
        s = (r >> 16) & 0x1FFF
        return (s, s)
    return ctx.eval(toks)


# ---------------------------------------------------------------------------
# World
# ---------------------------------------------------------------------------
class World:
    def __init__(self, root):
        self.root = root
        self.modules = []
        self.src_fns = {}
        self.lib_fns = {}
        self.global_assigned = set()
        self.search = []

    def load(self):
        lib_files = sorted(glob.glob(os.path.join(self.root, "lib", "*.cyr")))
        if not lib_files:
            print(f"check-stack-array-sizing: {os.path.join(self.root, 'lib')} has no *.cyr "
                  f"— run `cyrius deps` first")
            sys.exit(2)
        for p in lib_files:
            if os.path.basename(p) not in LIB_SKIP:
                self.modules.append(parse_module(p, os.path.relpath(p, self.root), True))
        for r in SCAN_ROOTS:
            for dirpath, dirnames, filenames in os.walk(os.path.join(self.root, r)):
                dirnames[:] = sorted(d for d in dirnames if d not in ("lib", "build", "deps"))
                for f in sorted(filenames):
                    if f.endswith(SCAN_EXTS):
                        p = os.path.join(dirpath, f)
                        self.modules.append(parse_module(p, os.path.relpath(p, self.root), False))
        for m in self.modules:
            self.global_assigned |= m.assigned
            table = self.lib_fns if m.is_lib else (self.src_fns if m.rel.startswith("src" + os.sep) else None)
            if table is not None:
                for name, fn in m.fns.items():
                    table.setdefault(name, (m, fn))
        self.search = [m for m in self.modules if m.rel.startswith("src" + os.sep)] + \
                      [m for m in self.modules if m.is_lib]

    def const_value(self, module, name, depth):
        if depth > MAX_DEPTH or name in self.global_assigned:
            return None
        for m in [module] + self.search:
            if name in m.gconst_toks:
                return Ctx(self, m, None, None).eval(m.gconst_toks[name], depth + 1)
            if name in m.enum_toks:
                toks, prev, off = m.enum_toks[name]
                if toks is not None:
                    return Ctx(self, m, None, None).eval(toks, depth + 1)
                if prev is None:
                    return (off, off)
                base = self.const_value(m, prev, depth + 1)
                return None if base is None else (base[0] + off, base[1] + off)
        return None

    def resolve_fn(self, module, name):
        if name in module.fns:
            return module, module.fns[name]
        return self.src_fns.get(name) or self.lib_fns.get(name)


def substitute(toks, mapping):
    out = []
    for t in toks:
        if t.k == "id" and t.v in mapping:
            out.extend(paren(mapping[t.v]))
        else:
            out.append(t)
    return out


def decompose_ptr(toks):
    """(kind, name, offset toks): kind "addr" for `&NAME ± e`, "name" for `NAME ± e`."""
    toks = strip_parens(toks)
    if not toks:
        return None
    terms, cur, sign, depth = [], [], "+", 0
    for t in toks:
        if t.v in ("(", "["):
            depth += 1
        elif t.v in (")", "]"):
            depth -= 1
        if depth == 0 and t.k == "p" and t.v in ("+", "-") and cur:
            terms.append((sign, cur))
            cur, sign = [], t.v
        else:
            cur.append(t)
    terms.append((sign, cur))
    head = strip_parens(terms[0][1])
    if len(head) == 2 and head[0].v == "&" and head[1].k == "id":
        kind, name = "addr", head[1].v
    elif len(head) == 1 and head[0].k == "id":
        kind, name = "name", head[0].v
    else:
        return None
    off = [num(0)]
    for (sg, term) in terms[1:]:
        if any(t.v == "&" for t in term):
            return None
        off = off + [Tok("p", sg, 0)] + paren(term)
    return kind, name, off


def resolve_base(fn, ptr, extra):
    """Follow single-assignment local aliases: (kind, name, offset toks) or None."""
    d = decompose_ptr(ptr)
    if d is None:
        return None
    kind, name, off = d
    off = off + [Tok("p", "+", 0)] + paren(extra)
    hops = 0
    while kind == "name" and name not in fn.params and fn.assign_count.get(name) == 1 \
            and name in fn.inits and hops < MAX_DEPTH:
        d2 = decompose_ptr(fn.inits[name])
        if d2 is None:
            return None
        kind, name, off2 = d2
        off = off2 + [Tok("p", "+", 0)] + paren(off)
        hops += 1
    return kind, name, off


def fn_effects(world, module, fn):
    """Every write in `fn`: (site, ptr toks, extra offset toks, width toks, how, line, conds).

    `conds` are the callee-side reachability conditions, already in this function's
    namespace; this function's own path conditions are added by the consumer.
    """
    zero = [num(0)]
    eff = []
    for (k, callee, args, line) in fn.calls:
        if callee in ("memset", "memcpy", "memmove") and len(args) == 3:
            eff.append((k, args[0], zero, args[2], callee, line, []))
        elif callee in STORE_WIDTH and len(args) == 2:
            eff.append((k, args[0], zero, [num(STORE_WIDTH[callee])], callee, line, []))
        elif callee == "syscall" and args:
            nr = Ctx(world, module, fn, k).eval(args[0])
            nr = nr[0] if nr and nr[0] == nr[1] else None
            if nr in (0, 17) and len(args) >= 4:
                eff.append((k, args[2], zero, args[3], "syscall read", line, []))
            elif nr == 318 and len(args) >= 3:
                eff.append((k, args[1], zero, args[2], "syscall getrandom", line, []))
            elif nr == 228 and len(args) >= 3:
                eff.append((k, args[2], zero, [num(16)], "syscall clock_gettime", line, []))
            elif nr == 96 and len(args) >= 2:
                eff.append((k, args[1], zero, [num(16)], "syscall gettimeofday", line, []))
            elif nr == 35 and len(args) >= 3:
                eff.append((k, args[2], zero, [num(16)], "syscall nanosleep rem", line, []))
            elif nr == 16 and len(args) >= 4:
                eff.append((k, args[3], zero, [Tok("id", "__IOC_OUT_SIZE", 0)] + paren(args[2]),
                            "syscall ioctl", line, []))
        else:
            r = world.resolve_fn(module, callee)
            if r is None:
                continue
            cfn = r[1]
            mapping = {p: (args[i] if i < len(args) else zero) for i, p in enumerate(cfn.params)}
            for pidx, entries in cfn.summary.items():
                if pidx >= len(args):
                    continue
                for (off_t, width_t, how, conds) in entries:
                    eff.append((k, args[pidx], substitute(off_t, mapping), substitute(width_t, mapping),
                                f"{callee}() arg{pidx} <- {how}", line,
                                [(substitute(c, mapping), want) for (c, want) in conds]))
    return eff


def expand_locals(fn, toks, depth=0):
    if depth > MAX_DEPTH:
        return None
    out = []
    for t in toks:
        if t.k == "id" and t.v not in fn.params and fn.assign_count.get(t.v) == 1 and t.v in fn.inits:
            sub = expand_locals(fn, fn.inits[t.v], depth + 1)
            if sub is None:
                return None
            out.extend(paren(sub))
        else:
            out.append(t)
    return out


def to_param_expr(world, m, fn, site, toks, keep_interval_hi=True):
    """Re-express `toks` over fn's (never reassigned) parameters; None if impossible.

    With no parameter left the value is folded (to its interval's upper bound when
    `keep_interval_hi`, else only when it is a single point).
    """
    if toks and toks[0].v == "__IOC_OUT_SIZE":
        inner = to_param_expr(world, m, fn, site, toks[1:], False)
        return None if inner is None else [toks[0]] + paren(inner)
    expanded = expand_locals(fn, toks)
    if expanded is None:
        return None
    ids = {t.v for t in expanded if t.k == "id"}
    params = {p for p in ids if p in fn.params and p not in fn.assign_count}
    if not params:
        v = Ctx(world, m, fn, site).eval(toks)
        if v is None or (not keep_interval_hi and v[0] != v[1]):
            return None
        return [num(v[1])]
    top = Ctx(world, m, None, None)
    for name in ids - params:
        if top.lookup(name, 0) is None:
            return None
    return expanded


def reachable(ctx, conds):
    """False when some condition is decided against its required truth."""
    for (c, want) in conds:
        tv = truth(ctx.eval(c))
        if tv is not None and tv != want:
            return False
    return True


def build_summaries(world):
    all_fns = [(m, fn) for m in world.modules for fn in m.fns.values()]
    for _ in range(MAX_DEPTH):
        changed = False
        for (m, fn) in all_fns:
            for (k, ptr, extra, width, how, line, conds) in fn_effects(world, m, fn):
                base = resolve_base(fn, ptr, extra)
                if base is None:
                    continue
                kind, name, off = base
                if kind != "name" or name not in fn.params or name in fn.assign_count:
                    continue
                all_conds = conds + fn.path_conds(k)
                if not reachable(Ctx(world, m, fn, k), all_conds):
                    continue
                off_t = to_param_expr(world, m, fn, k, off)
                width_t = to_param_expr(world, m, fn, k, width)
                if off_t is None or width_t is None:
                    continue
                kept = []
                for (c, want) in all_conds:
                    ct = to_param_expr(world, m, fn, k, c, False)
                    if ct is not None and any(t.k == "id" for t in ct):
                        kept.append((ct, want))
                entries = fn.summary.setdefault(fn.params.index(name), [])
                sig = (key_of(off_t), key_of(width_t), tuple((key_of(c), w) for (c, w) in kept))
                if any((key_of(a), key_of(b), tuple((key_of(c), w) for (c, w) in cs)) == sig
                       for (a, b, _h, cs) in entries):
                    continue
                if len(entries) >= MAX_ENTRIES_PER_PARAM:
                    continue
                entries.append((off_t, width_t, how, kept))
                changed = True
        if not changed:
            break


# ---------------------------------------------------------------------------
# The check
# ---------------------------------------------------------------------------
def object_for(world, m, fn, name, site):
    """(size bytes, label, decl line) of the object `&name` names at `site`, or None."""
    best = None
    for (k, nm, size_t, width, kind) in fn.decls:
        if nm == name and (best is None or k < site):
            best = (k, size_t, width, kind)
    if best is not None:
        k, size_t, width, kind = best
        line = fn.body[k].line
        if kind == "scalar":
            return 8, f"var {name} (scalar)", line
        v = Ctx(world, m, fn, k).eval(size_t)
        if v is None or v[0] != v[1]:
            return None
        label = f"var {name}[{v[0]}]" if width == 1 else f"var {name}: [{v[0]}] x {width} B"
        return v[0] * width, label, line
    if name in fn.params or name not in m.garrays:
        return None
    size_t, width, line = m.garrays[name]
    v = Ctx(world, m, None, None).eval(size_t)
    if v is None or v[0] != v[1]:
        return None
    return v[0] * width, f"global var {name}[{v[0]}]", line


def check(world, verbose):
    findings, checked = [], 0
    for m in world.modules:
        if m.is_lib:
            continue
        for fn in m.fns.values():
            for (k, ptr, extra, width, how, line, conds) in fn_effects(world, m, fn):
                base = resolve_base(fn, ptr, extra)
                if base is None or base[0] != "addr":
                    continue
                _kind, name, off = base
                obj = object_for(world, m, fn, name, k)
                if obj is None:
                    continue
                size, label, decl_line = obj
                ctx = Ctx(world, m, fn, k)
                if not reachable(ctx, conds + fn.path_conds(k)):
                    continue
                o = ctx.eval(off)
                w = eval_width(ctx, width)
                if o is None or w is None or w[1] <= 0:
                    continue
                checked += 1
                lo, hi = o[0], o[1] + w[1]
                if lo < 0 or hi > size:
                    callee = how.split("()")[0] if "()" in how else how.split()[0]
                    what = f"UNDERFLOW {-lo} B" if lo < 0 else f"OVERRUN {hi - size} B"
                    findings.append({
                        "key": (m.rel, fn.name, name, callee),
                        "msg": f"{m.rel}:{line}: {fn.name}: {label} ({size} B, line {decl_line}) "
                               f"<- {how}: writes bytes [{lo}, {hi}) — {what}",
                    })
                elif verbose:
                    print(f"  ok {m.rel}:{line}: {fn.name}: {label} ({size} B) <- {how} [{lo}, {hi})")
    return findings, checked


def run(root, verbose):
    world = World(root)
    world.load()
    build_summaries(world)
    findings, checked = check(world, verbose)
    hits = {key: 0 for key in ALLOWLIST}
    problems = []
    for f in findings:
        if f["key"] in ALLOWLIST:
            hits[f["key"]] += 1
            if verbose:
                print(f"  allowlisted ({ALLOWLIST[f['key']]}): {f['msg']}")
        else:
            problems.append(f["msg"])
    stale = [key for key, h in hits.items() if h == 0]
    for p in sorted(set(problems)):
        print(p)
    for key in stale:
        print(f"stale ALLOWLIST entry, matches no finding — remove it: {key}")
    print(f"\n{checked} resolvable write(s) into declared arrays / scalars checked: "
          f"{len(set(problems))} out of bounds, {sum(hits.values())} allowlisted, {len(stale)} stale "
          f"allowlist entr{'y' if len(stale) == 1 else 'ies'}")
    return 1 if (problems or stale) else 0


# ---------------------------------------------------------------------------
# Self-test: every detection and every exemption path, on fixtures
# ---------------------------------------------------------------------------
SELF_TEST_LIB = """
enum SysNr { SYS_READ = 0; SYS_IOCTL = 16; SYS_CLOCK_GETTIME = 228; }
"""
SELF_TEST_SRC = """
var BACKEND_SIZE = 328;
var MUTABLE_SIZE = 8;
var IOC_BIG = 0xC0206440;
var IOC_WRITE_ONLY = 0x40206440;
fn fill_backend(bebuf) { memset(bebuf, 0, BACKEND_SIZE); return 0; }
fn wire(ctx, be) { memset(ctx, 0, 176); fill_backend(be); return 0; }
fn put_pair(p, a, b) { store64(p, a); store64(p + 8, b); return 0; }
fn zero_n(p, n) { memset(p, 0, n); return 0; }
fn bump() { MUTABLE_SIZE = 64; return 0; }
fn guarded_read(ctx, dst, n) {
    if (ctx == 0) { return 1; }
    if (n <= 0) { return 2; }
    memcpy(dst, ctx, n);
    return 0;
}
fn detile(host, n, dir) {
    if (dir != 0) { memcpy(host, 0, n); }
    return 0;
}
fn fwd(ctx, dst, n) { return guarded_read(ctx, dst, n); }
fn ioctl_wrap(fd, req, arg) { return syscall(SYS_IOCTL, fd, req, arg); }
"""
SELF_TEST_CASES = [
    # (name, body of `fn t()`, expected finding count)
    ("memset-direct-over", "var be[248]; memset(&be, 0, BACKEND_SIZE);", 1),
    ("memset-direct-ok", "var be[328]; memset(&be, 0, BACKEND_SIZE);", 0),
    ("helper-2-levels-over", "var ctx[176]; var be[256]; wire(&ctx, &be);", 1),
    ("helper-2-levels-ok", "var ctx[176]; var be[328]; wire(&ctx, &be);", 0),
    ("store64-literal-over", "var px[20]; store64(&px + 0, 1); store64(&px + 24, 1);", 1),
    ("store64-bytes-not-slots", "var probes[3]; store64(&probes + 0, 1);", 1),
    ("store64-ok", "var px[32]; store64(&px + 24, 1);", 0),
    ("paren-addr-form", "var be[248]; memset((&be), 0, BACKEND_SIZE);", 1),
    ("typed-array-ok", "var a: i64[4]; store64(&a + 24, 1);", 0),
    ("typed-array-over", "var a: i32[4]; store64(&a + 12, 1);", 1),
    ("helper-offset-over", "var q[16]; put_pair(&q + 8, 1, 2);", 1),
    ("helper-offset-ok", "var q[24]; put_pair(&q + 8, 1, 2);", 0),
    ("helper-symbolic-size-over", "var q[16]; zero_n(&q, 64);", 1),
    ("call-in-var-initialiser", "var q[16]; var rc = zero_n(&q, 64);", 1),
    ("alias-local-over", "var buf[16]; var p = &buf + 8; store64(p + 4, 1);", 1),
    ("loop-counter-over", "var a[24]; var i = 0; while (i < 4) { store64(&a + i * 8, 0); i = i + 1; }", 1),
    ("loop-counter-ok", "var a[32]; var i = 0; while (i < 4) { store64(&a + i * 8, 0); i = i + 1; }", 0),
    ("loop-step-4-ok", "var a[64]; var i = 0; while (i < 64) { store8(&a + i + 3, 0); i = i + 4; }", 0),
    ("loop-step-4-over", "var a[64]; var i = 0; while (i < 64) { store8(&a + i + 4, 0); i = i + 4; }", 1),
    ("for-loop-over", "var a[8]; for (var i = 0; i < 2; i = i + 1) { store64(&a + i * 8, 0); }", 1),
    ("unresolvable-skipped", "var a[8]; var n = 0; n = input(); store64(&a + n, 0);", 0),
    ("mutable-global-not-a-constant", "var a[8]; memset(&a, 0, MUTABLE_SIZE);", 0),
    ("scalar-over", "var ts = 0; syscall(SYS_CLOCK_GETTIME, 1, &ts);", 1),
    ("syscall-read-over", "var b[8]; syscall(SYS_READ, 0, &b, 64);", 1),
    ("ioctl-read-dir-over", "var req[16]; syscall(SYS_IOCTL, 3, IOC_BIG, &req);", 1),
    ("ioctl-read-dir-ok", "var req[32]; syscall(SYS_IOCTL, 3, IOC_BIG, &req);", 0),
    ("ioctl-write-only-dir-quiet", "var req[16]; syscall(SYS_IOCTL, 3, IOC_WRITE_ONLY, &req);", 0),
    ("ioctl-through-wrapper-over", "var req[16]; ioctl_wrap(3, IOC_BIG, &req);", 1),
    ("underflow", "var b[16]; store64(&b - 8, 0);", 1),
    ("comment-and-string-ignored", "var b[8]; # store64(&b + 64, 0);\n print(\"store64(&b + 64, 0)\");", 0),
    ("enum-constant-size", "var b[8]; memset(&b, 0, SYS_IOCTL);", 1),
    ("guard-true-unreachable", "var d[8]; guarded_read(0, &d, 64);", 0),
    ("guard-true-n-zero", "var c[8]; var d[8]; guarded_read(&c, &d, 0);", 0),
    ("guard-undecided-reachable", "var c[8]; var d[8]; guarded_read(&c, &d, 64);", 1),
    ("guard-through-forwarder", "var d[8]; fwd(0, &d, 64);", 0),
    ("guard-through-forwarder-reachable", "var c[8]; var d[8]; fwd(&c, &d, 64);", 1),
    ("enclosing-if-false-unreachable", "var h[8]; detile(&h, 64, 0);", 0),
    ("enclosing-if-true-reachable", "var h[8]; detile(&h, 64, 1);", 1),
    ("own-else-branch", "var h[8]; if (1 == 1) { store8(&h, 0); } else { store64(&h + 8, 0); }", 0),
]


def self_test():
    failures = 0
    with tempfile.TemporaryDirectory(prefix="mabda-stackgate-") as tmp:
        for d in ("lib", "src", os.path.join("tests", "tcyr")):
            os.makedirs(os.path.join(tmp, d))
        with open(os.path.join(tmp, "lib", "syscalls_x86_64_linux.cyr"), "w") as f:
            f.write(SELF_TEST_LIB)
        with open(os.path.join(tmp, "src", "helpers.cyr"), "w") as f:
            f.write(SELF_TEST_SRC)
        for (name, body, want) in SELF_TEST_CASES:
            with open(os.path.join(tmp, "tests", "tcyr", "case.tcyr"), "w") as f:
                f.write(f"fn t() {{\n    {body}\n    return 0;\n}}\n")
            world = World(tmp)
            world.load()
            build_summaries(world)
            findings, _ = check(world, False)
            ok = len(findings) == want
            failures += 0 if ok else 1
            print(f"  {'ok  ' if ok else 'FAIL'} {name}: {len(findings)} finding(s), want {want}")
            if not ok:
                for fd in findings:
                    print(f"         {fd['msg']}")
    print(f"self-test: {len(SELF_TEST_CASES) - failures}/{len(SELF_TEST_CASES)} cases pass")
    return 1 if failures else 0


def main():
    argv = sys.argv[1:]
    if "--self-test" in argv:
        return self_test()
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
    if "--root" in argv:
        root = argv[argv.index("--root") + 1]
    return run(os.path.abspath(root), "--verbose" in argv)


if __name__ == "__main__":
    sys.exit(main())
