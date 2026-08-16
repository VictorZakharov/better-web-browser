# Security policy

## Supported versions

Breeze is an early browser-engine MVP. Only the current `main` branch receives security fixes; no
released version should yet be used for sensitive authenticated browsing.

## Reporting a vulnerability

Please use GitHub private vulnerability reporting for this repository. If that channel is
unavailable, email `neolisk@gmail.com`. Include a minimal reproducer, affected revision, expected
impact, and any crash output that does not contain private data.

Do not open a public issue for an undisclosed vulnerability. We will acknowledge a complete report,
coordinate validation and remediation privately, and credit reporters who want public attribution.

The documented input, process, and fuzzing boundaries are in
[`docs/security-and-fuzzing.md`](docs/security-and-fuzzing.md).
