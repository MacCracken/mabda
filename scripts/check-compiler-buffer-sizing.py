#!/usr/bin/env python3
"""check-compiler-buffer-sizing.py — static gate for the SPIR-V→GFX9 compiler's
caller-provided scratch buffers.

WHY THIS EXISTS
---------------
The compiler pipeline takes every scratch buffer from its caller and is told the
CAPACITY separately, as a plain integer. Several stages then `memset` the full
capacity before use:

    mir_mod_init         memset(vals,  0,    cap_ids  * MIR_VAL_REC)     src/mir.cyr
    gfx9_regalloc        memset(alloc, 0xFF, cap_ids  * RA_REC)          src/gfx9_regalloc.cyr
    _spirv_resolve_...   memset(outbi, 0xFF, id_bound * 8)               src/spirv_lower.cyr
    spirv_build_*_table  memset(out,   0,    id_bound * SPIRV_*_REC)     src/spirv_parse.cyr

A callee cannot bounds-check a raw pointer, so if the declared buffer is smaller
than capacity * record-size the memset runs off the end of the caller's STACK and
silently corrupts whatever follows.

⛔ This is not hypothetical. `programs/native_spirv_saxpy_e2e.cyr` declared
`vals[1440]` while passing `cap_ids = 40` (needing 40 * 40 = 1600). The 160-byte
overrun landed on the adjacent MirMod header and zeroed `m + 64` — the cap_ids
field itself. Every subsequent id then failed `id >= cap_ids` (0) and the compile
returned `MIR_ERR_ID_OOR`, pointing at "your ids are out of range" when the real
fault was "your vals buffer is too small". That gate was red for four releases
(4.0.5 -> 4.0.10) and the misleading error is exactly why.

⭐ The bug is invisible at runtime and trivial at compile time: both the buffer
size and the capacity are integer literals sitting a few lines apart. Hence a
static gate rather than a runtime check.

USAGE
    scripts/check-compiler-buffer-sizing.py [--verbose]
Exit 0 = every call site adequately sized. Exit 1 = at least one deficit.
"""
import glob
import os
import re
import sys

# Record sizes, mirrored from src/. Kept here as literals ON PURPOSE: if someone
# changes a REC in src/ without updating this gate, the gate's numbers stop
# matching and the mismatch is caught by test_compiler_buffer_sizing_recs in
# tests/tcyr/compiler_compile.tcyr, which asserts these exact values.
MIR_VAL_REC = 40
MIR_INSTR_REC = 32
MIR_PTR_REC = 32
RA_REC = 16
SPIRV_TYPE_REC = 24
SPIRV_CONST_REC = 16
SPIRV_DECO_REC = 16
OUTBI_REC = 8
GISEL_REC = 48
CMP_ISEL_CAP = 256

DECL = re.compile(r"\bvar\s+(\w+)\s*\[\s*(\d+)\s*\]")
CALL = re.compile(
    r"mir_mod_init\(\s*&?(\w+)\s*,\s*&?(\w+)\s*,\s*&?(\w+)\s*,\s*&?(\w+)\s*,"
    r"\s*&?(\w+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)\s*\)"
)
# `store64(&cc + CC_ALLOC, &alloc);` — the cc-struct wiring for the buffers that
# reach the later stages.
CC_STORE = re.compile(r"store64\(\s*&(\w+)\s*\+\s*(CC_\w+)\s*,\s*&(\w+)\s*\)")
CC_CAP = re.compile(r"store64\(\s*&(\w+)\s*\+\s*CC_CAP_IDS\s*,\s*(\w+)\s*\)")

# cc slot -> (record size, "sized by cap_ids", human name)
CC_SLOTS = {
    "CC_TYPES": (SPIRV_TYPE_REC, "types"),
    "CC_CONSTS": (SPIRV_CONST_REC, "consts"),
    "CC_DECO": (SPIRV_DECO_REC, "deco"),
    "CC_OUTBI": (OUTBI_REC, "outbi"),
    "CC_ALLOC": (RA_REC, "alloc"),
    "CC_ISEL": (GISEL_REC, "isel"),
    "CC_ISEL2": (GISEL_REC, "isel2"),
}


def sources():
    out = []
    for root in ("src", "programs", "tests", "fuzz"):
        for ext in ("*.cyr", "*.tcyr", "*.bcyr", "*.fcyr"):
            out += glob.glob(os.path.join(root, "**", ext), recursive=True)
    return sorted(set(out))


FN_START = re.compile(r"^fn\s+\w+\s*\(")


def enclosing_fn(lines, idx):
    """[start, end) line range of the function containing `idx`.

    ⚠ Scanning merely BACKWARDS from the call is wrong twice over: it walks past
    the function header into a previous function (reporting a stale buffer of the
    same name — an earlier draft of this gate blamed `alloc[256]` from one test
    for a call in the next, which declares `alloc[768]`), and it misses buffers
    declared AFTER mir_mod_init, which is where several call sites put them.
    """
    start = 0
    for j in range(idx, -1, -1):
        if FN_START.match(lines[j]):
            start = j
            break
    end = len(lines)
    for j in range(idx + 1, len(lines)):
        if FN_START.match(lines[j]):
            end = j
            break
    return start, end


def decls_in_fn(lines, idx):
    """Every `var NAME[N]` in the function containing `idx`.

    A name declared twice in one function keeps the LARGER size: the gate must not
    fire on a redeclaration that is adequately sized.
    """
    start, end = enclosing_fn(lines, idx)
    seen = {}
    for j in range(start, end):
        for d in DECL.finditer(lines[j]):
            nm, sz = d.group(1), int(d.group(2))
            if nm not in seen or sz > seen[nm]:
                seen[nm] = sz
    return seen


