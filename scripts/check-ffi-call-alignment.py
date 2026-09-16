#!/usr/bin/env python3
"""check-ffi-call-alignment.py — static gate: no call that can reach extern C is made
with the stack 8 bytes off the SysV 16-byte alignment.

WHY THIS EXISTS
---------------
cycc (cyrius 6.6.4, x86_64) evaluates expressions on the machine stack. Every call
argument is evaluated and then `push rax`-ed, and a binary or comparison operator pushes
its left operand while it evaluates the right one. A call evaluated while an ODD number of
those values is still pushed runs with rsp 8 bytes off 16-byte alignment:

    store64(pp, wgpu_device_create_buffer(dev, desc));   # arg 2: one pending push
    var n = x + wgpu_buffer_get_size(buf);               # binop rhs: one pending push

Cyrius code does not care. glibc-compiled C does: aligned SSE on a stack slot
(`movaps %xmm0, (%rsp)`) raises #GP, which arrives as SIGSEGV. NVK's create_buffer
crashed exactly this way in the 4.1.3 verification, and the misalignment is inherited
through any number of Cyrius frames, so a Cyrius helper that reaches C is as dangerous
in that position as the C call itself. Details, repro and the codegen root cause:
docs/development/issues/2026-09-16-cycc-nested-call-stack-alignment.md

⚠ A source-text rule for "which call runs misaligned" is not reliable: `fncallN(...)` is
lowered to an indirect call with its callee spilled to a frame slot in expression
position, but compiled as an ordinary call into lib/fnptr.cyr (callee pushed as argument
1) as a bare statement. So this gate reads the compiler's own output instead.

HOW
---
1. Compile `include "src/lib.cyr"` (every src/ and lib/ function) and each
   programs/*.cyr in cycc `object;` mode — the mode the C-launcher builds use.
2. Disassemble each object and walk every function's control-flow graph, tracking how
   many bytes rsp sits below its entry value. A `call` is aligned when that depth is
   8 mod 16; a tail `jmp` when it is 0 mod 16 (the callee inherits the entry alignment).
3. A function REACHES C when it makes an indirect call, calls an undefined (extern)
   symbol, or calls/tail-jumps to a function that reaches C. Every indirect call counts,
   except in the functions listed in NOT_C_INDIRECT below, each with its reason.
4. FAIL on any misaligned call or tail jump to C, or to a function that reaches C, in a
   function defined in src/, lib/ or the scanned program.

The analysis refuses to pass blind. It FAILS on a depth conflict at a join, an rsp write
it does not model, an indirect jmp, a `call` the CFG walk never reached, a stale
NOT_C_INDIRECT entry, or a src/ object in which `wgpu_device_create_buffer` does not
reach C.

SELF-TEST (runs first, every time)
----------------------------------
A fixture with known aligned and misaligned shapes is compiled with the same cycc. The
analyzer must flag exactly the misaligned rows. When gcc is available the fixture is
also linked against a C leaf that returns `(rsp + 8) & 15` at entry and run, and the
analyzer's verdict must equal the measured value for every row. So a cycc change (a
fix upstream, or a new codegen shape) cannot silently blind the gate: it either still
agrees with the hardware, or the gate fails and says which row disagrees.

USAGE
    scripts/check-ffi-call-alignment.py [--verbose] [--jobs N] [--keep DIR] [FILE ...]
FILE defaults to programs/*.cyr. Exit 0 = clean, 1 = violation or analysis failure,
2 = usage/tool error. Needs cycc and objdump; gcc for the runtime half of the self-test
(GCC=none skips that half and compares the fixture against the recorded cycc 6.6.4 verdicts).
"""
import argparse
import concurrent.futures
import glob
import os
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Functions whose indirect calls can only reach Cyrius code in this tree. Everything
# else that calls through a pointer is assumed to be able to reach C.
NOT_C_INDIRECT = {
    # lib/alloc.cyr allocator vtable. Every vtable in the link is built in lib/alloc.cyr
    # from Cyrius functions (_bump_*, arena_*, _test_*); nothing in src/ or programs/
    # calls allocator_new or installs its own.
    "alloc_via": "allocator vtable (lib/alloc.cyr, Cyrius allocators only)",
    "realloc_via": "allocator vtable (lib/alloc.cyr, Cyrius allocators only)",
    "free_via": "allocator vtable (lib/alloc.cyr, Cyrius allocators only)",
    "reset_via": "allocator vtable (lib/alloc.cyr, Cyrius allocators only)",
}

