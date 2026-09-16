#!/usr/bin/env python3
"""check-program-write-lengths.py — static gate: no hand-counted write lengths, no
hand-typed pass tallies.

WHY THIS EXISTS
---------------
Until 4.1.3 every wgpu integration program printed through
`syscall(1, 1, "literal", <hand-counted length>)`, and 21 of those counts were
wrong (verify-logs 2026-09-16):

  * too long  — phase0.cyr wrote each message's NUL terminator (and, past it,
                whatever followed in .rodata) into its output (13 sites);
  * too short — spirv_e2e, wgpu_array_sample_e2e, wgpu_array_layer_select_e2e,
                wgpu_cube_sample_e2e, compressed_texture_e2e and
                wgpu_texture_sample_e2e dropped the trailing newline, gluing
                two report lines together (8 sites).

Their closing summaries were literals too — `"\\n12 passed, 0 failed\\n"` — which
stopped matching the PASS lines the moment a check was added or removed, so a
program could print "12 passed" after running 13 checks. 4.1.3 moved every
program to strlen-based helpers that count the PASS lines they print; this gate
keeps it that way.

RULES
-----
  1. programs/ (recursive): no write whose buffer is a string literal and whose
     length is an integer literal — `syscall(1 | SYS_WRITE, fd, "…", N)`,
     `sys_write(fd, "…", N)`, or a call to any function that forwards its
     (buffer, length) parameters straight into one of those. Right or wrong
     today, a hand count is the defect: take the length from strlen.
  2. programs/ (recursive): no string literal carrying a pass tally such as
     "12 passed" or "3/3 passed". Count what was printed.
  3. src/ tests/ fuzz/ examples/: the same literal+length writes are allowed
     (library diagnostics, the consumer example), but N must equal the literal's
     byte length after escape decoding (\\n \\t \\r \\0 \\\\ \\" \\' \\a \\b \\f \\v \\xHH
     \\uHHHH \\u{…}, the cyrius lexer's set).

USAGE
    scripts/check-program-write-lengths.py [--root DIR] [--verbose]
    scripts/check-program-write-lengths.py --self-test
Exit 0 = clean, 1 = a violation (or a failed self-test case).
"""
import contextlib
import io
import os
import re
import sys
import tempfile

SYS_WRITE_NAMES = {"1", "SYS_WRITE"}
TALLY = re.compile(r"\b\d+\s*(?:/\s*\d+\s*)?(?:checks?\s+|tests?\s+)?passed\b", re.IGNORECASE)


def lex(text):
    """Tokens (kind, value, line). Strings keep their raw (undecoded) body."""
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
            start, j = line, i + 1
            while j < n and text[j] != '"':
                if text[j] == "\\":
                    j += 1
                if j < n and text[j] == "\n":
                    line += 1
                j += 1
            toks.append(("str", text[i + 1:j], start))
            i = j + 1
        elif c.isalnum() or c == "_":
            j = i
            while j < n and (text[j].isalnum() or text[j] == "_"):
                j += 1
            word = text[i:j]
            toks.append(("num" if word[0].isdigit() else "id", word, line))
            i = j
        else:
            toks.append(("p", c, line))
            i += 1
    return toks


def decode(body):
    """The bytes a cyrius string literal body denotes; None if an escape is malformed."""
    out = bytearray()
    i = 0
    while i < len(body):
        c = body[i]
        if c != "\\":
            out += c.encode("utf-8")
            i += 1
            continue
        if i + 1 >= len(body):
            return None
        e = body[i + 1]
        simple = {"n": 10, "r": 13, "t": 9, "0": 0, "\\": 92, '"': 34, "'": 39,
                  "a": 7, "b": 8, "f": 12, "v": 11}
        if e in simple:
            out.append(simple[e])
            i += 2
        elif e == "x":
            h = body[i + 2:i + 4]
            if len(h) != 2 or not all(ch in "0123456789abcdefABCDEF" for ch in h):
                return None
            out.append(int(h, 16))
            i += 4
        elif e == "u":
            if body[i + 2:i + 3] == "{":
                close = body.find("}", i + 3)
                if close < 0:
                    return None
                digits = body[i + 3:close]
                i = close + 1
            else:
                digits = body[i + 2:i + 6]
                i += 6
            try:
                out += chr(int(digits, 16)).encode("utf-8")
            except ValueError:
                return None
        else:
            return None
    return bytes(out)


