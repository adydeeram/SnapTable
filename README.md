<div align="center">

<img src="apps/desktop/src-tauri/icons/128x128@2x.png" width="96" alt="SnapTable icon" />

# SnapTable

**A fast, local-first database manager and API client for macOS.**
Open source · no account · no telemetry · your credentials stay in the Keychain.

[![CI](https://github.com/adydeeram/SnapTable/actions/workflows/ci.yml/badge.svg)](https://github.com/adydeeram/SnapTable/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platform: macOS](https://img.shields.io/badge/platform-macOS%2012%2B-lightgrey)
![Status: alpha](https://img.shields.io/badge/status-alpha-orange)

<img src="docs/assets/screenshot.png" width="880" alt="SnapTable browsing a 150,000-row orders table on a read-only production connection" />

</div>

> [!NOTE]
> SnapTable is in **early development (Phase 1 of 4)**. Browsing and querying PostgreSQL and SQLite works today. Editing data, MySQL/MariaDB, Redis, SSH tunnels and the built-in API client are on the [roadmap](#roadmap).

## Features

| | |
|---|---|
| 🔌 **Connections** | PostgreSQL and SQLite, grouped by environment: Local, Staging and Production. Paste a `postgres://` URL to fill the form. |
| 🔐 **Keychain-only secrets** | Passwords live in the macOS Keychain, never on disk, in logs, or in the UI layer. |
| 🛡️ **Production safety** | Production connections open **read-only database sessions**, so the server itself rejects writes. A colored banner shows which environment you're in. |
| ⚡ **Fast grid** | Results stream in batches into a virtualized grid. 100k rows arrive in under half a second from a local Postgres. |
| 📝 **SQL editor** | Monaco with SQL highlighting. Run the statement at the cursor (`⌘↵`), everything (`⌘⇧↵`) or the selection. Results appear in tabs, errors are underlined inline. |
| ✋ **Real cancel** | `⌘.` / `Esc` stops the query **on the server** (`pg_cancel_backend`), not just in the UI. |
| 🧭 **Table browser** | Server-side paging, click-to-sort, `WHERE` filter, exact row counts and a row inspector with pretty-printed JSON. |
| ⌨️ **Keyboard-first** | Command palette (`⌘K`), quick open (`⌘P`), query history (`⌘Y`), tab restore, and a full native menu. |

## Install

### Download

Pre-built universal (Apple Silicon + Intel) `.dmg` files are published on the [Releases](https://github.com/adydeeram/SnapTable/releases) page as they are tagged.

Early builds are **unsigned**, so macOS blocks them the first time. Right-click `SnapTable.app` → **Open**, or run:

```sh
xattr -dr com.apple.quarantine /Applications/SnapTable.app
```

A Homebrew cask (`brew install --cask snaptable`) is planned once releases are signed.

### Build from source

You need macOS 12+, [Rust](https://rustup.rs) and [Node.js](https://nodejs.org) 20+. pnpm comes with Node through corepack.

```sh
git clone https://github.com/adydeeram/SnapTable.git
cd SnapTable
./scripts/dev.sh            # installs dependencies and launches the app in dev mode
```

`./scripts/dev.sh` keeps its data in `./.dev-data` and stores passwords in memory, so it never touches your real Keychain. Use `./scripts/dev.sh --real` for the real Keychain.

To build a release app instead:

```sh
cd apps/desktop && corepack pnpm install && corepack pnpm tauri build
# → target/release/bundle/macos/SnapTable.app and bundle/dmg/*.dmg
```

## Try it with sample data

With Docker running:

```sh
./scripts/demo-db.sh
```

This starts PostgreSQL with a sample `shop` database (customers, 150k orders, a view, and a second schema). In SnapTable press `⌘N`, paste `postgres://postgres:postgres@127.0.0.1:55432/shop`, and click **Save & Connect**.

Tag the connection as **Production** to see the read-only safety mode.

## Keyboard shortcuts

| Shortcut | Action | | Shortcut | Action |
|---|---|---|---|---|
| `⌘K` | Command palette | | `⌘↵` | Run statement at cursor / selection |
| `⌘P` | Quick open table | | `⌘⇧↵` | Run all |
| `⌘N` | New connection | | `⌘.` `Esc` | Cancel running query |
| `⌘T` | New query tab | | `⌘R` | Refresh |
| `⌘W` / `⌘⇧T` | Close / reopen tab | | `⌘F` | Filter rows |
| `⌘⇧[` `⌘⇧]` | Previous / next tab | | `⌘Y` | Query history |
| `⌘1` / `⌘2` | Database / API mode | | `⌘B` | Toggle sidebar |

The single source of truth for shortcuts is [`apps/desktop/shortcuts.json`](apps/desktop/shortcuts.json).

## Roadmap

The full plan, with exit criteria for each phase, is in [docs/SPEC.md](docs/SPEC.md#5-four-phase-execution-roadmap).

- [x] **Phase 1 (mostly done):** PostgreSQL and SQLite, schema browser, streaming grid, SQL editor with cancel, Keychain secrets, history, command palette
- [ ] **Phase 2:**
  - Inline editing with staged changes, a SQL preview and `⌘S` commit
  - MySQL/MariaDB and Redis
  - Structure editor
  - SSH tunnels and full TLS
  - Confirm-before-write flow
  - Autocomplete
  - Export to CSV, JSON and SQL
- [ ] **Phase 3:** built-in API client: request builder, `{{variables}}`, shared environments, response inspector
- [ ] **Phase 4:** polish, Postman import, opt-in auto-update, signed releases and a Homebrew cask, 1.0

## How it works

SnapTable is a [Tauri 2](https://tauri.app) app. A Rust backend does all networking and database work using `sqlx`, `tokio` and the Keychain. A React/TypeScript UI uses Monaco and TanStack Virtual. The UI can only call an explicit allow-list of commands and has no network access of its own.

```
apps/desktop/          Tauri app: src-tauri/ (command layer), ui/ (React), shortcuts.json
crates/core            shared types → generated TypeScript bindings
crates/sqlgen          statement splitter, quoting, generated queries
crates/drivers         connection pools, sessions, streaming executor, cancellation
crates/store           app metadata database (connections, history, workspace)
crates/secrets         macOS Keychain wrapper
docs/                  specification and architecture decision records
```

## Contributing

Contributions are very welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers setup, tests and conventions, and issues labelled [`good first issue`](https://github.com/adydeeram/SnapTable/labels/good%20first%20issue) are a good place to start.

Please report security issues privately; see [SECURITY.md](SECURITY.md).

## License

[MIT](LICENSE) © SnapTable contributors
