# ADR 0007: One shared Environment entity

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
DB connection safety tags and API variable sets share one Environment entity (Local, Staging, Production, plus custom ones), and protection levels are attached to it.

## Rationale
Switching environments then scopes both modes at once, and the safety rules live in one place (SPEC §2.2, §4.5).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
