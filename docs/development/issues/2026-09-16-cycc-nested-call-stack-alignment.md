# cycc makes a call with rsp 8 bytes off 16-byte alignment when the call is nested in an expression

**Status:** open upstream. Filed in the cyrius repo as `cyrius/docs/development/issues/2026-09-16-mabda-cycc-nested-call-stack-misalignment.md`.
**Discovered:** 2026-09-16, mabda 4.1.3 verification. `programs/benchmarks.cyr` crashed inside
NVK's `create_buffer` during the wgpu-programs fix. The shape was then characterized and traced
to the codegen in the round-2 alignment sweep.
**Toolchain:** `cyrius 6.6.4` (`cycc`, x86_64 SysV). The code below is unchanged at cyrius
`HEAD` `4f3731e8`.
**Component:** compiler (`src/frontend/parse_expr.cyr`, `parse_fn.cyr`; `src/backend/x86/emit.cyr`)
**Severity:** **Low.** It only matters for C interop, which is legacy (in mabda, the wgpu C
launcher path). Nothing fails at compile time, and pure Cyrius code never notices. A C callee that uses aligned SSE on its stack faults with #GP
(SIGSEGV), and gcc emits that for ordinary code. Whether a call faults depends on the driver
and the code path, so the failure looks intermittent and vendor-specific.

## Summary

cycc evaluates expressions on the machine stack. It pushes each call argument (`push rax`)
after evaluating it, and pops them into the argument registers only once the whole list is
evaluated. A binary or comparison operator pushes its left operand while it evaluates the
right one. The function frame is padded to 16 bytes, so rsp is aligned at statement level.
**Nothing pads for the values still pushed when a nested call is emitted.** A call made while
an odd number of those values is pending runs with rsp 8 bytes off.

The shift is also inherited. A Cyrius function entered misaligned runs its whole body
misaligned, so every statement-level call inside it is off too, down to any depth. A second
misaligned nesting flips it back. So the question is not whether a C function is called in a
nested position, but whether anything that can reach C is.

```cyrius
store64(pp, wgpu_device_create_buffer(dev, desc));   # arg 2: 1 pending push -> MISALIGNED
var buf = wgpu_device_create_buffer(dev, desc);      # statement level        -> aligned
store64(pp, buf);
```

## Which shapes misalign (measured)

Each row calls a C leaf that returns `(rsp + 8) & 15` at entry (0 = SysV-aligned, 8 = off by
8). The results are identical on the dev box (Ryzen 7 5800H) and on chew (i7-6700K), both
gcc 16. Log paths in this file are in the 4.1.3 verification scratchpad, not the repo
(`verify-logs/r2-align/`):
`01-alignprobe-shapes.log`, `05-alignprobe-more-operators.log` and
`chew/03-alignprobe-chew.log`.

| Shape (inside a function entered aligned) | Pending pushes | Result |
| --- | --- | --- |
| `var r = c();`, `r = c();`, `return c();`, `if (c() == 0)` | 0 | aligned |
| `f(c(), 0)`: argument 1 | 0 | aligned |
| `f(0, c())`: argument 2 (also `f(x, c())`, `f(0, c(), 0)`) | 1 | **off by 8** |
| `f(0, 0, c())`: argument 3 | 2 | aligned |
| `f(0, 0, 0, c())`: argument 4; `f(..5, c())`: argument 6; `f(..7, c())`: argument 8 of 8 | odd | **off by 8** |
| `f(..4, c(), 0)`: argument 5; `f(..6, c())`: argument 7 of 7 or 8 | even | aligned |
| `store64(&slot, c())` | 1 | **off by 8** |
| `x + c()`, `0 - c()`, `x * c()`, `x << c()`, `16 / (c() + 1)`, `0 == c()`: right operand of an arithmetic, shift, bitwise or comparison operator | 1 | **off by 8** |
| `-c()`: unary minus on a non-literal | 1 | **off by 8** |
| `c() + x` (left operand) | 0 | aligned |
| `f(0, g(0, c()))` | 2 | aligned |
| `f(0, g(c(), 0))`, `f(0, g(0, 0, c()))` | 1, 3 | **off by 8** |
| `var r = fncall1(fp, c())`: fncallN in expression position, user argument 1 | 0 | aligned |
| `fncall2(fp, 0, c())` in expression position: user argument 2 | 1 | **off by 8** |
| `fncall1(fp, c());` as a bare statement | 1 | **off by 8** |
| `fncall2(fp, 0, c());` as a bare statement | 2 | aligned |
| `f(0, h())`, where `h` calls C at statement level | inherited | **off by 8** |
| `f(0, h())`, where `h` itself nests its C call as argument 2 | 1 + 1 | aligned (two faults cancel) |

Rules that follow from the codegen and hold for every row:

- **Direct call:** argument *k* is evaluated with *k* − 1 of that call's arguments pushed
  (`parse_fn.cyr:2133-2134`: `PCMPE(S); EPUSHR(S);` per argument, then `ECALLPOPS` at `:2149`).
