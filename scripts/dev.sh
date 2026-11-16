#!/usr/bin/env bash
# One-command development run: installs JS deps if needed and starts the app with an
# isolated data directory and in-memory secrets (no Keychain prompts).
#   ./scripts/dev.sh           isolated dev data in ./.dev-data
#   ./scripts/dev.sh --real    use the real Keychain and ~/Library/Application Support
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/apps/desktop"

command -v cargo >/dev/null || { echo "Rust is required: https://rustup.rs"; exit 1; }
command -v node >/dev/null || { echo "Node.js 20+ is required: https://nodejs.org"; exit 1; }
[[ -d node_modules ]] || COREPACK_ENABLE_DOWNLOAD_PROMPT=0 corepack pnpm install

if [[ "${1:-}" == "--real" ]]; then
  exec corepack pnpm tauri dev
fi
mkdir -p "$ROOT/.dev-data"
SNAPTABLE_DATA_DIR="$ROOT/.dev-data" SNAPTABLE_SECRETS=memory exec corepack pnpm tauri dev
