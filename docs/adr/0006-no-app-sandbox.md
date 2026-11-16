# ADR 0006: Ship without the App Sandbox

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
Distribute outside the Mac App Store with Hardened Runtime, Developer ID signing, and notarization, without the App Sandbox. Tauri capabilities provide in-app least privilege.

## Rationale
A sandbox would block reading ~/.ssh keys, opening arbitrary SQLite files, and connecting to arbitrary hosts without security-scoped bookmarks everywhere (SPEC §4.2).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