def calls(toks):
    """(callee, [arg token lists], line) for every `ident(...)` call."""
    out = []
    for k in range(len(toks) - 1):
        if toks[k][0] != "id" or toks[k + 1] != ("p", "(", toks[k + 1][2]):
            continue
        depth, args, cur = 0, [], []
        j = k + 1
        while j < len(toks):
            kind, v = toks[j][0], toks[j][1]
            if kind == "p" and v in ("(", "["):
                depth += 1
                if depth == 1:
                    j += 1
                    continue
            elif kind == "p" and v in (")", "]"):
                depth -= 1
                if depth == 0:
                    break
            if depth == 1 and kind == "p" and v == ",":
                args.append(cur)
                cur = []
            else:
                cur.append(toks[j])
            j += 1
        if cur or args:
            args.append(cur)
        out.append((toks[k][1], args, toks[k][2]))
    return out


def functions(toks):
    """name -> (params, body tokens) for top-level `fn name(params) ... { body }`."""
    fns = {}
    k = 0
    while k < len(toks) - 2:
        if toks[k][:2] == ("id", "fn") and toks[k + 1][0] == "id" and toks[k + 2][:2] == ("p", "("):
            j = k + 3
            params, cur = [], []
            while j < len(toks) and toks[j][:2] != ("p", ")"):
                if toks[j][:2] == ("p", ","):
                    params.append(cur[0] if cur else None)
                    cur = []
                elif toks[j][0] == "id" and not cur:
                    cur.append(toks[j][1])
                j += 1
            if cur:
                params.append(cur[0])
            while j < len(toks) and toks[j][:2] != ("p", "{"):
                j += 1
            depth, b = 0, j
            while b < len(toks):
                if toks[b][:2] == ("p", "{"):
                    depth += 1
                elif toks[b][:2] == ("p", "}"):
                    depth -= 1
                    if depth == 0:
                        break
                b += 1
            fns.setdefault(toks[k + 1][1], (params, toks[j + 1:b]))
            k = b
        k += 1
    return fns


def single(arg, kind=None):
    if len(arg) == 1 and (kind is None or arg[0][0] == kind):
        return arg[0]
    return None


def write_shape(callee, args, wrappers):
    """(buf arg, len arg) when this call writes buf[0:len], else None."""
    if callee == "syscall" and len(args) == 4:
        nr = single(args[0])
        if nr is not None and nr[1] in SYS_WRITE_NAMES:
            return args[2], args[3]
    if callee == "sys_write" and len(args) == 3:
        return args[1], args[2]
    if callee in wrappers:
        bi, li = wrappers[callee]
        if bi < len(args) and li < len(args):
            return args[bi], args[li]
    return None


def find_wrappers(fn_tables):
    """Functions that pass two of their parameters straight into a write as (buf, len)."""
    wrappers = {}
    changed = True
    while changed:
        changed = False
        for fns in fn_tables:
            for name, (params, body) in fns.items():
                if name in wrappers:
                    continue
                for (callee, args, _line) in calls(body):
                    shape = write_shape(callee, args, wrappers)
                    if shape is None:
                        continue
                    b, n = single(shape[0], "id"), single(shape[1], "id")
                    if b and n and b[1] in params and n[1] in params:
                        wrappers[name] = (params.index(b[1]), params.index(n[1]))
                        changed = True
                        break
    return wrappers


def source_files(root, top):
    base = os.path.join(root, top)
    out = []
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d not in ("lib", "build", "deps"))
        for f in sorted(filenames):
            if f.endswith((".cyr", ".tcyr", ".bcyr", ".fcyr")):
                out.append(os.path.join(dirpath, f))
    return out