- **`fncallN` / `callptr` in expression position** (`PINDIRECT_CALL`, `parse_expr.cyr:335-423`):
  the callee is spilled to a frame slot, not pushed, so user argument *j* sees *j* − 1 pending.
- **`fncallN(...)` as a bare statement** is not lowered. It is an ordinary call into
  `lib/fnptr.cyr`'s `fncallN`, with the callee pushed as argument 1, so user argument *j* sees
  *j* pending. The same source text has **opposite** alignment in the two positions
  (`verify-logs/r2-align/03-alignprobe-fncall-statement-vs-expression.log`). That is why a
  source-text rule is unreliable and the gate reads the compiled object instead.
- **Binary and comparison operators** push the left operand for the duration of the right one:
  `EPUSHR` in `PEXPR` for `+ - & | ^` (`parse_expr.cyr:3588-3703`), `ESPILL` (also `push rax`,
  `emit.cyr:163`) in `_PARSE_TERM_IMPL` for `* / % << >>`, and `EPUSHR` in `_PLOGIC_ATOM`
  (`:3756`) for comparisons. A non-literal unary minus pushes a 0 (`:466`). `&&` and `||`
  balance their push before the right operand, so they add nothing.
- Pending pushes from all enclosing expressions in the statement add up, and a misaligned
  function entry adds one more.

## Root cause

- `EPUSHR` and `ESPILL` (`src/backend/x86/emit.cyr:140`, `:163`) emit `push rax` (8 bytes).
  Nothing records how deep the value stack is at a given point in the code.
- The only alignment invariant is the frame size, `fsz = (flc * 8 + 15) & (0 - 16)`
  (`parse_fn.cyr:5892`), which makes rsp 16-aligned in the body **between** expressions.
- The call emitters (`ECALLPOPS` `emit.cyr:3389`, `ECALLTO` `:3195`, `ECALLFIX` `:3216`,
  `ECALLIND` `:3293`) never adjust for pending pushes. The SysV parity fix in `ECALLPOPS`
  (v5.6.41) handles only the call's **own** stack arguments (odd `nextra`), and it too assumes
  the stack was aligned before the call's pushes began.
- The PE backend already has this class, fixed for indirect calls only: `ECALLPTR_PE` (v6.0.71)
  force-aligns with `and rsp, -16` because "the generic marshal+call below runs at cyrius's
  constant body alignment, which lands callees at ≡ 0 entry (off by 8) — fine for cyrius
  callees (no SSE), fatal for real Win64 ones" (`parse_expr.cyr:397-402`).
- aarch64 is not affected: its `EPUSHR` is `str x0, [sp, #-16]!` (`backend/aarch64/emit.cyr:264`),
  so SP stays 16-aligned. PE direct calls were not checked (not a mabda target).

## Reconciling "cyrius 6.3.26 proved 16-byte alignment correct"

Cheat-sheet entry B1 and cyrius `docs/ffi/fncall-abi.md` say 6.3.26 proved `fncallN`
arg-passing and 16-byte alignment correct into extern C. For what that gate measures, it is
right, but it measures only positions this bug never touches:

- `tests/gates/platform/ffi_stack_protected_extern_c.sh` calls `fncall4..7` at **top level**,
  in left-operand position (`if (fncall4(f4, 1, 2, 3, 4) != 10)`). No value is pending
  there, so every call it makes is aligned. Its C callees would catch a misaligned call:
  compiled the way the gate compiles them (gcc 16 `-O2 -fstack-protector-all`), `sum4..sum7`
  copy their `volatile long s[N]` initializers with `movdqa` / `movaps` on stack slots
  (`verify-logs/r2-align/07-upstream-ffi-gate-callees-use-aligned-sse.log`). The gate's pass
  shows that position is aligned. It says nothing about a nested one.
- `tests/tcyr/codegen/sysv_odd_stack_args.tcyr` (v5.6.41) calls a Cyrius leaf whose comment
  describes it as using `movdqu`, which "doesn't fault on misaligned addresses".

So B1's conclusion stands for what it tested: statement-level calls are aligned, and the old
`fncall6` crash was `%fs`. The general claim that rsp is 16-aligned at every call is false for
nested calls.

## Reproduction (CPU only)

```sh
cat > leaf.c <<'EOF'
extern void _cyrius_init(void);
extern long run(void);
long rsp_mod(void);    /* (rsp + 8) & 15 at entry */
long sse_leaf(void);   /* movaps to a stack slot: #GP when misaligned */
__asm__(".globl rsp_mod\nrsp_mod:\n lea 8(%rsp),%rax\n and $15,%rax\n ret\n"
        ".globl sse_leaf\nsse_leaf:\n sub $24,%rsp\n xorps %xmm0,%xmm0\n"
        " movaps %xmm0,(%rsp)\n add $24,%rsp\n xor %eax,%eax\n ret\n");
int main(void) { _cyrius_init(); return (int)run(); }
EOF
cat > nested.cyr <<'EOF'
fn second(a, b): i64 { return b; }
fn run(): i64 {
    var direct = rsp_mod();                # 0
    var nested = second(0, rsp_mod());     # 8 on cyrius 6.6.4
    syscall(1, 1, "direct/nested: ", 15);
    var d[3]; store8(&d, 48 + direct); store8(&d + 1, 47); store8(&d + 2, 48 + nested);
    syscall(1, 1, &d, 3); syscall(1, 1, "\n", 1);
    var slot[8];
    store64(&slot, sse_leaf());            # SIGSEGV (exit 139) on cyrius 6.6.4
    return 0;
}
EOF
gcc -c leaf.c -o leaf.o
printf 'object;\n' | cat - nested.cyr | cycc > nested.o     # warns: undefined rsp_mod/sse_leaf
gcc leaf.o nested.o -o nested && ./nested; echo "exit=$?"
# direct/nested: 0/8
# exit=139
```

