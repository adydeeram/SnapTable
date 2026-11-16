# SnapTable — Technical Specification & Execution Plan

## Context

The goal is an open-source (MIT), local-first macOS app that does two jobs in one window: a database manager on par with TablePlus (PostgreSQL, MySQL/MariaDB, SQLite, Redis) and a lightweight API client, roughly a small Postman. This document is the product and technical specification; architecture decisions are recorded as ADRs in `docs/adr/`.

**Name.** **SnapTable** says what the app does and works as a Homebrew cask token (`snaptable`). (The working name "Lattice" was dropped: it is an established HR-software brand, a trademark and SEO risk.) Bundle ID: `dev.snaptable.app`.

---

## 0. Key Architectural Decisions (ADR candidates)

| # | Decision | Rationale |
|---|---|---|
| D1 | **All I/O happens in Rust.** The webview never opens a socket. DB, SSH, and HTTP all go through `reqwest`/`sqlx`/`russh`. | No CORS for the API client. Secrets never reach JS. The CSP can block all network access from the webview. |
| D2 | **Bulk data moves over Tauri v2 `Channel`s. Commands handle request/response. Global events are only for app-wide notifications.** | Channels are ordered and scoped to one invocation, and they are the fastest Tauri v2 streaming primitive. Global `emit` broadcasts to every listener and does not scale to row streaming. |
| D3 | **Row data lives outside React state** in a chunked columnar store. Zustand holds only metadata. | Putting 100k rows into Zustand/React triggers re-renders and GC pressure. The grid reads cells imperatively by `(row, col)`. |
| D4 | **Write protection is enforced in the backend.** The DB session itself is read-only. A SQL parser handles classification for UX only. | UI-only guards can be bypassed, and parsers miss side-effecting functions such as `SELECT nextval(...)` or DML inside a CTE. |
| D5 | **Two connection modes per DB connection:** a shared pool for browsing and introspection, and a *pinned session* per SQL editor tab. | Users expect `BEGIN`, `SET search_path`, and temp tables to persist inside one editor tab. |
| D6 | **No App Sandbox.** Use Hardened Runtime + Developer ID + notarization instead. | A sandbox would block reading `~/.ssh` keys, opening arbitrary SQLite files, and connecting to arbitrary hosts without security-scoped bookmarks everywhere. The Tauri capability layer covers in-app least privilege. |
| D7 | **Environments are one shared entity** used by DB connections (safety tags) and API requests (variable sets). | Switching to "Staging" can scope both modes at once, and the safety rules live in one place. |
| D8 | **Auto-update is opt-in** and off by default. Homebrew users update through brew. | This respects the "100% offline" promise. No network calls happen without consent. |

---

## 1. System Architecture & IPC Design

### 1.1 Process / layer diagram

```
┌──────────────────────── WKWebView (React/TS) ────────────────────────┐
│ Shell (titlebar, sidebar, tabs, palette)                             │
│ Feature modules: db-browser · grid · sql-editor · schema · redis ·   │
│                  api-client · settings                               │
│ State: Zustand stores (metadata)  +  RowStore (chunked, non-React)   │
│ IPC client: typed wrappers (generated from Rust via tauri-specta)    │
└───────────────▲─────────────────────────────┬────────────────────────┘
   invoke() → Result<T, AppError>            Channel<T> streams
   global events (low-volume)                (per invocation)
┌───────────────┴─────────────────────────────▼────────────────────────┐
│ src-tauri: thin command layer (validation, capability checks)        │
├──────────────────────────────────────────────────────────────────────┤
│ Core services (Rust workspace crates, tokio runtime):                │
│  ConnectionManager ── DriverRegistry ── {Postgres, MySQL, SQLite,    │
│        │                                  Redis} implementing traits │
│        ├── TunnelManager (russh local port-forward per connection)   │
│        ├── TlsConfigBuilder (rustls)                                 │
│  QueryEngine (exec, stream, cancel, statement split, classify)       │
│  EditEngine (staged changes → dialect SQL → transactional commit)    │
│  SchemaService (introspection cache, DDL generation)                 │
│  HttpEngine (reqwest, variable resolution, timing, body capture)     │
│  ExportService (stream to disk: CSV/JSON/SQL)                        │
│  TaskRegistry (task_id → CancellationToken + server-side cancel)     │
│  MetaStore (sqlx-sqlite app DB)   SecretStore (keyring → Keychain)   │
└──────────────────────────────────────────────────────────────────────┘
```

### 1.2 IPC contract rules
- **Typed bindings:** Rust command and event types are the source of truth. TS types are generated with `tauri-specta` (or `ts-rs`) and CI fails on drift.
- **Errors:** every command returns `Result<T, AppError>`. `AppError` is a tagged enum (`Connection`, `Auth`, `Query{code, position, detail}`, `Cancelled`, `Tunnel`, `Tls`, `Protected{reason}`, `Conflict`, `Internal`) serialized as `{kind, message, data}`. The UI switches on `kind`, never on message strings.
- **IDs:** the frontend generates a `task_id` (UUIDv7) for any cancellable operation, so it can cancel before the first response arrives.
- **Command groups:** `conn.*` (test, connect, disconnect, list), `schema.*`, `query.*` (execute, cancel, fetch_more), `edit.*` (preview, commit), `export.*`, `redis.*`, `http.*` (send, cancel), `store.*` (CRUD for saved entities), `secret.*` (write-only set/delete, plus `exists`).

