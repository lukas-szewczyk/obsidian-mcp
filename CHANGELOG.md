# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

## 0.2.0 - 2026-06-05

First public release.

### Added

- Local stdio MCP server backed by the official Obsidian CLI.
- Tools, resources, and prompts for notes, daily notes, tasks, tags, backlinks, and projects.
- Typed task targets and task statuses.
- MCP protocol round-trip tests and GitHub Actions release automation.

### Changed

- Split the server into CLI, domain, model, tool, resource, and prompt modules.
- Replaced `write_note` with explicit `create_note` and `replace_note` tools.
- Renamed `append_task` to `create_task`.
- Renamed `complete_task` to `set_task_status`.
- Renamed `read_daily_range` to `read_daily_notes`.

### Security

- Vault paths remain validated and relative.
- Delete, move, rename, and generic CLI execution remain intentionally unavailable.
