# Security Policy

## Reporting a vulnerability

Please report security vulnerabilities **privately** to
**security@axonos.org**. Do not open a public issue for a security report.

Include where relevant: affected version or commit, a description of the
issue, and a minimal reproduction. We aim to acknowledge a report within a few
days and to provide a remediation timeline after triage. Target window for a
coordinated fix is **90 days**, adjusted by severity and complexity.

## Scope

This repository is a **pre-clinical engineering artifact**, not a medical
device. The most security-relevant property it asserts is the application
boundary (`docs/PRIVACY_BOUNDARY.md`): raw signal types must not cross it
through this crate's API. Reports that demonstrate a way to defeat that
boundary through the crate's public surface are especially in scope.

Cryptographic authentication and consent enforcement are **out of scope here**
— they live in `axonos-consent` and the kernel. Please report those against
their own repositories.

## Supported versions

During pre-1.0 development, only the latest released minor version receives
security fixes.

---

The AxonOS Project · axonos.org · connect@axonos.org · security@axonos.org · github.com/AxonOS-org