### 1.3 Query execution & streaming flow
1. UI calls `query.execute({task_id, session_id, sql, params, max_rows}, channel)`.
2. Backend registers `task_id` in TaskRegistry, then splits statements with a dialect-aware splitter that handles `$$` dollar quotes, comments, string literals, and MySQL `DELIMITER`.
3. For each statement it sends `ResultStart{statement_idx, columns:[{name, type_name, type_oid, nullable, editable_hint}]}`, then `RowBatch{statement_idx, start_row, rows}` in batches of ~1,000 rows or every 50 ms (whichever comes first), then `ResultEnd{rows_affected, elapsed_ms, truncated}`. Errors arrive as `StatementError{...}`.
4. **Wire format:** batches are **columnar** (one array per column) to cut JSON key overhead. A Phase 1 spike benchmarks JSON against MessagePack over a raw-bytes channel. Switch to binary only if JSON misses the budget in §1.6.
5. **Value encoding:** a `Value` enum (`Null`, `Bool`, `Int`, `BigInt` serialized as a **string** whenever it exceeds 2^53, `Float`, `Decimal` as string, `Text`, `Bytes{len, preview_hex}`, `Json`, `Timestamp` as ISO plus the original text, `Array`, `Unknown{text}`).
   - Unknown or extension types (PostGIS, custom enums, ranges) fall back to text. If decoding fails, the browse query is re-issued with an explicit `::text` cast for those columns.
   - Large values (>64 KB text, any blob) send a truncated preview. The full value is fetched lazily with `query.fetch_cell`.
6. **Backpressure:** the result cap defaults to 100k rows (configurable). Past the cap, the UI shows "Load more" (`query.fetch_more` on a held cursor or session) or "Export full result" (streams straight to disk in Rust and never crosses IPC).
7. **Table browsing** (as opposed to ad-hoc SQL) is paginated server-side: `LIMIT/OFFSET` with a stable `ORDER BY` on the PK, using keyset pagination when a PK exists. Sorting and filtering are pushed down as SQL. The grid virtualizes over the page window.

### 1.4 Cancellation design (DB + HTTP)
- **TaskRegistry** maps `task_id → {CancellationToken, ServerCancel}`. Every executing future runs inside `tokio::select!` against its token.
- **Server-side cancel**, so the query actually stops on the server and not only in our client:
  - **PostgreSQL:** capture `pg_backend_pid()` when a session is checked out. On cancel, issue `pg_cancel_backend(pid)` over a separate pooled connection. The backend reports `Cancelled`, not an error.
  - **MySQL/MariaDB:** capture `CONNECTION_ID()`, then run `KILL QUERY <id>` over a side connection.
  - **SQLite:** drop the stream. sqlx steps rows on a worker thread, so this stops at the next row boundary. *Known gap:* one long `step()` (a large aggregate) cannot be interrupted. Mitigation: if needed, move the SQLite driver to `rusqlite` with `InterruptHandle` (tracked as a Phase 2 risk).
  - **Redis:** commands are short. Long scans run as SCAN-cursor loops that check the token on each iteration.
  - **HTTP:** drop the reqwest future, which closes the connection. Partial bodies are discarded.
- **Cancellation inside a pinned session with an open user transaction:** the transaction stays open and the UI shows a "Transaction open" badge. Cancelling does not roll back user transactions implicitly.
- **Timeouts:** per-connection statement timeout (PG `statement_timeout`, MySQL `max_execution_time` for SELECT) and per-request HTTP timeout. Both are optional.

### 1.5 Connection lifecycle
`Disconnected → Connecting (tunnel up → TLS → auth) → Connected → Degraded (keepalive failed) → Reconnecting → Connected | Failed`
- Lifecycle changes are emitted as a global event `conn://state` with `{connection_id, state, error?}`.
- Pool size: browse pool of 2–4 connections, plus one pinned session per open editor tab, opened lazily.
- On sleep/wake or network change: probe connections with `SELECT 1`. A dead SSH tunnel is rebuilt transparently. Pinned sessions holding an open transaction are reported as lost and never silently replaced.

### 1.6 Performance budgets (CI-tracked where possible)
- 100k rows × 20 columns: first rows painted in under 300 ms after the first batch, full stream ingested in under 3 s on M1 with a local PG, scrolling at 60 fps, heap growth under 250 MB.
- Cold start to interactive: under 1.5 s. Idle RAM: under 150 MB.
- Command palette opens in under 50 ms. Autocomplete suggestions arrive in under 100 ms on a schema with 5k tables (column lists load lazily).

---

## 2. State Management & Local Persistence

### 2.1 Frontend state split
| Store | Contents | Persistence |
|---|---|---|
| `useWorkspaceStore` (Zustand) | mode, open tabs, active tab per mode, layout sizes | restored from MetaStore `workspace_state` |
| `useConnectionStore` | connection list, live states, active DB/schema per connection | MetaStore (definitions); live state is in-memory |
| `useSchemaStore` | per-connection schema cache (tables, columns, indexes, FKs) | in-memory, invalidated after DDL or manual refresh |
| `useEditStore` | per-grid-tab staged change sets | in-memory; a draft snapshot is persisted so a crash does not lose edits |
| `useApiStore` | collections tree, open request drafts, active environment | MetaStore |
| **RowStore** (plain TS class, not Zustand) | columnar chunks per result, version counter | in-memory only |
| UI-only state (popovers, focus) | component-local | none |

The grid subscribes to RowStore's version counter. It does not subscribe to row arrays.

### 2.2 Metadata DB (app-owned SQLite)
- **Location:** `~/Library/Application Support/dev.snaptable.app/snaptable.db`, in WAL mode, with migrations embedded via `sqlx::migrate!`. Migrations are forward-only. The DB is backed up to `snaptable.db.bak-<version>` before each migration.
- **Hard rule:** this DB never stores a secret. Secret columns hold only a `secret_ref` flag, and the actual Keychain key is derived from it (§2.3).