# Inline-asm functions that switch stacks (the clone child path pops its entry point off
# the new stack), so a depth walk over them is meaningless. Their indirect call is the
# thread entry, a Cyrius function.
ASM_STACK_SWITCH = {
    "_thread_spawn": "lib/thread.cyr clone child path runs on the new stack",
    "_thread_spawn_detached": "lib/thread.cyr clone child path runs on the new stack",
}

# The one wrapper the src/ object must show reaching C, or the call graph is broken.
CANARY_REACHES_C = "wgpu_device_create_buffer"

FN_DEF = re.compile(r"^\s*fn\s+(\w+)\s*\(")
SYM_LINE = re.compile(r"^([0-9a-f]+) <([^>]+)>:$")
INS_LINE = re.compile(r"^\s*([0-9a-f]+):\t(\S+)\s*(.*)$")
REL_LINE = re.compile(r"^\s*([0-9a-f]+): (R_X86_64_\S+)\s+(\S+)$")
TARGET = re.compile(r"^([0-9a-f]+) <")
RSP_IMM = re.compile(r"^\$0x([0-9a-f]+),%rsp$")


def die(msg):
    print("check-ffi-call-alignment: " + msg, file=sys.stderr)
    sys.exit(2)


def compile_object(cycc, src_path, obj_path, cwd):
    with open(src_path, "rb") as f:
        body = f.read()
    with open(obj_path, "wb") as out:
        r = subprocess.run([cycc], input=b"object;\n" + body, stdout=out,
                           stderr=subprocess.PIPE, cwd=cwd, timeout=600)
    err = r.stderr.decode("utf-8", "replace")
    if r.returncode != 0 or os.path.getsize(obj_path) == 0:
        return "cycc failed (rc=%d) on %s:\n%s" % (r.returncode, src_path, err[-2000:])
    return None


def disassemble(objdump, obj_path):
    """Parse objdump -dr over the whole .text. Returns (ins, starts):
    ins    = address-sorted [addr, op, args, reloc symbol or None]
    starts = {function symbol: start address}, symbol-table .text functions only.
    objdump also prints synthetic labels such as `<fncall0-0x5>` for the entry stub at
    .text+0; those are not functions. Top-level statements are NOT inside any function:
    cycc chains them between function bodies with jmps from `_cyrius_init`, so a walk
    must follow jumps across symbol boundaries rather than stop at them."""
    sym = subprocess.run([objdump, "-t", obj_path], capture_output=True, text=True,
                         check=True).stdout
    starts = {}
    for line in sym.splitlines():
        parts = line.split()
        if len(parts) >= 6 and parts[3] == ".text" and parts[2] == "F":
            starts[parts[-1]] = int(parts[0], 16)
    out = subprocess.run([objdump, "-dr", "--no-show-raw-insn", "-j", ".text", obj_path],
                         capture_output=True, text=True, check=True).stdout
    ins = []
    for line in out.splitlines():
        m = INS_LINE.match(line)
        if m:
            ins.append([int(m.group(1), 16), m.group(2), m.group(3).split("#")[0].strip(), None])
            continue
        m = REL_LINE.match(line)
        if m and ins:
            ins[-1][3] = re.split(r"[-+]", m.group(3))[0]
    ins.sort(key=lambda x: x[0])
    return ins, starts


def owner_table(ins, starts):
    """For each instruction index, the symbol whose range contains it."""
    order = sorted(starts.items(), key=lambda kv: kv[1])
    owner = [None] * len(ins)
    j = -1
    for i, x in enumerate(ins):
        while j + 1 < len(order) and order[j + 1][1] <= x[0]:
            j += 1
        owner[i] = order[j][0] if j >= 0 else None
    return owner


