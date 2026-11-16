## What & why

<!-- What does this change and why? Link issues with "Closes #123". -->

## How it was tested

<!-- Commands run, databases tested against, screenshots for UI changes. -->

## Checklist

- [ ] `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test --workspace` pass
- [ ] `pnpm typecheck` and `pnpm test` pass (in `apps/desktop`)
- [ ] IPC bindings regenerated if `crates/core` types changed
- [ ] New commands are listed in `build.rs` and `capabilities/main.json`
- [ ] No secrets can reach the webview, logs, errors or the metadata DB
- [ ] Docs / CHANGELOG updated if user-facing