| Table | Key columns |
|---|---|
| `schema_migrations` | managed by sqlx |
| `environments` | id, name, color, `protection_level` (none · confirm_writes · read_only), sort_order, is_builtin |
| `connection_groups` | id, name, parent_id, sort_order |
| `connections` | id, group_id, name, engine (postgres·mysql·mariadb·sqlite·redis), environment_id, host, port, database, username, `has_password`, sqlite_path, redis_db_index, options_json (search_path, statement_timeout, charset, read_only_override…), created_at, updated_at, last_connected_at |
| `connection_ssh` | connection_id (PK/FK), host, port, username, auth_method (password·key·agent), key_path, `has_password`, `has_passphrase`, known_host_fingerprint, keepalive_secs |
| `connection_tls` | connection_id, mode (disable·prefer·require·verify_ca·verify_full), ca_cert_path, client_cert_path, client_key_path, `has_client_key_passphrase` |
| `saved_queries` | id, connection_id (nullable), name, sql, folder, updated_at |
| `query_history` | id, connection_id, database, sql, status, rows_affected, elapsed_ms, executed_at. Has an **FTS5** index on sql. Capped at 10k rows per connection and pruned on startup. |
| `workspace_state` | key, json (open tabs, layout, last mode) |
| `api_collections` | id, name, description, sort_order |
| `api_folders` | id, collection_id, parent_id, name, sort_order |
| `api_requests` | id, collection_id, folder_id, name, method, url_raw, params_json (ordered and with duplicates; each row has enabled, key, value, description), headers_json, auth_type, auth_config_json (non-secret parts only), body_type, body_json, settings_json (timeout, follow_redirects, verify_tls), sort_order, updated_at |
| `api_variables` | id, environment_id (null = global), key, value (non-secret), `is_secret`, enabled |
| `api_history` | id, request_id (nullable), method, resolved_url (**secret values redacted**), status, elapsed_ms, size_bytes, executed_at. Response bodies are not stored by default. |
| `app_settings` | key, value_json |

**Privacy setting:** "Don't record history for Production connections". It is on by default for environments with `protection_level != none`.

### 2.3 Keychain secret mapping
- Uses the `keyring` crate with the `apple-native` backend: generic password items with **service** `dev.snaptable.app` and an **account** string derived like this:

| Secret | Keychain account |
|---|---|
| DB password | `conn:{connection_id}:db_password` |
| SSH password | `conn:{connection_id}:ssh_password` |
| SSH key passphrase | `conn:{connection_id}:ssh_key_passphrase` |
| TLS client key passphrase | `conn:{connection_id}:tls_key_passphrase` |
| Redis ACL password | `conn:{connection_id}:db_password` (same slot) |
| Secret API variable | `envvar:{variable_id}` |
| Request auth secret (bearer token, basic password, API key value) | `req:{request_id}:auth` |

- **Write-only API to the frontend:** `secret.set(ref, value)`, `secret.delete(ref)`, `secret.exists(ref)`. No command returns a secret to the webview. A "reveal password" action is a deliberate exception: it goes through a separate command gated by a native confirmation step and is logged.
- In Rust memory, secrets are wrapped in `secrecy::SecretString` (zeroized on drop). They are read from the Keychain only at connect or send time.
- **Lifecycle:**
  - Deleting a connection, request, or variable deletes its Keychain items in the same operation. On startup, an orphan sweep removes items with no matching metadata row.
  - Duplicating a connection copies its secrets to the new IDs.
  - Exports (§6) never include secrets unless the user opts into an encrypted bundle (Phase 4, passphrase-protected).
- **Edge case:** Keychain ACLs are bound to the code signature. Unsigned or ad-hoc dev builds trigger repeated "allow access" prompts. Mitigations: sign dev builds with a stable identity (§6), and document `Always Allow`.
- **Edge case:** the Keychain can be locked or access denied. The backend surfaces `AppError::Auth{keychain_denied}`, and the UI offers to enter the password once for this session without saving it.

---

## 3. UI/UX & Keyboard-First Navigation

### 3.1 Window layout
```
┌─ ● ● ●  [DB | API]   conn: prod-db ▾  env: ◉ Production ▾   ⌘K ───────┐ ← overlay titlebar
├═════════════ PRODUCTION · writes require confirmation ═════════════════┤ ← env banner (color = env)
│ Sidebar        │ Tab strip: [users] [orders*] [Query 1 ●] [+]           │
│ ─ Connections  │────────────────────────────────────────────────────────│
│   ▾ Staging    │  Toolbar: filter · sort · columns · refresh · ⟳ page    │
│     ▾ api-db   │  ┌──────────── Virtualized grid / Editor ────────────┐ │
│       ▾ public │  │                                                    │ │
│         users  │  └────────────────────────────────────────────────────┘ │
│   ▾ Production │  Bottom panel: Results 1 | Results 2 | Messages | SQL   │
│ ─ Saved queries│  Staged: 3 updates · 1 insert · 1 delete  [Preview][⌘S]│
│ ─ History      │                                     Right: Row inspector│
└────────────────┴────────────────────────────────────────────────────────┘
```
- **Sidebar, DB mode:** Environment → Connection → Database → Schema → {Tables, Views, Materialized Views, Functions, Sequences}. Redis connections show DB index → key namespaces, split on `:`, with SCAN-based lazy loading.
- **Sidebar, API mode:** Collections → Folders → Requests (method badge), plus History and Environments.
- **Environment color** (green/amber/red, plus custom) appears in three places: the sidebar connection dot, the tab underline, and the full-width banner when the active tab belongs to a protected environment.
- **Mode switching** (`⌘1`/`⌘2`) swaps sidebar content and the visible tab group. Each mode keeps its own tabs and active tab. A **peek drawer** (`⌘⇧D` in API mode) opens a compact table/query view of a chosen connection without leaving API mode, which serves the "hit an endpoint, check the row" workflow. A full split view is planned for Phase 4.