def walk(name, start, ins, index, by_start, reached):
    """Depth-walk from one function entry, following jumps anywhere in .text.
    Returns (edges, problems). edges: [(offset, kind, target, depth_bytes)] for every call
    and tail jump; offset is relative to the walked function's entry."""
    depth = {}
    edges = []
    problems = []

    def at(i):
        return "%s+%#x" % (name, ins[i][0] - start)

    if start not in index:
        return edges, ["%s: entry address not disassembled" % name]
    work = [(index[start], 0)]
    while work:
        i, k = work.pop()
        while 0 <= i < len(ins):
            if i in depth:
                if depth[i] != k:
                    problems.append("%s: stack depth conflict at a join (%d vs %d)" % (at(i), depth[i], k))
                break
            if ins[i][0] != start and ins[i][0] in by_start:
                problems.append("%s: fell through into another function" % at(i))
                break
            depth[i] = k
            reached.add(i)
            addr, op, a, rel = ins[i]
            off = addr - start
            if op.startswith("push"):
                k += 8
            elif op.startswith("pop"):
                k -= 8
            elif op == "leave":
                k = 0
            elif op == "mov" and a == "%rbp,%rsp":
                k = 8
            elif op in ("sub", "add") and a.endswith(",%rsp"):
                m = RSP_IMM.match(a)
                if not m:
                    problems.append("%s: unmodelled rsp update `%s %s`" % (at(i), op, a))
                else:
                    k += int(m.group(1), 16) * (1 if op == "sub" else -1)
            elif a.endswith("%rsp") and op not in ("cmp", "test"):
                problems.append("%s: unmodelled rsp write `%s %s`" % (at(i), op, a))
            elif op == "ret":
                break
            elif op == "call":
                if a.startswith("*"):
                    edges.append((off, "indirect", a, k))
                elif rel and not rel.startswith("."):
                    edges.append((off, "call", rel, k))
                else:
                    m = TARGET.match(a)
                    tgt = int(m.group(1), 16) if m else None
                    edges.append((off, "call", by_start.get(tgt, a), k))
            elif op.startswith("j"):
                if a.startswith("*"):
                    problems.append("%s: indirect jmp `%s` (targets unknown)" % (at(i), a))
                    break
                m = TARGET.match(a)
                tgt = int(m.group(1), 16) if m else None
                if rel and not rel.startswith("."):
                    # rel32 relocated against a function symbol (including this one): a
                    # tail call. The printed target is the unrelocated placeholder.
                    edges.append((off, "tail", rel, k))
                    if op == "jmp":
                        break
                elif tgt in by_start and tgt != start:
                    edges.append((off, "tail", by_start[tgt], k))
                    if op == "jmp":
                        break
                elif tgt in index:
                    if op == "jmp":
                        i = index[tgt]
                        continue
                    work.append((index[tgt], k))
                else:
                    problems.append("%s: jump to an address outside .text `%s %s`" % (at(i), op, a))
                    if op == "jmp":
                        break
            i += 1
    return edges, problems


def analyze(objdump, obj_path, reach_in, skip=None):
    """Walk every function in the object except those in `skip` (already analyzed in the
    src/ coverage object), plus `_cyrius_init`, whose walk covers top-level statements.
    reach_in: names already known to reach C. Returns (graph, problems, unreached, reach)."""
    ins, starts = disassemble(objdump, obj_path)
    index = {x[0]: i for i, x in enumerate(ins)}
    by_start = {addr: n for n, addr in starts.items()}
    wanted = None if skip is None else (set(starts) - set(skip))
    names = set(starts) if wanted is None else wanted | {"_cyrius_init"}
    names &= set(starts)
    graph = {}
    problems = []
    reached = set()
    for name in sorted(names):
        edges, probs = walk(name, starts[name], ins, index, by_start, reached)
        if name in ASM_STACK_SWITCH:
            probs = []
        graph[name] = [(off, kind, tgt, k, kind != "indirect" and tgt not in starts)
                       for off, kind, tgt, k in edges]
        problems.extend(probs)
    owner = owner_table(ins, starts)
    unreached = ["%s+%#x" % (owner[i], ins[i][0] - starts[owner[i]])
                 for i in range(len(ins))
                 if ins[i][1] == "call" and i not in reached and owner[i] is not None
                 and (wanted is None or owner[i] in names)
                 and owner[i] not in ASM_STACK_SWITCH]
    reach = set(reach_in)
    changed = True
    while changed:
        changed = False
        for name, edges in graph.items():
            if name in reach:
                continue
            for off, kind, tgt, k, extern in edges:
                indirect_c = kind == "indirect" and name not in NOT_C_INDIRECT \
                    and name not in ASM_STACK_SWITCH
                if indirect_c or extern or tgt in reach:
                    reach.add(name)
                    changed = True
                    break
    return graph, problems, unreached, reach


