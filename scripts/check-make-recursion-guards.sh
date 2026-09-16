#!/bin/sh
# check-make-recursion-guards.sh — prove that every Makefile recipe which calls $(MAKE)
# stops itself under `make -n`, `-t` and `-q` (and --dry-run / --touch / --question).
#
# Usage: scripts/check-make-recursion-guards.sh [MAKEFILE]   (default: ./Makefile)
#        scripts/check-make-recursion-guards.sh --self-test
# The make under test is $MAKE (default: make). Needs no compiler and no GPU.
#
# WHY: GNU make runs any recipe line that references $(MAKE) even under -n, -t and -q
# ("the -t, -n and -q options do not apply to such lines"), and the inner make inherits
# the flag. Before 4.1.3 `make -n test-native-all` walked every native gate as a dry run
# and printed "71 passed" for gates that never ran; `-t` does the same, and `-q` runs the
# body with every inner gate "failing". The Makefile's RECURSION_GUARD stops such a recipe:
# under -n / -t it prints "<target>: not run under make -n / -t" and succeeds; under -q it
# exits 1 silently, which is what `make -q` answers for any phony target.
#
# HOW: every recipe containing $(MAKE) is found by scanning the Makefile, then run under
# each flag with MAKE= pointed at a stub that only records that it was called. A guarded
# recipe never reaches the stub. Passing requires, per flag:
#   -n / -t : exit 0, the exact guard line printed, the stub never called
#   -q      : exit 1, no output,                    the stub never called
# The known recursive recipes (test-native-all, check-fuzz-logs) must be among those
# found, so a broken scan cannot pass by checking nothing.
set -u

MAKE_BIN=${MAKE:-make}
FLAGS="-n --dry-run -t --touch -q --question"

# Recipes that reference $(MAKE): the target of the nearest preceding rule line.
recursive_targets() {
    awk '/^[A-Za-z0-9_.\/%-]+:([^=]|$)/ { split($0, parts, ":"); target = parts[1] }
         /^\t/ && index($0, "$(MAKE)") && target != "" { print target }' "$1" | sort -u
}

# check_one MAKEFILE TARGET FLAG -> 0 when the recipe stopped itself correctly.
# (POSIX sh has no `local`: every variable here is co_-prefixed so callers keep theirs.)
check_one() {
    co_mf=$1 co_target=$2 co_flag=$3
    co_work=$(mktemp -d "${TMPDIR:-/tmp}/mabda-recguard-XXXXXX") || return 2
    co_stub="$co_work/make-stub"
    printf '#!/bin/sh\ntouch "%s/recursed"\nexit 0\n' "$co_work" > "$co_stub"
    chmod +x "$co_stub"
    co_out=$(cd "$(dirname "$co_mf")" && "$MAKE_BIN" --no-print-directory "$co_flag" \
             -f "$(basename "$co_mf")" "$co_target" MAKE="$co_stub" 2>&1)
    co_rc=$?
    co_why=""
    [ -e "$co_work/recursed" ] && co_why="its body ran and called \$(MAKE)"
    case "$co_flag" in
        -q|--question)
            [ "$co_rc" -eq 1 ] || co_why="${co_why:+$co_why; }exit $co_rc, want 1"
            [ -z "$co_out" ] || co_why="${co_why:+$co_why; }printed output under -q" ;;
        *)
            [ "$co_rc" -eq 0 ] || co_why="${co_why:+$co_why; }exit $co_rc, want 0"
            printf '%s\n' "$co_out" | grep -qxF "$co_target: not run under make -n / -t" \
                || co_why="${co_why:+$co_why; }no guard line" ;;
    esac
    rm -rf "$co_work"
    if [ -n "$co_why" ]; then
        echo "  FAIL make $co_flag $co_target: $co_why"
        printf '%s\n' "$co_out" | tail -4 | sed 's/^/        | /'
        return 1
    fi
    return 0
}

