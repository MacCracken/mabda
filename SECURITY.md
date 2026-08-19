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
| 4.0.x   | **Yes** — current release, receives security fixes       |
| 3.4.x   | Yes — receives security fixes via back-ports on request  |
| < 3.4   | No                                                       |

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

| Date       | Release | Findings | Report |
|------------|---------|----------|--------|
| 2026-04-19 | 2.3.0   | 0 CRITICAL / 2 HIGH / 6 MED / 6 LOW | [`2026-04-19-audit.md`](docs/audit/2026-04-19-audit.md) |
| 2026-04-30 | 3.0.0   | 0 CRITICAL / 2 HIGH | [`2026-04-30-audit.md`](docs/audit/2026-04-30-audit.md) |
| 2026-06-14 | 3.0.4   | 0 CRITICAL / 4 HIGH — full-surface post-GA re-audit | [`2026-06-14-audit.md`](docs/audit/2026-06-14-audit.md) |
| 2026-06-15 | 3.2.0   | 0 CRITICAL / 0 HIGH / 0 MED — Phase-T compressed textures | [`2026-06-15-audit.md`](docs/audit/2026-06-15-audit.md) |
| 2026-06-15 | 3.2.1   | 0 CRITICAL / 2 HIGH / 1 MED — Phase-X buffer copy | [`2026-06-15-buffer-copy-audit.md`](docs/audit/2026-06-15-buffer-copy-audit.md) |
| 2026-06-15 | 3.2.2   | 0 CRITICAL / 2 HIGH → fixed — Phase-TS sampler/image | [`2026-06-15-ts-sampling-audit.md`](docs/audit/2026-06-15-ts-sampling-audit.md) |
| 2026-06-16 | 3.2.3   | 0 CRITICAL / 2 HIGH → fixed — Phase-TS.6–8 | [`2026-06-16-ts678-audit.md`](docs/audit/2026-06-16-ts678-audit.md) |
| 2026-06-16 | 3.2.4   | 0 CRITICAL / 0 HIGH / 0 MED — Phase S SPIR-V ingestion | [`2026-06-16-phaseS-audit.md`](docs/audit/2026-06-16-phaseS-audit.md) |
| 2026-06-16 | 3.2.5   | 0 CRITICAL / 0 HIGH / 0 MED — Phase N.0/N.1 compiler foundation | [`2026-06-16-phaseN01-audit.md`](docs/audit/2026-06-16-phaseN01-audit.md) |
| 2026-06-19 | 3.3.0   | **1 CRITICAL** → fixed + regression-tested — asset-loading parsers (untrusted input) | [`2026-06-19-asset-loading-audit.md`](docs/audit/2026-06-19-asset-loading-audit.md) |
| 2026-06-20 | 3.4.3   | 0 CRITICAL — render-graph aliasing planner | [`2026-06-20-audit.md`](docs/audit/2026-06-20-audit.md) |
| 2026-07-01 | 4.0.0   | 0 CRITICAL / 0 HIGH — NVIDIA native backend | [`2026-07-01-audit.md`](docs/audit/2026-07-01-audit.md) |
| 2026-07-02 | 4.0.1   | 0 CRITICAL / 0 HIGH / 0 MED — AMD-wgpu deprecation diff | [`2026-07-02-audit.md`](docs/audit/2026-07-02-audit.md) |

2.3.0 was the last audit-gated *stdlib-candidate* release; from 2.4.0
onward the `CLAUDE.md` Security Hardening checklist is the rolling
internal gate, and a full audit pass runs at each significant arc
close rather than at every patch. Every patch that fixes a latent bug
lands with a CPU regression assertion in the matching
`tests/tcyr/<domain>.tcyr` suite so the bug can't re-enter.

**Since 4.0.1 (2026-07-02)** the 4.0.x line has been maintenance and
consumer-facing work — NVIDIA multi-BO (4.0.7), toolchain/dep currency
(4.0.3, 4.0.6, 4.0.8, 4.0.9, 4.0.10) — with no new untrusted-input
surface, so no new full audit has been filed. The next one is due with
the v4.0.11 native SPIR-V→GFX9 repairs, which do touch a parser
consuming consumer-supplied SPIR-V (see
[`docs/development/issues/`](docs/development/issues/)).

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
