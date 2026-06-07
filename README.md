# obsidian-mcp

`obsidian-mcp` is a local [Model Context Protocol](https://modelcontextprotocol.io/) server that lets Codex work with an Obsidian vault through the official Obsidian CLI.

The server runs over stdio, keeps Obsidian as the source of truth, and exposes focused tools, resources, and prompts for notes, Bases, daily notes, tasks, tags, backlinks, and project workflows.

The current source tree targets `v0.3.0`. The latest published GitHub Release is `v0.2.0`.

## Requirements

- macOS on Apple Silicon for the prebuilt `v0.2.0` binary
- Obsidian running with **Settings > General > Command line interface** enabled
- A local Obsidian vault
- Codex or another MCP client with stdio server support
- Rust stable when building from source

## Safety Model

- Every note path must be relative to the configured vault.
- Note operations accept Markdown files only.
- `create_note` refuses to replace an existing note.
- `replace_note` refuses to create a missing note.
- `preview_note_change` shows exact proposed note contents without writing.
- `preview_change_set` preflights every operation before writing and returns a deterministic approval token.
- `apply_change_set` refuses all writes when the approved token no longer matches current note state.
- `set_property` supports a read-only preview mode.
- Task and append operations are explicit and non-idempotent where appropriate.
- `create_base_item` requires an explicit Base path, named view, and item name.
- The server queries Bases but does not create or edit `.base` definitions.
- The server does not expose delete, move, rename, or generic CLI execution.
- MCP protocol output is written to stdout; diagnostics are written to stderr.

## Install

### GitHub Release

Download `obsidian-mcp-v0.2.0-aarch64-apple-darwin.tar.gz` and its SHA-256 file from the [v0.2.0 release](https://github.com/lukas-szewczyk/obsidian-mcp/releases/tag/v0.2.0).

```bash
shasum -a 256 -c obsidian-mcp-v0.2.0-aarch64-apple-darwin.tar.gz.sha256
tar -xzf obsidian-mcp-v0.2.0-aarch64-apple-darwin.tar.gz
```

The binary is unsigned and not notarized. If macOS quarantines it after download:

```bash
xattr -d com.apple.quarantine obsidian-mcp-v0.2.0-aarch64-apple-darwin/obsidian-mcp
```

### Build From Source

```bash
git clone https://github.com/lukas-szewczyk/obsidian-mcp.git
cd obsidian-mcp
cargo build --release --locked
```

The binary is written to `target/release/obsidian-mcp`.

## Codex Configuration

Add the server to `~/.codex/config.toml`:

```toml
[mcp_servers.obsidian]
command = "/absolute/path/to/obsidian-mcp"

[mcp_servers.obsidian.env]
OBSIDIAN_VAULT_PATH = "/absolute/path/to/your/vault"
OBSIDIAN_VAULT_NAME = "main"
OBSIDIAN_CLI = "/Applications/Obsidian.app/Contents/MacOS/obsidian"
OBSIDIAN_PROJECTS_PATH = "Projects"
```

Environment variables:

| Variable | Required | Default | Purpose |
| --- | --- | --- | --- |
| `OBSIDIAN_VAULT_PATH` | No | Project `obsidian-vault` fixture | Local vault directory |
| `OBSIDIAN_VAULT_NAME` | No | Most recently focused vault | Prefixes CLI calls with `vault=<name>` |
| `OBSIDIAN_CLI` | No | `obsidian` | Obsidian CLI executable |
| `OBSIDIAN_PROJECTS_PATH` | No | `Projects` | Vault-relative project notes directory |

Restart Codex after changing MCP configuration.

## MCP Interface

### Tools

| Tool | Behavior |
| --- | --- |
| `vault_info` | Return configured and Obsidian-reported vault metadata |
| `list_notes` | List Markdown notes, optionally under a directory |
| `read_note` | Read one Markdown note |
| `create_note` | Create a missing Markdown note |
| `replace_note` | Replace an existing Markdown note |
| `append_note` | Append text to a Markdown note |
| `search_notes` | Search notes with matching context |
| `list_tags` | List vault or note tags |
| `list_backlinks` | List backlinks to a note |
| `get_note_context` | Read aliases, outline, outgoing links, and backlinks for one note |
| `audit_vault` | Audit unresolved links, orphan notes, and dead ends |
| `list_bases` | List Obsidian Base files in the vault |
| `query_base` | Query a Base's default or named view as dynamic JSON |
| `create_base_item` | Create a note through an explicit Base and named view |
| `read_daily_note` | Read today's daily note |
| `append_daily_note` | Append text to today's daily note |
| `read_daily_notes` | Read an inclusive date range of daily notes |
| `list_tasks` | List tasks using typed target and status filters |
| `create_task` | Create a task in a note or today's daily note |
| `set_task_status` | Set a task to todo, done, or a custom status |
| `list_projects` | List Markdown notes under the projects directory |
| `list_properties` | List structured frontmatter properties for a note |
| `set_property` | Preview or set a typed frontmatter property |
| `list_overdue_tasks` | List incomplete tasks due before an explicit date |
| `list_tasks_by_project` | List tasks belonging to one project note |
| `get_project_status` | Read a project with properties, tasks, and backlinks |
| `preview_note_change` | Preview create, replace, or append results without writing |
| `preview_change_set` | Preview one to fifty note changes and return an approval token |
| `apply_change_set` | Apply an approved change set after a fresh full preflight |

Typed task values use tagged JSON:

```json
{"target":{"type":"note","path":"Todo.md"},"status":{"type":"todo"}}
```

Valid read targets are `vault`, `daily`, and `note`. Valid write targets are `daily` and `note`. Valid statuses are `todo`, `done`, and `custom`; a custom status must contain exactly one character.

Overdue task detection supports the common `📅 YYYY-MM-DD` and `due:: YYYY-MM-DD` task markers. `list_overdue_tasks` requires an explicit `as_of` date so results remain deterministic.

Property writes accept optional types: `text`, `list`, `number`, `checkbox`, `date`, and `datetime`.

```json
{"path":"Projects/Rust.md","name":"status","value":"paused","property_type":"text","preview":true}
```

`preview` defaults to `true`. Use the same request with `preview=false` only after reviewing the previous value returned by the preview.

Base queries return the dynamic JSON columns configured by the selected view. `create_base_item` does not support preview because Obsidian determines the new note's properties and location from the named view.

For multi-note writes, send the same ordered `changes` array to `preview_change_set` and `apply_change_set`, together with the accepted `preview_token`. A changed note or operation produces `conflict` and no writes. A write error after application starts produces `partial_failure`; earlier writes remain applied and later operations are skipped.

### Resources

| Resource | Content |
| --- | --- |
| `obsidian://vault/info` | Vault metadata |
| `obsidian://vault/audit` | Knowledge graph audit with unresolved links, orphan notes, and dead ends |
| `obsidian://bases/index` | Obsidian Base paths |
| `obsidian://notes/index` | Markdown note paths |
| `obsidian://tags/index` | Tags with counts |
| `obsidian://daily/today` | Today's daily note |
| `obsidian://tasks/open` | Open tasks with path and line references |
| `obsidian://projects/index` | Project note paths |
| `obsidian://note/{path}` | One Markdown note |
| `obsidian://backlinks/{path}` | Backlinks for one note |
| `obsidian://context/{path}` | One-hop metadata context for one note |
| `obsidian://base/{path}` | Dynamic JSON results from a Base's default view |
| `obsidian://daily/{date}` | One daily note by `YYYY-MM-DD` |
| `obsidian://tasks/overdue/{date}` | Incomplete tasks due before a date |
| `obsidian://project/{path}` | Project note with properties, tasks, and backlinks |
| `obsidian://properties/{path}` | Structured frontmatter properties for a note |

### Prompts

| Prompt | Purpose |
| --- | --- |
| `summarize_note` | Summarize a note and extract follow-up items |
| `search_and_synthesize` | Search the vault and synthesize relevant context |
| `draft_note_update` | Draft an approved create, replace, or append operation |
| `draft_change_set` | Draft and preview a guarded multi-note change set |
| `daily_review` | Review today's daily note |
| `plan_day` | Plan one explicit date from daily notes, overdue tasks, and projects |
| `tag_overview` | Summarize how a tag is used |
| `backlink_review` | Review incoming links to a note |
| `weekly_review` | Review a range of daily notes and open tasks |
| `project_review` | Review a project note, backlinks, and project tasks |
| `inbox_triage` | Triage open tasks and inbox-like notes |
| `vault_audit` | Review graph quality and recommend prioritized improvements |
| `base_review` | Review one Base view and propose next actions |

## Development

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

Run the MCP Inspector against the local fixture:

```bash
./scripts/inspect.sh
```

The stable read-only Work System evaluation set is in `evaluations/work-system-v0.3.0.xml`.

Run the ignored live smoke test against an open Obsidian vault:

```bash
OBSIDIAN_VAULT_PATH="/absolute/path/to/vault" \
OBSIDIAN_VAULT_NAME=main \
OBSIDIAN_CLI="/Applications/Obsidian.app/Contents/MacOS/obsidian" \
cargo test --locked real_cli_smoke_ -- --ignored --nocapture
```

## License

MIT
