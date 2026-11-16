# ADR 0002: Stream bulk data over Tauri v2 Channels

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
Commands handle request/response. Per-invocation Channels carry bulk streams such as query rows, export progress, and HTTP bodies. Global events are used only for low-volume, app-wide notifications such as connection state.

## Rationale
Channels are ordered, scoped to one call, and the fastest Tauri v2 streaming primitive. Global emit broadcasts to every listener and does not scale to row streaming (SPEC §1.3).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