### 3.2 Tab lifecycle
- **Kinds:** `table-data`, `table-structure`, `sql-editor`, `redis-key`, `redis-console`, `api-request`, `settings`.
- **Preview tabs:** a single click opens an italic preview tab that the next single click replaces. Editing, double-clicking, or running a query pins the tab.
- **Identity and dedupe:** a tab's identity is `(kind, connection_id, db, schema, object)`, so opening the same table focuses the existing tab.
- **Dirty state:** shown as a dot for unsaved SQL, staged edits, or an unsaved request draft. Closing a dirty tab prompts Save / Discard / Cancel. Closing a tab with an **open transaction** prompts Commit / Rollback.
- **Restore on relaunch:** tab descriptors are restored, editor contents are restored from drafts, and connections reconnect **lazily** when a tab is focused (never automatically for Production). Result sets are not restored.
- **Disconnected connection:** its tabs stay open with an inline "Reconnect" state.

### 3.3 Keyboard map
Shortcuts are defined once in a **shortcut registry**. They appear in the native macOS menu bar (Tauri menu accelerators, which makes them discoverable) and dispatch commands to the frontend.

| Shortcut | Action |
|---|---|
| `⌘K` | Command palette. Searches every command, connection, table, saved query, and request, with fuzzy matching and recents first. |
| `⌘P` | Quick open: jump to a table or request in the current mode |
| `⌘1` / `⌘2` | DB mode / API mode |
| `⌘T` / `⌘W` / `⌘⇧T` | New tab (editor or request, depending on mode) / close / reopen closed |
| `⌘⇧[` `⌘⇧]` / `⌃Tab` | Previous / next tab |
| `⌘↵` | Run the statement under the cursor or the selection (SQL), or send the request (API) |
| `⌘⇧↵` | Run all statements |
| `⌘.` / `Esc` (while running) | Cancel the in-flight query or request |
| `⌘S` | Commit staged edits (grid) / save the query or request |
| `⌘Z` / `⌘⇧Z` | Undo / redo a staged edit |
| `⌘⇧P` | Preview staged changes as SQL |
| `⌘I` / `⌘⌫` / `⌘D` | Insert row / mark row for deletion / duplicate row |
| `⌘F` | Filter the grid / search the response body |
| `⌘R` | Refresh the data or schema |
| `⌘L` | Focus the URL bar (API) |
| `⌘E` | Environment switcher |
| `⌘N` | New connection / new request |
| `⌘,` | Settings |
| `↵` / `Tab` / arrows | Edit cell / move between cells |
| `⌥Space` | Open the full cell value in the inspector |

**Edge cases:**
- Monaco binds `⌘K` as a chord prefix and `⌘↵` to "insert line below". The editor must unbind or override those keys so the app-level bindings win.
- Shortcuts must not fire while an IME composition is active.
- Every action must be keyboard-reachable. Accessibility target: VoiceOver labels on the grid (ARIA grid roles) and on the sidebar tree.

### 3.4 Data grid behaviour
- **Rendering:** 2-D virtualization (rows and columns) with TanStack Virtual, fixed row height, and absolutely positioned cells. A single floating editor overlays the active cell.
- **Column widths:** auto-sized from the header plus the first 200 rows, user-resizable, and persisted per table.
- **Cell display:** NULL renders as a distinct italic `NULL` chip, separate from an empty string. JSON gets a collapsed preview. Bytes show `0x…` and their length.
- **Staged edit overlay:**
  - Modified cells are tinted amber, inserted rows green, and rows marked for deletion red with a strikethrough.
  - The change set is keyed by primary-key values plus the original values.
- **Edit safety:**
  - Tables without a PK or unique not-null key are **read-only**, with an explanation. Clients that edit by `ctid` or LIMIT 1 are a known footgun, so this app does neither.
  - Views and query results that join tables are read-only. A single-table SELECT that includes the PK is editable.
- **Commit** (`⌘S`) runs one transaction. Each UPDATE or DELETE uses `WHERE pk = $n` (optimistic mode can also compare the original values of edited columns). If any statement affects `rows != 1`, the whole transaction rolls back and the UI shows a **conflict** naming the row. All values are bound parameters. The preview pane renders an inlined, human-readable form, labelled as a display rendering.
- **Type input:** the user types text and the backend casts per dialect (`$1::type` on PG). Explicit "Set NULL" and "Set DEFAULT" actions exist. Invalid input is flagged before commit where the type is known (int, bool, uuid, json, date).
- **MySQL caveat:** DDL causes an implicit commit, so structure changes cannot be wrapped atomically. The preview warns about this.

### 3.5 Schema inspector & SQL editor (UX notes)
- **Structure tab:** sections for Columns, Indexes, Foreign Keys, Constraints, Triggers (view-only), and DDL. Every change is staged and previewed as `ALTER` statements before it runs.
  - **SQLite:** limited `ALTER` support is handled with the documented table-rebuild procedure (create new table, copy, drop, rename), run inside a transaction with `foreign_keys` checks.
- **Autocomplete:** a Monaco completion provider over the schema cache. Keywords come first, then tables (scoped to `search_path` or the current DB), then columns resolved through FROM/JOIN aliases using a lightweight tokenizer. Column lists load lazily for very large schemas.
- **Multiple result sets** appear as sub-tabs. Messages (NOTICE, warnings, rows affected, timing) go to a Messages tab.
- **Export (CSV, JSON, SQL INSERTs):** works on the current result, or reruns the query streaming straight to a file the user picks in a save dialog. Progress and cancel use the same task model.