def violations(graph, reach):
    out = []
    for name, edges in graph.items():
        if name in ASM_STACK_SWITCH:
            continue
        for off, kind, tgt, k, extern in edges:
            aligned = (k % 16 == 0) if kind == "tail" else (k % 16 == 8)
            if aligned:
                continue
            if kind == "indirect":
                to_c = name not in NOT_C_INDIRECT
            else:
                to_c = extern or tgt in reach
            if to_c:
                out.append((name, off, kind, tgt))
    return out


def fn_index(paths):
    """{fn name: (path, first line)} over source files."""
    where = {}
    for p in paths:
        try:
            with open(p, encoding="utf-8", errors="replace") as f:
                for n, line in enumerate(f, 1):
                    m = FN_DEF.match(line)
                    if m and m.group(1) not in where:
                        where[m.group(1)] = (p, n)
        except OSError:
            continue
    return where


def candidate_lines(path, def_line, callee, whole_file):
    """Source lines in the function (or, for top-level code, the whole file) that call
    `callee` (`*` = any fncallN/callptr), to point the reader at the site."""
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            lines = f.readlines()
    except OSError:
        return []
    if callee == "*":
        pat = re.compile(r"\b(fncall[0-8]|callptr)\s*\(")
    else:
        pat = re.compile(r"\b%s\s*\(" % re.escape(callee))
    hits = []
    first = 1 if whole_file else def_line
    for n in range(first, len(lines) + 1):
        text = lines[n - 1]
        if not whole_file and n > def_line and FN_DEF.match(text):
            break
        if pat.search(text) and not text.lstrip().startswith("#"):
            hits.append("%s:%d: %s" % (os.path.relpath(path, ROOT), n, text.strip()[:110]))
    return hits


def report(viol, where, label, top_path):
    """Print one FAIL per violation. top_path: the file whose top-level statements
    `_cyrius_init` stands for (None for the src/ coverage object)."""
    for name, off, kind, tgt in sorted(viol):
        if kind == "indirect":
            what = "indirect call"
        elif kind == "tail":
            what = "tail jump to " + tgt
        else:
            what = "call to " + tgt
        top = name == "_cyrius_init"
        path, line = (top_path, 1) if (top and top_path) else where.get(name, (None, 0))
        loc = "%s:%d" % (os.path.relpath(path, ROOT), line) if path else "?"
        who = "top-level code" if top else name
        print("FAIL [%s] %s %s+%#x: %s reaches C with rsp 8 bytes off 16-byte alignment"
              % (label, loc, who, off, what))
        if path:
            for c in candidate_lines(path, line, "*" if kind == "indirect" else tgt, top):
                print("      " + c)


# --------------------------------------------------------------------------- self-test

