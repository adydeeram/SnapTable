# ADR 0005: Shared pool plus pinned sessions

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
Each DB connection has a small shared pool for browsing and introspection, plus a dedicated pinned session per SQL editor tab.

## Rationale
Users expect BEGIN, SET search_path, and temp tables to persist within one editor tab. Pinned sessions also make server-side cancellation possible via a known backend PID or connection ID (SPEC §1.4–1.5).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
