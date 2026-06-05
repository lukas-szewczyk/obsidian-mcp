# Changelog

All notable changes to this project will be documented in this file.

## Unreleased

### Added

- Work-system tools for structured note properties, overdue tasks, project tasks, and composed project status.
- Read-only previews for note changes and property writes.
- `plan_day` prompt plus project, properties, and overdue-task resource templates.
- Inspector fixture project with frontmatter and dated tasks.
- Stable read-only evaluation questions for the Work System fixture.

### Changed

- Package version now targets `0.3.0`.
- `draft_note_update`, `weekly_review`, and `project_review` prompts use the new work-system capabilities.
- GitHub workflows use `actions/checkout@v6`.

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