### 3.6 Redis UX (scope)
- Key browser using `SCAN MATCH` with a page size of 500. The app never calls `KEYS`.
- Type-aware viewers and editors for string (with a JSON pretty-print toggle), hash, list, set, zset, and stream (read-only at first).
- TTL display and edit, key rename and delete, DB index switcher, and a console tab with command history.
- Cluster and Sentinel support are out of scope until after 1.0.

### 3.7 API client UX
- **URL bar:** method dropdown, URL with `{{var}}` highlighting (green when resolved, red when unresolved), and a Send/Cancel button.
- **Request tabs:** Params · Headers · Auth · Body · Settings.
  - **Params ⇄ URL two-way sync:** the URL is parsed tolerantly, keeping `{{vars}}` inside the query intact. Row order, duplicate keys, and disabled rows are preserved. Disabled rows are omitted from the URL. Encoding is preserved as typed, with no double-encoding. Edits on either side re-serialize the other side without reformatting it.
  - **Auth:** Bearer, Basic, API key (header or query). Secret parts go to the Keychain. Generated headers are shown read-only in Headers.
  - **Body:** JSON (Monaco, with lint), form-data (text and file fields; file paths come from a native picker), x-www-form-urlencoded, and raw (with a content-type picker).
- **Response pane:**
  - Status badge colored by class, latency in ms, size in KB (decoded and on-wire where available).
  - Tabs: Body · Headers · Cookies (read-only) · Raw.
  - Body viewer: pretty JSON/XML/HTML (read-only Monaco, folding, search), with images previewed inline.
  - Bodies over 5 MB are shown as virtualized plain text. Bodies over 50 MB are offered as save-to-file only.
- **Variable resolution happens in Rust at send time:**
  - Precedence: request-local, then active environment, then global.
  - Nested `{{a}}` → `{{b}}` resolves with cycle detection.
  - Dynamic variables: `{{$uuid}}`, `{{$timestamp}}`, `{{$isoTimestamp}}`, `{{$randomInt}}`.
  - The resolved request shown in the UI and history masks secret values.
- **Copy as cURL:** resolved, with secrets optionally masked. **cURL import** pastes into the URL bar. Postman v2.1 import is planned for Phase 4.

---

## 4. Security & Sandboxing Model

### 4.1 Tauri v2 capabilities & webview hardening
- **Capabilities:** one capability file for the `main` window that grants only:
  - the app's own command set (each custom command listed explicitly through the generated permission set)
  - `core:event` and `core:window` basics
  - `dialog` open/save
  - `clipboard-manager` write
  - **No** `shell`, `http`, or broad `fs` plugin. File reads and writes for exports and SQLite paths go through custom commands that only accept paths returned by the native dialog in the same session (an allowlisted path token), or paths already saved on a connection record.
- **CSP:** `default-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'` (required by Monaco); `worker-src blob:` (Monaco workers); no remote origins.
  - The webview cannot reach the network. All networking goes through Rust (D1).
  - Monaco is bundled locally. No CDN.
- **Release builds:** devtools disabled. Navigation outside the app origin is blocked. `window.open` is denied, and external links open through a custom command that validates `https:` URLs.
- **Rendering untrusted content:**
  - API responses rendered as HTML go to a **sandboxed preview** (`<iframe sandbox>` with no scripts) or show as source by default.
  - DB cell contents are always rendered as text and never as HTML.
- **Optional hardening:** the Tauri **isolation pattern**, evaluated in Phase 4.

### 4.2 macOS platform security
- Hardened Runtime, Developer ID Application signing, and notarization with a stapled ticket.
- Entitlements are kept to the minimum. Hardened Runtime has no network restrictions, so only add exceptions that prove necessary. Do **not** enable the App Sandbox (D6).
- The metadata DB file is chmod `0600`. Logs (`~/Library/Logs/dev.snaptable.app/`) never contain secrets, SQL parameter values from Production connections, or HTTP auth headers. A redaction layer in `tracing` enforces this.

### 4.3 SSH tunnels (russh)
- **Per connection:** bind `127.0.0.1:0` (an ephemeral loopback port that is never exposed on `0.0.0.0`). For each accepted local TCP connection, open a `direct-tcpip` channel to `db_host:db_port` and copy bidirectionally. The DB driver connects to the loopback port.
- **Authentication:**
  - Password.
  - Private key: OpenSSH and PEM formats, ed25519/ecdsa/rsa, encrypted keys using a passphrase from the Keychain.
  - **ssh-agent** via `SSH_AUTH_SOCK` (covers 1Password and Secretive agents). Agent support lands in Phase 2, alongside the others.
- **Host key verification:** check `~/.ssh/known_hosts` first, then the app's stored fingerprint. On first connect, show a **TOFU dialog** with the SHA256 fingerprint. A mismatch is a hard failure with a clear MITM warning. "Ignore host key" is never offered.
- **Keepalive:** every 30 s. When the tunnel dies, the connection state goes to Degraded and reconnects with backoff.
- **Out of scope for 1.0:** jump hosts (ProxyJump) and parsing `~/.ssh/config` aliases (beyond reading `IdentityFile`).

### 4.4 SSL/TLS
- **Driver TLS:** sqlx with `tls-rustls`. PG modes map to `disable / prefer / require / verify-ca / verify-full`, and MySQL to the equivalent `ssl-mode`.
  - Custom CA bundle and client cert/key (mTLS) are supported.
  - Trust roots: the system trust store via `rustls-platform-verifier`, so corporate CAs installed in the macOS Keychain work, plus any user-provided CA.
- **Redis:** `rediss://` with the same TLS builder.
- **HTTP:** reqwest with rustls and the platform verifier. A per-request "Disable TLS verification" toggle exists, shows a persistent warning badge on the response, and is **blocked for Production-tagged environments**.

