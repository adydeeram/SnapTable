# ADR 0008: Make auto-update opt-in

- **Status:** Accepted
- **Date:** 2026-11-16
- **Spec reference:** [SPEC.md §0](../SPEC.md)

## Decision
The Tauri updater is off by default. Homebrew users update through brew, and the cask sets auto_updates false.

## Rationale
This keeps the 100% offline, no-network-without-consent promise (SPEC §6.4).

## Consequences
See the related sections of SPEC.md. Any change to this decision needs a superseding ADR.