self_test() {
    real_mf=$1
    work=$(mktemp -d "${TMPDIR:-/tmp}/mabda-recguard-self-XXXXXX") || exit 2
    trap 'rm -rf "$work"' EXIT
    # The guard under test is the real Makefile's, copied verbatim.
    guard_defs=$(grep -E '^(MAKE_MODE_FLAGS|MAKE_NO_RUN|RECURSION_GUARD) *=' "$real_mf")
    [ "$(printf '%s\n' "$guard_defs" | wc -l)" -eq 3 ] || {
        echo "self-test: MAKE_MODE_FLAGS / MAKE_NO_RUN / RECURSION_GUARD not found in $real_mf"; exit 1; }
    tab=$(printf '\t')
    {
        printf '%s\n' "$guard_defs"
        printf '.PHONY: guarded unguarded n_only leaf\n'
        printf 'leaf:\n%s@true\n' "$tab"
        loop='for t in leaf leaf; do $(MAKE) -f Makefile $$t; done'
        n_only='$(if $(findstring n,$(firstword -$(MAKEFLAGS))),echo "n_only: not run under make -n / -t"; exit 0;)'
        printf 'guarded:\n%s@$(RECURSION_GUARD) \\\n%s%s\n' "$tab" "$tab" "$loop"
        printf 'unguarded:\n%s@%s\n' "$tab" "$loop"
        printf 'n_only:\n%s@%s \\\n%s%s\n' "$tab" "$n_only" "$tab" "$loop"
    } > "$work/Makefile"
    fail=0
    expect() {   # expect <target> <flag> <0=must pass|1=must fail>
        if check_one "$work/Makefile" "$1" "$2" > "$work/out" 2>&1; then got=0; else got=1; fi
        if [ "$got" -eq "$3" ]; then
            echo "  ok   $1 $2 ($([ "$3" -eq 0 ] && echo stops || echo caught))"
        else
            echo "  FAIL $1 $2: expected $([ "$3" -eq 0 ] && echo pass || echo failure)"
            sed 's/^/        /' "$work/out"
            fail=1
        fi
    }
    for fl in $FLAGS; do expect guarded "$fl" 0; done
    for fl in $FLAGS; do expect unguarded "$fl" 1; done
    expect n_only -n 0
    for fl in -t --touch -q --question; do expect n_only "$fl" 1; done
    found=$(recursive_targets "$work/Makefile" | tr '\n' ' ')
    if [ "$found" = "guarded n_only unguarded " ]; then
        echo "  ok   scan finds exactly the \$(MAKE) recipes"
    else
        echo "  FAIL scan found [$found]"; fail=1
    fi
    [ $fail -eq 0 ] && echo "self-test: OK" || echo "self-test: FAILED"
    return $fail
}

if [ "${1:-}" = "--self-test" ]; then
    self_test "${2:-Makefile}"
    exit $?
fi

mf=${1:-Makefile}
[ -f "$mf" ] || { echo "check-make-recursion-guards: no makefile '$mf'"; exit 2; }
targets=$(recursive_targets "$mf" | tr '\n' ' ')
missing=""
for need in test-native-all check-fuzz-logs; do
    case " $targets " in *" $need "*) ;; *) missing="$missing $need" ;; esac
done
bad=0
if [ -n "$missing" ]; then
    echo "  FAIL the scan did not find the known recursive recipe(s):$missing"
    bad=1
fi
for t in $targets; do
    for fl in $FLAGS; do
        check_one "$mf" "$t" "$fl" || bad=1
    done
done
if [ $bad -ne 0 ]; then
    echo "check-make-recursion-guards: FAILED — a \$(MAKE) recipe runs its body under -n/-t/-q, or a known one is missing"
    exit 1
fi
echo "check-make-recursion-guards: OK — $(echo $targets | wc -w) recursive recipe(s) stop under $FLAGS: $targets"