### 4.5 Production write-protection (defense in depth)

| Layer | `confirm_writes` (default for Staging) | `read_only` (default for Production) |
|---|---|---|
| **UI** | env banner, colored tab, confirmation sheet that shows SQL + target + affected-row estimate | grid editing disabled, edit actions hidden, banner says READ-ONLY |
| **Statement classifier** (`sqlparser-rs`, per dialect) | flags DML, DDL, TRUNCATE, GRANT, CALL; flags **UPDATE/DELETE without WHERE** with an extra typed confirmation (type the table name) | classifies the statement; the UI blocks it before sending |
| **Backend token gate** | write commands require a single-use `confirmation_token` issued by the backend when the user confirms the exact SQL hash; it is rejected if the SQL changed | n/a |
| **DB session** | normal session | PG: `SET SESSION CHARACTERISTICS AS TRANSACTION READ ONLY` + `default_transaction_read_only=on` · MySQL: `SET SESSION TRANSACTION READ ONLY` · SQLite: open with `mode=ro` · Redis: command allowlist (read commands only), enforced in the backend |
| **Override** | n/a | "Unlock writes for 10 minutes" needs explicit confirmation, shows a countdown in the banner, and logs to history |

The classifier is advisory because it cannot see side effects inside functions. The **DB-level read-only session is the guarantee.** Global safeguards apply in every environment: "Rows affected" previews for staged edits, and an optional setting to warn on any UPDATE/DELETE without WHERE.

---

## 5. Four-Phase Execution Roadmap

The estimates assume 1–2 core developers. Each phase ends with a tagged pre-release.

### Phase 1 — Foundations & Read-Only Explorer (≈6–8 weeks)
**Scope:**
- Repo scaffold and CI. Tauri v2 shell with an overlay titlebar, sidebar, tabs, and native menu plus the shortcut registry.
- MetaStore with migrations, and SecretStore (Keychain).
- Connection CRUD and Test Connection for **PostgreSQL + SQLite**. No SSH/TLS yet beyond TLS `prefer/require`.
- Schema tree introspection and read-only table browsing with server-side pagination, sort, and filter.
- Streaming grid (Channel + RowStore + 2-D virtualization) and the IPC spike that decides JSON vs. binary.
- SQL editor (Monaco): run the statement under the cursor, run all, cancel (PG `pg_cancel_backend`), multiple result sets, Messages tab, pinned sessions.
- Query history (FTS search).
- Environment entity with colors and banners (visual only).

**Definition of Done:**
- [ ] 100k-row × 20-col PG query meets the §1.6 budgets on an M1 and an Intel Mac (recorded benchmark in `docs/perf/`).
- [ ] Cancelling a `SELECT pg_sleep(60)` frees the server backend within 1 s (verified in `pg_stat_activity`).
- [ ] No secret appears in the metadata DB, logs, or IPC payloads. Verified by an automated test that greps the DB, the log output, and IPC mocks for a sentinel password.
- [ ] Unsigned dev build and **signed** CI build (universal) both produced. ADRs D1–D8 are committed.
- [ ] Rust integration tests run against Dockerized PG (versions 13–17) in CI. Vitest covers RowStore and the tab store.

### Phase 2 — Full DB Parity: Editing, Engines, Security (≈8–10 weeks)
**Scope:**
- **Engines:** MySQL 8 / MariaDB 10.11+ and Redis 7 (key browser, type viewers, console, TTL).
- **Grid editing:** staged edits (update, insert, delete, duplicate), undo/redo, SQL preview, transactional `⌘S` commit, and conflict detection.
- **Structure tab:** view, then alter (columns, indexes, FKs), with dialect DDL generation including the SQLite rebuild procedure.
- **Connectivity:** SSH tunnels (password, key, agent) with TOFU known-hosts, and full TLS modes plus mTLS.
- **Production safety:** protection levels, classifier, confirmation tokens, read-only sessions, and unlock with a timer.
- **Editor:** schema-aware autocomplete and saved queries.
- **Export:** CSV, JSON, and SQL, streamed to disk.

**Definition of Done:**
- [ ] Edit round-trip tests run on PG, MySQL, MariaDB, and SQLite for these types: int, bigint > 2^53, numeric, text, bool, uuid, json/jsonb, timestamp/tz, bytea/blob, enum, array (PG), and NULL vs. empty.
- [ ] Concurrent-modification test: when another session changes the row, commit rolls back and shows a conflict.
- [ ] Read-only Production connection rejects an `UPDATE` **even when sent through raw IPC**, bypassing the UI (backend test).
- [ ] SSH tests against a Dockerized `openssh-server` pass for password, ed25519 key with passphrase, and RSA key. A host key mismatch hard-fails.
- [ ] TLS `verify-full` tests run against PG with a self-signed CA, including both pass and fail cases.
- [ ] Exporting 1M rows to CSV completes with flat memory (under 200 MB RSS growth).
- [ ] First **notarized** beta DMG is published to GitHub Releases (pre-release).

### Phase 3 — API Client (≈5–7 weeks)
**Scope:**
- HttpEngine (reqwest, rustls): all 7 methods, timeouts, redirects, cancel, and timing and size measurement.
- Request builder with two-way URL/params sync, headers, auth helpers, and all body types (multipart files included).
- Collections and folders with drag reordering, and API history.
- Shared environments: variables, secret variables in the Keychain, dynamic vars, and resolution in Rust.
- Response inspector: pretty or raw view, search, headers, cookies, size caps, and image preview.
- `⌘1`/`⌘2` mode switching with per-mode tab groups, and the DB **peek drawer** from API mode.
- Copy as cURL / import cURL.