def main():
    verbose = "--verbose" in sys.argv
    problems = []
    sites = 0

    for path in sources():
        try:
            lines = open(path).read().split("\n")
        except (OSError, UnicodeDecodeError):
            continue
        joined = "\n".join(lines)
        if "mir_mod_init" not in joined:
            continue

        consumed = set()
        for i, line in enumerate(lines):
            if i in consumed:
                continue
            # A call may wrap onto the next line. Mark the continuation consumed so
            # one physical call is never counted twice (which would double every
            # deficit and make the totals meaningless).
            probe = line
            if CALL.search(line):
                mm = CALL.search(line)
            else:
                nxt = lines[i + 1] if i + 1 < len(lines) else ""
                probe = line + " " + nxt
                mm = CALL.search(probe)
                if mm:
                    consumed.add(i + 1)
            if not mm:
                continue
            # ⚠ Skip call sites that are ARGUMENTS to an assertion. Those are
            # negative tests deliberately passing a rejected configuration to
            # prove mir_mod_init rejects it (see
            # test_mir_mod_init_contract in tests/tcyr/compiler_lower.tcyr).
            # Flagging them would make the gate fire on the very tests that
            # defend it, and the only way to quiet it would be deleting them.
            # ⚠ Test `probe`, not `line`. When the call wraps, the match is
            # attributed to the PRECEDING line — often a comment — and testing
            # that line alone finds no "assert" and skips nothing.
            if "assert" in probe:
                continue
            sites += 1
            (_m, vals, instrs, ptrs, _blocks,
             cap_instrs, cap_ptrs, id_bound, cap_ids) = mm.groups()
            cap_instrs, cap_ptrs = int(cap_instrs), int(cap_ptrs)
            id_bound, cap_ids = int(id_bound), int(cap_ids)
            sizes = decls_in_fn(lines, i)

            def check(name, need, why):
                have = sizes.get(name)
                if have is None:
                    return  # declared elsewhere (a param, or a helper's buffer)
                if have < need:
                    problems.append(
                        f"{path}:{i + 1}: {name}[{have}] needs {need} B ({why}) "
                        f"— DEFICIT {need - have} B"
                    )
                elif verbose:
                    print(f"  ok {path}:{i + 1} {name}[{have}] >= {need} ({why})")

            # Eagerly memset by mir_mod_init itself — an undersized vals corrupts
            # unconditionally, at init, before any lowering runs.
            check(vals, cap_ids * MIR_VAL_REC, f"EAGER memset: {cap_ids} ids x {MIR_VAL_REC}")
            # Written on demand, bounded by the cap — a deficit here is LATENT and
            # only fires once a program emits that many records.
            check(instrs, cap_instrs * MIR_INSTR_REC, f"{cap_instrs} x {MIR_INSTR_REC}")
            check(ptrs, cap_ptrs * MIR_PTR_REC, f"{cap_ptrs} x {MIR_PTR_REC}")

            if id_bound > cap_ids:
                problems.append(
                    f"{path}:{i + 1}: id_bound {id_bound} > cap_ids {cap_ids} "
                    f"— the SPIR-V ids alone do not fit vals[] "
                    f"(id_bound == cap_ids is legal: no synth headroom)"
                )

            # The cc-wired buffers, if this call site also builds a cc struct.
            # Scan forward for the stores; they follow mir_mod_init in every
            # existing site but tolerate either order.
            fn_start, fn_end = enclosing_fn(lines, i)
            window = lines[fn_start:fn_end]
            declared_cap = None
            for w in window:
                cm = CC_CAP.search(w)
                if cm and cm.group(2).isdigit():
                    declared_cap = int(cm.group(2))
            for w in window:
                sm = CC_STORE.search(w)
                if not sm:
                    continue
                slot, buf = sm.group(2), sm.group(3)
                if slot not in CC_SLOTS:
                    continue
                rec, label = CC_SLOTS[slot]
                if slot in ("CC_ISEL", "CC_ISEL2"):
                    # gfx9_compile always passes CMP_ISEL_CAP to gfx9_isel, which
                    # bounds its writes on that value — so any caller of
                    # gfx9_compile must size isel/isel2 for the full cap.
                    need, why = CMP_ISEL_CAP * rec, f"CMP_ISEL_CAP {CMP_ISEL_CAP} x {rec}"
                elif slot == "CC_ALLOC":
                    # ⚠ gfx9_regalloc reads cap_ids from the MIR header
                    # (`load64(m + 64)`), NOT from CC_CAP_IDS. The two can and do
                    # differ at the same call site, so using CC_CAP_IDS here would
                    # under-require the buffer.
                    need = cap_ids * rec
                    why = f"{label}: EAGER memset: mir cap_ids {cap_ids} x {rec}"
                else:
                    c = declared_cap if declared_cap is not None else cap_ids
                    eager = " EAGER memset:" if slot in ("CC_OUTBI", "CC_TYPES",
                                                         "CC_CONSTS", "CC_DECO") else ""
                    need, why = c * rec, f"{label}:{eager} CC_CAP_IDS {c} x {rec}"
                check(buf, need, why)

    for p in sorted(set(problems)):
        print(p)
    print(f"\n{sites} mir_mod_init call site(s) checked, "
          f"{len(set(problems))} problem(s)")
    return 1 if problems else 0


if __name__ == "__main__":
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    sys.exit(main())
