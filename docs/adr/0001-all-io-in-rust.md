# ADR 0001: All I/O happens in Rust

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
The webview never opens a socket. DB, SSH, and HTTP traffic all go through Rust (reqwest, sqlx, russh, redis).

## Rationale
No CORS problems for the API client. Secrets never reach JavaScript. The CSP can deny all network access from the webview (SPEC §4.1).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
