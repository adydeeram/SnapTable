# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added — Phase 1 foundations

- **Connections**
  - PostgreSQL and SQLite connections, grouped by environment (Local / Staging / Production).
  - Passwords are stored in the macOS Keychain and are write-only from the UI.
  - Connection URL import and a "Test connection" button.
- **Safety:** Production-tagged connections open read-only database sessions (`default_transaction_read_only` on PostgreSQL, `mode=ro` on SQLite).
- **Schema browser** for schemas, tables, views and materialized views, with a filter.
- **Table browser**
  - Server-side paging, sorting and a raw `WHERE` filter.
  - Exact row counts and a row inspector.
- **Data grid**
  - Virtualized in both directions; results stream in columnar batches.
  - Lossless display of big integers, numerics, bytea and unknown types.
- **SQL editor (Monaco)**
  - Run the statement at the cursor, run all, or run the selection.
  - Multiple result sets with a Messages tab, and inline error markers.
  - Server-side query cancellation (`pg_cancel_backend`).
  - A pinned session per editor tab, so transactions, `SET` and temp tables persist between runs.
- **Query history** with full-text search. Protected environments are not recorded.
- **Keyboard-first UI**
  - Command palette (`⌘K`) and quick open (`⌘P`).
  - Tab lifecycle with preview tabs and restore on relaunch.
  - The native menu bar is built from a single shortcut registry.
- **Engineering**
  - Metadata store with migrations, plus generated TypeScript IPC bindings.
  - Least-privilege Tauri capabilities.
  - CI for lint, tests (including PostgreSQL integration tests) and the macOS build.