def run(root, verbose):
    parsed = {}
    for top in ("programs", "src", "tests", "fuzz", "examples"):
        for p in source_files(root, top):
            toks = lex(open(p, encoding="utf-8", errors="replace").read())
            parsed[os.path.relpath(p, root)] = (toks, functions(toks))
    src_tables = [fns for rel, (_t, fns) in parsed.items() if rel.startswith("src" + os.sep)]
    problems, sites = [], 0
    for rel, (toks, fns) in sorted(parsed.items()):
        strict = rel.startswith("programs" + os.sep)
        wrappers = find_wrappers([fns] + src_tables)
        for (callee, args, line) in calls(toks):
            shape = write_shape(callee, args, wrappers)
            if shape is None:
                continue
            buf, length = single(shape[0], "str"), single(shape[1], "num")
            if buf is None or length is None:
                continue
            sites += 1
            data = decode(buf[1])
            want = None if data is None else len(data)
            got = int(length[1]) if length[1].isdigit() else None
            label = f'{callee}("{buf[1]}", {length[1]})'
            if strict:
                why = "hand-counted length"
                if want is not None and got != want:
                    why += f" and WRONG: literal is {want} B"
                problems.append(f"{rel}:{line}: {label} — {why}; take the length from strlen "
                                f"(the programs' _e2e_line/_pass/_fail helpers do)")
            elif want is None or got != want:
                problems.append(f"{rel}:{line}: {label} — length {length[1]} but the literal is "
                                f"{want} B" + (" (" + ("over-read" if (got or 0) > (want or 0)
                                                       else "truncated") + ")" if want is not None else ""))
            elif verbose:
                print(f"  ok {rel}:{line}: {label}")
        if strict:
            for (kind, value, line) in toks:
                data = decode(value) if kind == "str" else None
                if data is not None and TALLY.search(data.decode("utf-8", "replace")):
                    problems.append(f'{rel}:{line}: "{value}" — a hard-coded pass tally; print the '
                                    f"count of PASS lines actually emitted")
    for p in problems:
        print(p)
    print(f"\n{sites} literal-buffer write(s) checked across {len(parsed)} file(s): "
          f"{len(problems)} problem(s)")
    return 1 if problems else 0


SELF_TEST = [
    # (relative path, source, expected problem count)
    ("programs/a.cyr", 'fn main() { syscall(1, 1, "hi\\n", 3); return 0; }', 1),
    ("programs/a.cyr", 'fn main() { syscall(1, 1, "hi\\n", 4); return 0; }', 1),
    ("programs/a.cyr", 'fn main() { syscall(SYS_WRITE, 2, "x", 1); return 0; }', 1),
    ("programs/a.cyr", 'fn out(s, n) { syscall(1, 1, s, n); return 0; }\nfn main() { out("ab", 2); }', 1),
    ("programs/a.cyr", 'fn out2(s, n) { out(s, n); return 0; }\nfn out(s, n) { syscall(1, 1, s, n); }\n'
                       'fn main() { out2("ab", 9); }', 1),
    ("programs/a.cyr", 'fn line(s) { syscall(1, 1, s, strlen(s)); return 0; }\nfn main() { line("ok\\n"); }', 0),
    ("programs/a.cyr", 'fn main() { _fail("exit code is not a length", 2); return 0; }', 0),
    ("programs/a.cyr", 'fn main() { syscall(1, 1, "\\n12 passed, 0 failed\\n", 22); }', 2),
    ("programs/a.cyr", 'fn main() { print_cstr("3/3 passed\\n"); }', 1),
    ("programs/a.cyr", 'fn main() { print_cstr("passed: "); print_i64(n); }', 0),
    ("programs/a.cyr", '# syscall(1, 1, "commented out", 3);\nfn main() { return 0; }', 0),
    ("programs/a.cyr", 'fn main() { var w = "fn f() { (,["; syscall(1, 1, "a,b(", 4); }', 1),
    ("src/lib.cyr", 'fn d() { syscall(1, 2, "gpu error: ", 11); }', 0),
    ("src/lib.cyr", 'fn d() { syscall(1, 2, "gpu error: ", 12); }', 1),
    ("examples/x/src/main.cyr", 'fn m() { syscall(1, 1, "tab\\there\\n", 9); }', 0),
    ("examples/x/src/main.cyr", 'fn m() { syscall(1, 1, "\\x41\\u{e9}\\u00e9", 5); }', 0),
    ("tests/tcyr/t.tcyr", 'fn m() { sys_write(1, "abc", 2); }', 1),
]


def self_test():
    failures = 0
    for i, (rel, source, want) in enumerate(SELF_TEST):
        with tempfile.TemporaryDirectory(prefix="mabda-writelen-") as tmp:
            path = os.path.join(tmp, rel)
            os.makedirs(os.path.dirname(path))
            with open(path, "w") as f:
                f.write(source + "\n")
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                run(tmp, False)
            got = sum(1 for ln in buf.getvalue().splitlines() if ln.startswith(rel + ":"))
        ok = got == want
        failures += 0 if ok else 1
        print(f"  {'ok  ' if ok else 'FAIL'} case {i}: {rel}: {got} problem(s), want {want}")
        if not ok:
            print("       " + buf.getvalue().replace("\n", "\n       "))
    print(f"self-test: {len(SELF_TEST) - failures}/{len(SELF_TEST)} cases pass")
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
