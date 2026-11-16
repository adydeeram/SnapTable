# ADR 0004: Enforce write protection in the backend

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
Protected environments use DB-level read-only sessions and backend-issued, single-use confirmation tokens. The SQL classifier (sqlparser-rs) only shapes the UX.

## Rationale
UI-only guards can be bypassed, and parsers cannot see side effects inside functions (SPEC §4.5).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
