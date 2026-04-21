# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in mabda, please report it
responsibly through **GitHub Security Advisories**:

1. Go to the [Security tab](../../security/advisories) of this repository
2. Click **"Report a vulnerability"**
3. Fill in the details and submit

**Do not open a public issue for security vulnerabilities.**

## Scope

This policy covers the mabda library and its published API. Mabda owns
the wgpu-native FFI boundary — vulnerabilities in wgpu-native itself or
in the underlying GPU driver (Vulkan / Metal / DX12) should be reported
to the respective upstream projects. If a wgpu-native or driver bug
affects mabda users specifically, flag it here and we will either
harden the wrapper, document the workaround, or both.

## Supported Versions

| Version | Supported                                                |
|---------|----------------------------------------------------------|
| 2.5.x   | **Yes** — current release, receives security fixes       |
| 2.4.x   | Yes — receives security fixes via back-ports on request  |
| < 2.4   | No                                                       |

## Response Timeline

| Action                           | Target                     |
|----------------------------------|----------------------------|
| Acknowledgement                  | Within **48 hours**        |
| Initial assessment               | Within **5 business days** |
| Fix for CRITICAL severity        | Within **14 days**         |
| Fix for HIGH severity            | Within **30 days**         |
| Fix for MEDIUM / LOW severity    | Next scheduled release     |

Severity ladder: **CRITICAL** (exploitable immediately) / **HIGH**
(moderate effort) / **MEDIUM** (specific conditions) / **LOW**
(defense-in-depth). Same rubric the internal audits use.

## Audit History

Mabda runs a P(-1) scaffold-hardening pass before every minor / major
bump — code walk of every `src/*.cyr` module against the Security
Hardening checklist in `CLAUDE.md`, plus an external CVE sweep of
adjacent GPU / wgpu surfaces. Findings are filed as
`docs/audit/YYYY-MM-DD-audit.md`.

| Date       | Release | Findings                     | Report                                                        |
|------------|---------|------------------------------|---------------------------------------------------------------|
| 2026-04-19 | 2.3.0   | 0 CRITICAL / 2 HIGH / 6 MED / 6 LOW | [`docs/audit/2026-04-19-audit.md`](docs/audit/2026-04-19-audit.md) |

2.3.0 was the last audit-gated stdlib-candidate release. From 2.4.0
through 2.5.x, the `CLAUDE.md` Security Hardening checklist has been
the rolling internal gate. Every v2.4.x patch that fixed a latent
bug (e.g. v2.4.2 FFI offset sweep, v2.4.4 `depth_texture_new` /
`rtb_build` stub fixes) landed with a CPU regression assertion in
`tests/tcyr/mabda.tcyr` so the bug can't re-enter. The next full
audit pass will land alongside the v3.0 backend swap (native Cyrius
GPU backend — see ADR 004 / planned ADR 006).

## Design Principles

- **No memory-unsafe primitives** — Cyrius has no raw pointer
  arithmetic in user code; struct access is `load64`/`store64` with
  named offset constants documented per module
- **No libc** — all syscalls go through the Cyrius stdlib or the C
  launcher; mabda's own source has one direct syscall
  (`clock_gettime` in the profiler, return-value-guarded since 2.3.0)
- **No I/O** — mabda does not touch the filesystem or network
- **Input validation at consumer boundaries** — every function
  accepting consumer-supplied dimensions, sizes, or descriptor
  fields validates bounds before use; `a * b` on sizes is
  overflow-guarded; `/` on divisors is zero-guarded
- **`fncall6` avoidance** — wgpu-native calls with 6+ i64 args go
  through C struct-packing shims to avoid the Cyrius `fncall6` +
  wgpu-native ABI crash
- **Audit-driven regressions** — every fix from an audit pass lands
  with an assertion in `tests/tcyr/mabda.tcyr` that would have caught
  the original bug
- **No vendored dependencies** — mabda tracks the installed Cyrius
  toolchain via a `lib/` symlink; wgpu-native is consumer-provided
  at the edge

## Disclosure

We follow coordinated disclosure. Once a fix is released, we will
publish a security advisory crediting the reporter (unless anonymity
is requested). Audit findings that surface internally are disclosed
through `docs/audit/*.md` and the corresponding CHANGELOG entry.
