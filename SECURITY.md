# Security Policy

SnapTable handles database credentials and production data, so we take security reports seriously.

## Reporting a vulnerability

**Please do not open a public issue.** Report privately through [GitHub Security Advisories](https://github.com/adydeeram/SnapTable/security/advisories/new).

Include affected versions, reproduction steps and the impact you see. We aim to acknowledge reports within 3 working days and to agree on a disclosure timeline with you.

## Supported versions

SnapTable is pre-1.0. Only the latest release and `main` receive security fixes.

## Security model, in short

See [docs/SPEC.md §4](docs/SPEC.md#4-security--sandboxing-model) for details.

- **Secrets** are stored only in the macOS Keychain. They are never written to the app database, logs or error messages, and are never sent to the UI layer.
- **All network I/O runs in the Rust backend.** The webview's Content Security Policy blocks network access, and it can only call an explicit allow-list of commands.
- **Production-tagged connections** open read-only database sessions, so the server rejects writes even if the UI is bypassed.
- **No telemetry.** The app makes no network requests except to the databases (and later, the APIs) you configure.

These areas are especially interesting to us:
- Ways to read a secret from the webview.
- Ways to bypass read-only enforcement.
- CSP or capability escapes.
- SQL injection in generated queries. Raw filters and editor SQL are user-authored by design.
