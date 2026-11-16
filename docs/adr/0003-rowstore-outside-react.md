# ADR 0003: Keep row data outside React state

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
Result rows live in a chunked columnar RowStore (a plain TypeScript class). Zustand holds only metadata. The grid reads cells imperatively and subscribes only to a version counter.

## Rationale
Putting 100k+ rows into React or Zustand state causes re-render storms and GC pressure (SPEC §2.1).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