**Definition of Done:**
- [ ] Params ⇄ URL sync property tests pass (round-trips for duplicates, empty values, encoded chars, `{{vars}}`, fragments).
- [ ] Secret variables never appear in the webview, history, logs, or exported cURL (unless the user explicitly chooses "unmask").
- [ ] A 50 MB response does not freeze the UI and the save-to-file path works. Cancel mid-download releases the socket.
- [ ] Integration tests run against a local test HTTP server (httpbin-like fixture) covering every body type and auth type.
- [ ] Switching the environment updates the resolved variables and the safety banner in both modes.

### Phase 4 — Polish, Performance, Ecosystem & 1.0 (≈5–6 weeks)
**Scope:**
- Full command palette coverage, quick open, and a configurable keymap.
- Split view (API + DB side by side) and session/tab restore hardening.
- Performance pass covering large schemas, wide tables (500+ columns), and long sessions (memory leak soak test).
- Postman v2.1 collection import. Connection export/import as JSON without secrets, with an optional encrypted bundle.
- Optional auto-update (opt-in) via the Tauri updater with signed manifests.
- Accessibility pass (VoiceOver, contrast in light and dark), crash-safe drafts, and an in-app "Report issue" action that pre-fills a GitHub issue locally with no telemetry.
- Documentation site, Homebrew Cask publishing, and the 1.0 release.

**Definition of Done:**
- [ ] Every user-facing action is reachable from `⌘K`, and the keyboard-only walkthrough script passes.
- [ ] A 4-hour soak (repeated queries, tab churn) shows less than 10% memory growth.
- [ ] `brew install --cask snaptable` (tap) installs, launches, and passes Gatekeeper on a clean macOS VM (both arm64 and x86_64).
- [ ] Zero outbound network calls at idle with updates disabled, verified with Little Snitch/`nettop` during QA.
- [ ] Docs cover install, connections, SSH/TLS, safety model, shortcuts, and contributing. The security policy is published.
- [ ] 1.0 is tagged and the release is notarized, stapled, and published with checksums.

### Key risks & mitigations
| Risk | Mitigation |
|---|---|
| IPC throughput for wide or large results | Phase 1 spike; columnar batches; binary channel fallback; export bypasses IPC |
| sqlx dynamic decoding of exotic types | `Unknown` text fallback; re-query with `::text` cast; per-type test matrix |
| SQLite long-step cancellation | rusqlite `InterruptHandle` fallback (ADR if triggered) |
| Monaco bundle size and keybinding conflicts | Load lazily per editor tab; central keybinding override layer |
| Keychain prompts on dev builds | Stable signing identity for dev; documented in CONTRIBUTING |
| macOS e2e: `tauri-driver` WebDriver does not support macOS | UI e2e runs in a browser with `@tauri-apps/api/mocks` (Playwright). Backend is covered by Rust integration tests. A manual QA checklist runs per release on real hardware. |

---

## 6. Open-Source Repo & Release Strategy

### 6.1 Repository layout
```
snaptable/
├─ apps/desktop/
│  ├─ src-tauri/            # Tauri app: commands, menu, capabilities/, tauri.conf.json, entitlements.plist
│  └─ ui/                   # React + TS + Tailwind + Vite
│     └─ src/{app,shell,features/{connections,grid,sql-editor,schema,redis,api-client,palette,settings},stores,ipc(generated),lib}
├─ crates/                  # Cargo workspace (pure Rust, unit-testable without Tauri)
│  ├─ core/                 # Value, AppError, ids, traits (Driver, Session, Introspector)
│  ├─ drivers/{postgres,mysql,sqlite,redis}/
│  ├─ sqlgen/               # dialect quoting, staged-edit → SQL, DDL gen, statement splitter, classifier
│  ├─ tunnel/               # russh port forwarding, known_hosts
│  ├─ http-engine/          # reqwest, variable resolver, curl import/export
│  ├─ store/                # metadata DB + migrations
│  └─ secrets/              # keyring wrapper, redaction
├─ test/
│  ├─ docker-compose.yml    # pg13–17, mysql8, mariadb11, redis7, openssh-server, tls-pg, http fixture
│  └─ fixtures/
├─ docs/ (SPEC.md, adr/, perf/, user-guide/)
├─ packaging/homebrew/snaptable.rb.tmpl
├─ .github/ (workflows/, ISSUE_TEMPLATE/, PULL_REQUEST_TEMPLATE.md, CODEOWNERS, dependabot.yml)
├─ CONTRIBUTING.md · CODE_OF_CONDUCT.md · SECURITY.md · LICENSE (MIT) · README.md · CHANGELOG.md
```
Tooling: `pnpm` for the UI, `just` (a justfile) for task orchestration (`just dev`, `just test`, `just db-up`), `cargo-deny` for the license and advisory allowlist (MIT-compatible only), `cargo-nextest`, `clippy -D warnings`, `rustfmt`, ESLint/Prettier, Vitest, and Playwright.

### 6.2 CONTRIBUTING.md outline
1. **Prerequisites:** macOS 12+, Xcode CLT, Rust stable (pinned via `rust-toolchain.toml`), Node LTS plus pnpm, Docker.
2. **Quick start:** `just setup`, `just db-up`, `just dev`.
3. **Architecture tour:** a link to SPEC.md and the ADRs, plus the rule that "all I/O lives in Rust, no secrets cross IPC".
4. **Adding a DB driver:** the trait checklist (connect, session, cancel, introspect, sqlgen dialect) plus the required test matrix.
5. **Conventions:**
   - Conventional Commits, which drive the changelog.
   - Branch naming.
   - A PR checklist: tests, regenerated IPC bindings, screenshots for UI changes, and a security-sensitive label for any change touching secrets, capabilities, or protection.