FIXTURE = r'''
include "lib/fnptr.cyr"
var _fx_fp = 0;
fn fx_pick2(a, b): i64 { return b; }
fn fx_pick1(a, b): i64 { return a; }
fn fx_pick3(a, b, c): i64 { return c; }
fn fx_pick8(a, b, c, d, e, f, g, h): i64 { return h; }
fn fx_helper(): i64 { var r = fx_leaf(); return r; }
# `return f();` compiles to a tail jump, so fx_tail reaches C only through a tail edge.
fn fx_tail(): i64 { return fx_helper(); }
fn row_01(): i64 { var r = fx_leaf(); return r; }
fn row_02(): i64 { var r = fncall0(_fx_fp); return r; }
fn row_03(): i64 { var r = fx_pick1(fx_leaf(), 0); return r; }
fn row_04(): i64 { var r = fx_pick2(0, fx_leaf()); return r; }
fn row_05(): i64 { var r = fx_pick3(0, 0, fx_leaf()); return r; }
fn row_06(): i64 { var r = fx_pick8(0, 0, 0, 0, 0, 0, 0, fx_leaf()); return r; }
fn row_07(): i64 { var s[8]; store64(&s, fncall0(_fx_fp)); return load64(&s); }
fn row_08(): i64 { var x = 5; var r = x + fx_leaf(); return r - 5; }
fn row_09(): i64 { var x = 5; var r = fx_leaf() + x; return r - 5; }
fn row_10(): i64 { var r = fx_pick2(0, fx_pick2(0, fx_leaf())); return r; }
fn row_11(): i64 { var r = fx_pick2(0, fx_helper()); return r; }
fn row_12(): i64 { var t = fx_leaf(); var r = fx_pick2(0, t); return r; }
fn row_13(): i64 { var r = fncall1(&fx_pick1, fx_leaf()); return r; }
fn row_14(): i64 { var r = 0; if (fx_leaf() == 0) { r = 0; } else { r = 8; } return r; }
fn row_15(): i64 { var r = 0; if (0 == fx_leaf()) { r = 0; } else { r = 8; } return r; }
fn row_18(): i64 { var r = fx_pick2(0, fx_tail()); return r; }
fn row_16(): i64 { return fx_top_bad; }
fn row_17(): i64 { return fx_top_ok; }
fn fx_set(fp): i64 { _fx_fp = fp; return 0; }
# Top-level statements run inside _cyrius_init, chained between function bodies.
var fx_top_bad = fx_pick2(0, fx_leaf());
var fx_top_ok = fx_leaf2();
'''

FIXTURE_ROWS = 18
# Top-level rows: which extern the _cyrius_init call targets identifies the row.
TOP_ROW_BY_LEAF = {"fx_leaf": 16, "fx_leaf2": 17}
# cycc 6.6.4 verdicts: rows whose leaf call runs misaligned when the row is entered
# aligned. Used only when gcc is missing; with gcc the measured values are the reference.
EXPECTED_MISALIGNED_664 = {4, 6, 7, 8, 11, 15, 16, 18}

LAUNCHER = r'''
#include <unistd.h>
extern void _cyrius_init(void);
extern long fx_set(long fp);
long fx_leaf(void);
__asm__(".text\n.globl fx_leaf\n.type fx_leaf,@function\nfx_leaf:\n"
        "  lea 8(%rsp), %rax\n  and $15, %rax\n  ret\n"
        ".globl fx_leaf2\n.type fx_leaf2,@function\nfx_leaf2:\n"
        "  lea 8(%rsp), %rax\n  and $15, %rax\n  ret\n");
'''


