# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in mabda, please report it
responsibly through **GitHub Security Advisories**:

1. Go to the [Security tab](../../security/advisories) of this repository.
2. Click **"Report a vulnerability"**.
3. Fill in the details and submit.

**Do not open a public issue for security vulnerabilities.**

## Response Timeline

| Action | Target |
|---|---|
| Acknowledgement | Within **48 hours** |
| Initial assessment | Within **5 business days** |
| Fix for critical severity | Within **14 days** |
| Fix for high severity | Within **30 days** |
| Fix for moderate/low severity | Next scheduled release |

## Scope

This policy covers the `mabda` crate and its published API. Vulnerabilities
in dependencies (notably `wgpu`) should be reported to the respective upstream
projects and flagged here if they affect mabda users.

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.x | Yes |
| < 1.0 | No |

## Design Principles

- **No unwrap/panic** — all error paths return `Result` or safe defaults
- **No I/O** — GPU operations only, no network or filesystem access
- **Checked arithmetic** — integer overflow prevented with saturating/checked ops
- **Minimal dependencies** — wgpu, bytemuck, serde, thiserror, tracing
- **Supply chain** — `cargo audit` and `cargo deny` in CI on every push

## Disclosure

We follow coordinated disclosure. Once a fix is released, we will publish a
security advisory crediting the reporter (unless anonymity is requested).