6. **Keychain prompt note:** dev builds and how to sign locally with a self-signed identity.
7. **Security:** vulnerabilities are reported privately via GitHub Security Advisories (SECURITY.md), never in public issues.
8. **Governance:** the DCO sign-off (`git commit -s`, lighter than a CLA). Issue labels: `good first issue`, `engine:*`, `area:*`.

### 6.3 GitHub Actions
| Workflow | Trigger | Jobs |
|---|---|---|
| `ci.yml` | PR, push to main | lint (clippy, rustfmt, eslint, tsc), Rust unit tests, **integration tests on `ubuntu-latest`** with docker-compose services (cheap; the drivers are platform-independent), UI tests (Vitest + Playwright with mocked IPC), IPC binding drift check, `cargo-deny`, macOS build smoke test (`macos-latest`, unsigned, arm64) |
| `release.yml` | tag `v*` | runs on `macos-latest`. Adds both `aarch64-apple-darwin` and `x86_64-apple-darwin` targets, then `tauri build --target universal-apple-darwin` via `tauri-action`. Steps listed below the table. |
| `nightly.yml` | cron | signed universal build from main, published as a rolling `nightly` pre-release |
| `homebrew.yml` | release published | renders the cask from the template with the version and sha256, then opens a PR on the `adydeeram/homebrew-tap` repo (or pushes directly with a scoped token) |

`release.yml` steps:
1. **Signing:** import the Developer ID cert from `APPLE_CERTIFICATE` (base64 p12) and `APPLE_CERTIFICATE_PASSWORD` into an ephemeral keychain. Set `APPLE_SIGNING_IDENTITY`.
2. **Notarization:** the App Store Connect API key (`APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`). The Tauri bundler notarizes and staples.
3. Verify with `spctl -a -vv` and `stapler validate`.
4. Upload the `.dmg`, a `.app.tar.gz` plus `.sig` (for the opt-in updater, signed with `TAURI_SIGNING_PRIVATE_KEY`), and `SHA256SUMS`.
5. Generate release notes from Conventional Commits and create a **draft** release for human review.

Secrets live in a protected `release` GitHub Environment that requires reviewer approval. Release jobs never run on fork PRs.

### 6.4 Homebrew Cask publishing
- **Stage 1 (Phase 2 betas to 1.0):** a dedicated tap `adydeeram/homebrew-tap`, installed with `brew install --cask adydeeram/tap/snaptable`. The cask declares:
  - `version`, `sha256`, and a `url` pointing at the GitHub Release DMG
  - `name`, `desc`, `homepage`
  - `depends_on macos: ">= :monterey"`
  - `app "SnapTable.app"`
  - `auto_updates false` (brew owns updates, D8)
  - `zap trash:` covering `~/Library/Application Support/dev.snaptable.app`, `~/Library/Caches/dev.snaptable.app`, `~/Library/Logs/dev.snaptable.app`, `~/Library/Preferences/dev.snaptable.app.plist`, and `~/Library/Saved Application State/dev.snaptable.app.savedState`
  - The docs note that **Keychain items are not removed by zap** and give the `security delete-generic-password -s dev.snaptable.app` loop to remove them.
- **Stage 2 (after 1.0):** submit to `homebrew/cask` once the project meets Homebrew's notability criteria. From then on, version bumps happen automatically via Homebrew's autobump, or via `brew bump-cask-pr` from `homebrew.yml`.
- **Verification in CI:** `brew audit --cask --strict --online` and `brew style` run on the rendered cask before the tap PR is opened.

---

## Open decisions (defaults chosen)
1. **Name:** SnapTable (default) or another.
2. **Minimum macOS:** 12 Monterey (default). This trades reach against WKWebView feature parity.
3. **SQLite driver:** sqlx (default, per the requested stack), with rusqlite as a documented fallback.
4. **Production default protection:** `read_only` (default) or `confirm_writes`.

## Plan verification
- Write this spec to `docs/SPEC.md` and split §0 into `docs/adr/0001…0008`.
- Validate that the Phase 1 IPC spike reaches the §1.6 budgets before building the grid on top of it. This is the single biggest architectural risk.
- Run each phase's Definition-of-Done checklist as the gating review before tagging that phase's pre-release.

---

## Implementation Notes (Phase 1 progress, 2026-11-16)

These are deviations from the text above, discovered while building:

- **Crates:** the drivers ship as a single `crates/drivers` crate with per-engine modules (`pool`, `convert`, `introspect`) rather than `drivers/{postgres,sqlite,…}`. They will split once more engines land.
- **`AppError` wire shape:** serialized as `{kind, message, …fields}` (flattened), not `{kind, message, data}`.
- **IPC bindings:** generated with `ts-rs` (not tauri-specta). Regenerate with `cargo test -p snaptable-core`.
- **Keychain:** `keyring` v4 split into `keyring-core` + `apple-native-keyring-store`. Those are used directly.
- **PostgreSQL value decoding:** statements run over the simple-query protocol, so every value arrives as text. This gives a lossless fallback for any type (enums, PostGIS, ranges) with no per-type decoders.
- **`freezePrototype` is disabled** in `tauri.conf.json`. Monaco assigns to built-in properties at load time and fails under a frozen `Object.prototype`. The CSP and command allow-list remain the main webview controls.
- **Read-only protection is enforced at the session level already in Phase 1**: PG `default_transaction_read_only=on`, SQLite `mode=ro`. The confirm-writes flow and the timed unlock are still Phase 2.
- **Not yet done from the Phase 1 scope:**
  - SQLite cancellation still stops at row boundaries only.
  - No "transaction open" badge yet. sqlx does not expose server transaction state for raw `BEGIN`.
  - Sidebar trees are not virtualized.
  - Signed CI builds are not set up yet.