def self_test(cycc, objdump, gcc, tmp, verbose):
    rows = ["row_%02d" % i for i in range(1, FIXTURE_ROWS + 1)]
    fx = os.path.join(tmp, "fixture.cyr")
    with open(fx, "w") as f:
        f.write(FIXTURE)
    obj = os.path.join(tmp, "fixture.o")
    err = compile_object(cycc, fx, obj, ROOT)
    if err:
        print("SELF-TEST FAIL: " + err)
        return False
    graph, problems, unreached, reach = analyze(objdump, obj, set())
    viol = violations(graph, reach)
    flagged = {int(n[4:]) for n, off, kind, tgt in viol if n.startswith("row_")}
    flagged |= {TOP_ROW_BY_LEAF.get(tgt, -1) for n, off, kind, tgt in viol if n == "_cyrius_init"}
    ok = True
    if problems or unreached:
        print("SELF-TEST FAIL: analysis problems on the fixture: %s" % (problems + unreached))
        ok = False
    for r in rows:
        if r not in graph:
            print("SELF-TEST FAIL: %s missing from the fixture object" % r)
            return False
    if "fx_helper" not in reach:
        print("SELF-TEST FAIL: a Cyrius helper that calls an extern was not seen to reach C")
        ok = False
    if not any(kind == "tail" and tgt == "fx_helper" for off, kind, tgt, k, ext in graph.get("fx_tail", [])):
        print("SELF-TEST FAIL: fx_tail no longer compiles to a tail jump, so row 18 stopped testing "
              "tail-edge reachability; rewrite that row for the new codegen")
        ok = False
    measured = None
    if gcc:
        c_src = os.path.join(tmp, "launch.c")
        with open(c_src, "w") as f:
            f.write(LAUNCHER)
            f.write("\n".join("extern long %s(void);" % r for r in rows))
            f.write("\nint main(void) {\n    _cyrius_init();\n    fx_set((long)fx_leaf);\n    long v;\n")
            f.write("    static char line[32];\n")
            for i, r in enumerate(rows, 1):
                # Called from C, so each row is entered aligned; write "NN V\n".
                f.write("    v = %s(); line[0] = '0' + %d / 10; line[1] = '0' + %d %% 10; "
                        "line[2] = ' '; line[3] = '0' + (int)(v / 10); line[4] = '0' + (int)(v %% 10); "
                        "line[5] = '\\n'; if (write(1, line, 6) != 6) return 3;\n" % (r, i, i))
            f.write("    return 0;\n}\n")
        exe = os.path.join(tmp, "fixture")
        link = subprocess.run([gcc, "-O0", c_src, obj, "-o", exe],
                              capture_output=True, text=True)
        if link.returncode != 0:
            print("SELF-TEST FAIL: could not link the fixture:\n" + link.stderr[-2000:])
            return False
        run = subprocess.run([exe], capture_output=True, text=True, timeout=60)
        if run.returncode != 0:
            print("SELF-TEST FAIL: fixture exited %d (a misaligned row may have faulted)\n%s"
                  % (run.returncode, run.stdout + run.stderr))
            return False
        measured = {}
        for line in run.stdout.splitlines():
            n, v = line.split()
            measured[int(n)] = int(v)
        if sorted(measured) != list(range(1, FIXTURE_ROWS + 1)):
            print("SELF-TEST FAIL: fixture printed rows %s" % sorted(measured))
            return False
        reference = {i for i, v in measured.items() if v != 0}
        label = "measured on this CPU"
    else:
        reference = EXPECTED_MISALIGNED_664
        label = "cycc 6.6.4 expectation (gcc not found, runtime half skipped)"
    if flagged != reference:
        print("SELF-TEST FAIL: analyzer flags rows %s but the reference (%s) is %s"
              % (sorted(flagged), label, sorted(reference)))
        ok = False
    if not reference:
        print("SELF-TEST NOTE: no fixture row is misaligned; cycc may have fixed the bug upstream")
    if 1 in reference or 2 in reference:
        print("SELF-TEST FAIL: statement-level calls measured misaligned; the model is wrong")
        ok = False
    if verbose or not ok:
        print("self-test: flagged %s, reference %s (%s)" % (sorted(flagged), sorted(reference), label))
    return ok


# --------------------------------------------------------------------------- main

