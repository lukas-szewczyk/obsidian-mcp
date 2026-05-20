#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export OBSIDIAN_VAULT_PATH="${OBSIDIAN_VAULT_PATH:-$ROOT/obsidian-vault}"

mkdir -p "$OBSIDIAN_VAULT_PATH/.obsidian"

echo "Vault: $OBSIDIAN_VAULT_PATH" >&2
echo "Inspector: http://localhost:6274" >&2

CLI_PROGRAM="${OBSIDIAN_CLI:-obsidian}"
if ! command -v "$CLI_PROGRAM" >/dev/null 2>&1; then
  echo "Warning: '$CLI_PROGRAM' is not available. Set OBSIDIAN_CLI if needed." >&2
elif CLI_CHECK_OUTPUT="$(cd "$OBSIDIAN_VAULT_PATH" && "$CLI_PROGRAM" vault info=name 2>&1 || true)" \
  && [[ "$CLI_CHECK_OUTPUT" == *"Vault not found"* ]]; then
  echo "Warning: Obsidian CLI does not recognize this vault yet." >&2
  echo "Open '$OBSIDIAN_VAULT_PATH' in Obsidian and enable Settings > General > Command line interface." >&2
fi

ENV_ARGS=(-e "OBSIDIAN_VAULT_PATH=$OBSIDIAN_VAULT_PATH")
if [[ -n "${OBSIDIAN_CLI:-}" ]]; then
  ENV_ARGS+=(-e "OBSIDIAN_CLI=$OBSIDIAN_CLI")
fi

exec npx -y @modelcontextprotocol/inspector \
  "${ENV_ARGS[@]}" \
  cargo run --manifest-path "$ROOT/Cargo.toml"
