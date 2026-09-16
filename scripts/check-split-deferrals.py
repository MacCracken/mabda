#!/usr/bin/env python3
"""check-split-deferrals.py — static gate: no deferral phrase split across two comment lines.

WHY THIS EXISTS
---------------
`cyrlint --strict-deferrals` flags deferral language that carries no tracking pointer, but
it reads ONE LINE AT A TIME. A multi-word phrase wrapped across a comment line break
("... is a later" / "# bite ...") is invisible to it, so the deferral ships untracked with
the gate green. The 4.1.3 verification found four such comments in src/ (gfx9_compile.cyr
twice, compute.cyr twice): one described work that had already landed, and the rest were
open work with no pointer anywhere.

RULE
----
For every pair of consecutive lines that both carry a `#` comment, FAIL when one of
cyrlint's multi-word deferral terms starts in the first comment and ends in the second.
The fix is always to reflow the comment so the phrase sits on one line. cyrlint then
applies its own tracking-pointer rule to that line, so this gate never needs a second
definition of "tracked". A pair where either line carries `#skip-lint` is skipped, as
cyrlint skips such a line.

TERMS mirrors `_line_deferral_term` in cyrius programs/cyrlint.cyr (6.6.4), case-sensitive
like cyrlint. Single-token terms (TODO, FIXME, deferred, ...) cannot be split by a line
break and are left to cyrlint. Re-check the list on a toolchain pin bump.

A built-in self-test runs first and fails the gate if the scanner stops catching a split
phrase or starts flagging a whole one.

USAGE
    scripts/check-split-deferrals.py [FILE ...]
FILE defaults to src/*.cyr programs/*.cyr tests/tcyr/*.tcyr tests/tcyr/*.cyr
tests/bcyr/*.bcyr fuzz/*.fcyr. Exit 0 = clean, 1 = split phrase found or self-test failed.
"""
import glob
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TERMS = ["for now", "not yet", "later bite", "future bite", "out of scope", "follow-up"]
DEFAULT_GLOBS = ["src/*.cyr", "programs/*.cyr", "tests/tcyr/*.tcyr", "tests/tcyr/*.cyr",
                 "tests/bcyr/*.bcyr", "fuzz/*.fcyr"]


def comment_text(line):
    """Text after the first `#` outside a string literal, or None."""
    in_str = False
    prev = ""
    for i, c in enumerate(line):
        if c == '"' and prev != "\\":
            in_str = not in_str
        elif c == "#" and not in_str:
            return line[i + 1:]
        prev = c
    return None


def split_hits(lines):
    """[(line_no_of_first_line, term)] for phrases spanning two consecutive comments."""
    hits = []
    for i in range(len(lines) - 1):
        a = comment_text(lines[i])
        b = comment_text(lines[i + 1])
        if a is None or b is None:
            continue
        if "#skip-lint" in lines[i] or "#skip-lint" in lines[i + 1]:
            continue
        head = a.rstrip()
        tail = b.strip()
        # A word break joins with a space; a hyphenated break ("follow-" / "up") joins directly.
        for sep in (" ", ""):
            joined = head + sep + tail
            for term in TERMS:
                start = joined.find(term)
                while start >= 0:
                    if start < len(head) and start + len(term) > len(head) + len(sep):
                        hits.append((i + 1, term))
                    start = joined.find(term, start + 1)
    return sorted(set(hits))


SELF_TEST_BAD = [
    "# the immediate-offset form is a later\n# bite for the encoder\n",
    "# this format is not\n# yet final\n",
    "var x = 0;   # used for\nvar y = 0;   # now only\n",
    "# see the follow-\n# up in the issue\n",
    "# that is out\n# of scope here\n",
    # Substring match, exactly like cyrlint: "for nowhere" on one line is flagged there too.
    "# waiting for\n# nowhere\n",
]
SELF_TEST_GOOD = [
    "# this is a later bite (docs/development/roadmap.md)\n# second line\n",
    "# not\n",
    "# for\nfn f(): i64 { return 0; }\n# now\n",
    '# a "#" in a string\nvar s = "for # now";\n',
    "# for #skip-lint\n# now\n",
]


def self_test():
    ok = True
    for text in SELF_TEST_BAD:
        if not split_hits(text.split("\n")):
            print("SELF-TEST FAIL: split phrase not caught: %r" % text)
            ok = False
    for text in SELF_TEST_GOOD:
        hits = split_hits(text.split("\n"))
        if hits:
            print("SELF-TEST FAIL: false positive %s on: %r" % (hits, text))
            ok = False
    return ok


def main():
    if not self_test():
        return 1
    files = sys.argv[1:]
    if not files:
        for g in DEFAULT_GLOBS:
            files.extend(sorted(glob.glob(os.path.join(ROOT, g))))
    if not files:
        print("check-split-deferrals: no files to scan")
        return 1
    n = 0
    for path in files:
        with open(path, encoding="utf-8", errors="replace") as f:
            lines = f.read().split("\n")
        for line_no, term in split_hits(lines):
            rel = os.path.relpath(path, ROOT)
            print("FAIL %s:%d-%d: '%s' is split across two comment lines, where cyrlint cannot "
                  "see it. Reflow so it sits on one line." % (rel, line_no, line_no + 1, term))
            n += 1
    if n:
        print("check-split-deferrals: %d split deferral phrase(s)" % n)
        return 1
    print("check-split-deferrals: OK (%d files)" % len(files))
    return 0


if __name__ == "__main__":
    sys.exit(main())
