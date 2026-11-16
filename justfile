# Task runner (https://github.com/casey/just). Plain commands work too — see CONTRIBUTING.md.

desktop := "apps/desktop"

# Install JS dependencies
setup:
    cd {{desktop}} && corepack pnpm install

# Run the app with isolated data and an in-memory secret store
dev:
    ./scripts/dev.sh

# Run against the real Keychain and ~/Library/Application Support
dev-real:
    ./scripts/dev.sh --real

# All checks CI runs (PG integration tests need SNAPTABLE_PG_PORT)
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace
    cd {{desktop}} && corepack pnpm typecheck && corepack pnpm test

# Start a throwaway PostgreSQL with sample data (Docker); also used by integration tests
db-up:
    ./scripts/demo-db.sh

db-down:
    ./scripts/demo-db.sh stop

# Run PostgreSQL integration tests against `just db-up`
test-pg:
    SNAPTABLE_PG_PORT=55432 SNAPTABLE_PG_PASSWORD=postgres cargo test -p snaptable-drivers

# Regenerate TypeScript IPC bindings from Rust types
bindings:
    cargo test -p snaptable-core

# Release build (.app + .dmg); signing/notarization env vars are optional
build:
    cd {{desktop}} && corepack pnpm tauri build --target universal-apple-darwin
