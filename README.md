WIP
# obsidian-mcp

MCP server for an Obsidian vault, implemented in Rust with `rmcp`.

The server uses stdio transport and reads/writes Markdown files directly in the
vault directory configured by `OBSIDIAN_VAULT_PATH`.

## Tools

- `vault_info` - show configured vault path and Markdown note count
- `list_notes` - list `.md` notes in the vault or a subdirectory
- `read_note` - read a note by relative path
- `write_note` - create or overwrite a note
- `append_note` - append text to a note
- `search_notes` - case-insensitive text search across notes

All note paths must be relative to the vault and must end with `.md`.

## Run

```bash
export OBSIDIAN_VAULT_PATH="/path/to/your/Obsidian/Vault"
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
        "OBSIDIAN_VAULT_PATH": "/path/to/your/Obsidian/Vault"
      }
    }
  }
}
```