def check_program(job):
    """Worker: compile one program and walk every function the src/ coverage object did not
    already cover (its own, plus any extra stdlib module it includes, e.g. lib/bench.cyr),
    plus its top-level code."""
    cycc, objdump, tmp, path, lib_names, reach = job
    base = os.path.splitext(os.path.basename(path))[0]
    obj = os.path.join(tmp, "prog_" + base + ".o")
    e = compile_object(cycc, path, obj, ROOT)
    if e:
        return path, e, None
    g, probs, unr, rch = analyze(objdump, obj, reach, skip=lib_names)
    return path, None, (violations(g, rch), probs, unr, len(g))


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("files", nargs="*", help="Cyrius programs to check (default programs/*.cyr)")
    ap.add_argument("--jobs", type=int, default=os.cpu_count() or 4)
    ap.add_argument("--keep", help="write objects here instead of a temp dir")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()

    cycc = os.environ.get("CYCC") or shutil.which("cycc")
    objdump = os.environ.get("OBJDUMP") or shutil.which("objdump")
    gcc = os.environ.get("GCC") or shutil.which("gcc")
    if gcc == "none":
        gcc = None
    if not cycc:
        die("cycc not found (install the cyrius toolchain or set CYCC)")
    if not objdump:
        die("objdump not found (install binutils or set OBJDUMP)")
    if not glob.glob(os.path.join(ROOT, "lib", "*.cyr")):
        die("lib/ is empty; run `cyrius deps` first")

    files = args.files or sorted(glob.glob(os.path.join(ROOT, "programs", "*.cyr")))
    files = [os.path.abspath(f) for f in files]
    tmp_owner = None
    if args.keep:
        tmp = os.path.abspath(args.keep)
        os.makedirs(tmp, exist_ok=True)
    else:
        tmp_owner = tempfile.TemporaryDirectory(prefix="mabda-ffialign-")
        tmp = tmp_owner.name

    if not self_test(cycc, objdump, gcc, tmp, args.verbose):
        return 1

    lib_src = sorted(glob.glob(os.path.join(ROOT, "src", "*.cyr"))) + \
        sorted(glob.glob(os.path.join(ROOT, "lib", "*.cyr")))
    where = fn_index(lib_src)

    cov = os.path.join(tmp, "src_coverage.cyr")
    with open(cov, "w") as f:
        f.write('include "src/lib.cyr"\n')
    cov_obj = os.path.join(tmp, "src_coverage.o")
    err = compile_object(cycc, cov, cov_obj, ROOT)
    if err:
        print("FAIL [build] " + err)
        return 1
    graph, problems, unreached, reach = analyze(objdump, cov_obj, set())
    rc = 0
    for p in problems:
        print("FAIL [analysis] src/lib.cyr: " + p)
        rc = 1
    for u in unreached:
        print("FAIL [analysis] src/lib.cyr: call never reached by the depth walk: " + u)
        rc = 1
    for name in list(NOT_C_INDIRECT) + list(ASM_STACK_SWITCH):
        if name not in graph:
            print("FAIL [analysis] exemption `%s` names no function in the object (stale list)" % name)
            rc = 1
    if CANARY_REACHES_C not in reach:
        print("FAIL [analysis] %s does not reach C; the call graph is broken" % CANARY_REACHES_C)
        rc = 1
    viol = violations(graph, reach)
    report(viol, where, "src", None)
    nviol = len(viol)
    lib_names = set(graph)

    jobs = [(cycc, objdump, tmp, path, lib_names, reach) for path in files]
    with concurrent.futures.ProcessPoolExecutor(max_workers=max(1, args.jobs)) as ex:
        results = list(ex.map(check_program, jobs))
    for path, e, res in results:
        rel = os.path.relpath(path, ROOT)
        if e:
            print("FAIL [build] " + e)
            rc = 1
            continue
        v, probs, unr, nown = res
        for p in probs:
            print("FAIL [analysis] %s: %s" % (rel, p))
            rc = 1
        for u in unr:
            print("FAIL [analysis] %s: call never reached by the depth walk: %s" % (rel, u))
            rc = 1
        report(v, fn_index([path] + lib_src), rel, path)
        nviol += len(v)
        if args.verbose:
            print("ok   %s (%d functions walked, %d violation(s))" % (rel, nown, len(v)))

    if nviol:
        print("check-ffi-call-alignment: %d misaligned call(s) that can reach C. Hoist each call "
              "into a local first (`var h = f(...); store64(p, h);`). See "
              "docs/development/issues/2026-09-16-cycc-nested-call-stack-alignment.md" % nviol)
        rc = 1
    if rc == 0:
        print("check-ffi-call-alignment: OK (self-test agrees with %s; src/ + %d program(s), "
              "%d function(s) reach C)" % ("the CPU" if gcc else "cycc 6.6.4 expectations",
                                            len(files), len(reach)))
    return rc


if __name__ == "__main__":
    sys.exit(main())
