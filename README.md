# WIP :) 

# obsidian-mcp

MCP server for an Obsidian vault, implemented in Rust with `rmcp` and the
Obsidian CLI.

The server uses stdio transport and communicates with a running Obsidian
instance through the `obsidian` command. By default it uses the project-local
vault at `obsidian-vault`. Set `OBSIDIAN_VAULT_PATH` only when you want to use a
different vault. Set `OBSIDIAN_CLI` only if the executable is not available as
`obsidian`.

Obsidian must be open and its CLI must be enabled.

## Quick start with MCP Inspector

Open `obsidian-vault` in Obsidian first, enable **Settings > General > Command
line interface**, then run:

```bash
./scripts/inspect.sh
```

The script starts the MCP Inspector with this Rust server over stdio. It follows
the Inspector usage from the MCP docs:

```bash
npx -y @modelcontextprotocol/inspector \
  -e OBSIDIAN_VAULT_PATH="$(pwd)/obsidian-vault" \
  cargo run --manifest-path ./Cargo.toml
```

The Inspector UI is served at `http://localhost:6274`.

## Tools

- `vault_info` - show configured vault path, Obsidian vault identity, and Markdown note count
- `list_notes` - list `.md` notes in the vault or a subdirectory
- `read_note` - read a note by relative path
- `write_note` - create a note or overwrite one when `overwrite=true`
- `append_note` - append text to a note
- `search_notes` - search notes using Obsidian search context

All note paths must be relative to the vault and must end with `.md`.
The server intentionally does not expose delete, rename, move, or a generic CLI
runner.

## Run

```bash
export OBSIDIAN_VAULT_PATH="./obsidian-vault"
# Optional:
# export OBSIDIAN_CLI="/path/to/obsidian"
cargo run
```

## MCP client config example

```json
{
  "mcpServers": {
    "obsidian": {
      "command": "cargo",
      "args": ["run", "--manifest-path", "/Users/lukasz/Desktop/obsidian-mcp/Cargo.toml"],
      "env": {
        "OBSIDIAN_VAULT_PATH": "/Users/lukasz/Desktop/obsidian-mcp/obsidian-vault"
      }
    }
  }
}
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

There is also an ignored smoke test for a live Obsidian CLI session:

```bash
OBSIDIAN_VAULT_PATH="/path/to/your/Obsidian/Vault" cargo test real_cli_smoke_vault_info -- --ignored
```