`var s = sse_leaf(); store64(&slot, s);` in place of the last call exits 0.

## Seen in the wild (NVK, non-hanging, user-space only)

`src/compute.cyr` `ping_pong_new` (public API) did `store64(pp, wgpu_device_create_buffer(...))`.
A probe program puts an asm trampoline in fn-table slot 8 to record entry alignment at the real
C boundary, then runs `ping_pong_new` on chew (GTX 1660 SUPER, NVK / mesa `vulkan-nouveau`
26.2.2):

- Unfixed: `TRAMP: create_buffer entered with rsp 8 bytes off alignment`, then SIGSEGV (exit
  139). Kernel: `general protection fault ... in libvulkan_nouveau.so`, at
  `movdqa -0x30(%rbp),%xmm0` (`verify-logs/r2-align/chew/11-*.log`, `12-*.log`).
- Fixed (the call hoisted to its own statement): both entries aligned, both buffers created,
  exit 0 (`chew/10-*.log`). No GPU reset in either run.

## Expected vs actual

- **Expected:** rsp is 16-aligned at every `call`, whatever expression the call sits in, as
  the SysV ABI requires and cyrius `docs/guides/cyrius-guide.md:2148` promises ("16-byte aligned
  on entry to any function").
- **Actual:** aligned only when an even number of values is pending in the enclosing
  expression and the enclosing function was itself entered aligned.

## Workaround in mabda

- **Never let a call that can reach C be evaluated inside another expression.** Bind it to a
  local first (`var h = f(...);`), then use the local. That covers wgpu/samvada fn-table calls
  and any Cyrius function that eventually makes one. Hoisting is always safe; the table above
  is for reading old code, not for deciding when to skip the hoist.
- **Gate:** `scripts/check-ffi-call-alignment.py`. It compiles `src/lib.cyr` and every
  `programs/*.cyr` in `object;` mode and walks rsp depth through each function in the
  disassembly. It fails on any misaligned call to C, or to a function that can reach C (every
  indirect call counts except the allocator vtable). Before each run it checks itself against a
  fixture of these shapes, executed against a C leaf on the build machine. It found 4 sites in
  `src/`, all fixed in 4.1.3:
  - `src/compute.cyr` `ping_pong_new`: two `create_buffer` calls as `store64`'s argument 2.
  - `src/render_graph.cyr` `_rg_execute_native_mq`: `gpu_queue_get` as argument 2 of
    `gpu_queue_barrier` and of `gpu_queue_wait_idle`. Shape-only today: that executor runs on
    the native backend, and the wgpu `queue_get` slot does not call C. The gate cannot tell
    which backend fills a slot, so it counts every slot dispatch as reaching C.
  - `programs/`, `tests/`, `tests/bcyr/` and `fuzz/`: 0.
- **Regression test:** `tests/tcyr/compute.tcyr`
  `test_ping_pong_new_ffi_calls_stack_aligned`. A slot-8 mock compares its own frame address
  against a statement-level baseline, and 2 assertions fail against the unfixed `ping_pong_new`.
- **Consumers** (soorat, rasa, ranga, bijli, aethersafta) call mabda's wrappers from their own
  code and are exposed the same way. They need the same hoisting discipline, or this gate run
  over their programs, until cycc is fixed.

## Suggested upstream fix

1. Track the pending push depth in the x86 SysV emitter: increment in `EPUSHR` / `ESPILL`,
   decrement in `EPOPR` / `EPOPC` / `EUNSPILL` / `ECALLPOPS`, and save and restore it across
   branches, which are already balanced. At each call emission with an odd depth, emit
   `sub rsp, 8` before and `add rsp, 8` after, folded into `ECALLPOPS` / `ECALLCLEAN` the way
   the v5.6.41 `nextra` parity already is.
2. Or, as a narrower fix, force-align extern and indirect calls the way `ECALLPTR_PE` does.
   That leaves Cyrius-to-Cyrius calls misaligned, and because the shift is inherited, a Cyrius
   helper that later calls C at statement level would still fault. (1) is the real fix.
3. Regression gate: a C leaf that measures `(rsp + 8) & 15` **and** one that does `movaps` or
   `movdqa` on a stack slot (not `movdqu`), called through every shape in the table above,
   including `fncallN` in both statement and expression position and through one intermediate
   Cyrius frame.

## Upstream status

Filed 2026-09-16 as `cyrius/docs/development/issues/2026-09-16-mabda-cycc-nested-call-stack-misalignment.md`.
