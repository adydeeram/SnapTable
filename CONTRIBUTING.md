# Contributing to SnapTable

Thanks for helping! Read [docs/SPEC.md](docs/SPEC.md) and the [ADRs](docs/adr) before larger changes.

## Ground rules

1. **All I/O lives in Rust** (ADR 0001). The webview never opens sockets.
2. **Secrets never cross IPC outward** and never touch the metadata DB, logs or errors. Passwords go in through `SecretChange` and are stored in the Keychain.
3. **Bulk data streams over Channels** in columnar batches (ADR 0002). Row data stays out of React state (ADR 0003).
4. **Safety is enforced by the backend.** UI guards are a convenience, not the protection (ADR 0004).

## Setup

```sh
./scripts/dev.sh          # installs JS deps on first run, then launches the app
./scripts/demo-db.sh      # optional: PostgreSQL with sample data (Docker)
```

The repo also has a [`justfile`](justfile) (`just dev`, `just check`, `just db-up`, `just test-pg`). Plain commands work too:

```sh
cd apps/desktop && corepack pnpm install
SNAPTABLE_DATA_DIR=$PWD/../../.dev-data SNAPTABLE_SECRETS=memory corepack pnpm tauri dev
```

`SNAPTABLE_SECRETS=memory` avoids macOS Keychain prompts. Keychain item ACLs are tied to the code signature, and unsigned dev builds change on every rebuild. To test the real Keychain path, drop the variable and click **Always Allow** when prompted.

## Before opening a PR

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
SNAPTABLE_PG_PORT=55432 SNAPTABLE_PG_PASSWORD=postgres cargo test --workspace   # after ./scripts/demo-db.sh
cd apps/desktop && corepack pnpm typecheck && corepack pnpm test
```

- **IPC types:** if you change a type in `crates/core`, run `cargo test -p snaptable-core` to regenerate `ui/src/ipc/bindings`, and commit the result. CI fails on drift.
- **New commands:** add the command to `generate_handler!` (`src-tauri/src/lib.rs`), to `COMMANDS` in `src-tauri/build.rs`, and an `allow-…` entry in `capabilities/main.json`.
- **New shortcuts:** add them to `apps/desktop/shortcuts.json`. Both the native menu and the command palette read that file.
- Use Conventional Commit messages (`feat:`, `fix:`, `docs:`…).
- Sign off your commits (`git commit -s`, [DCO](https://developercertificate.org/)).
- Apply the `security-sensitive` label to any PR touching secrets, capabilities or protection levels.

## Adding a database engine

Implement it in `crates/drivers`. That means connect options (`pool.rs`), row conversion (`convert.rs`), cancellation (`tasks.rs`), introspection (`introspect.rs`), and a dialect in `crates/sqlgen`. Add an integration test file mirroring `tests/postgres.rs`.

## Releases (maintainers)

1. Update `CHANGELOG.md`, and bump `version` in `Cargo.toml` (the app version is read from it) and `apps/desktop/package.json`.
2. Tag and push the version, e.g. `git tag v0.1.0 && git push origin v0.1.0`. The [Release workflow](.github/workflows/release.yml) builds a universal `.dmg` into a draft GitHub Release.
3. **Signing (optional):** add these repository secrets to the `release` environment to sign and notarize:
   - `APPLE_CERTIFICATE` (base64 `.p12`) and `APPLE_CERTIFICATE_PASSWORD`
   - `APPLE_SIGNING_IDENTITY`
   - `APPLE_API_ISSUER`, `APPLE_API_KEY` and `APPLE_API_KEY_PATH`

   Without them, the build is unsigned.

## Security issues

Report them privately through GitHub Security Advisories, not in public issues.
