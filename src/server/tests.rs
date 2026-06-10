use std::{
    collections::VecDeque,
    ffi::OsString,
    fs,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use rmcp::{
    ClientHandler, ServiceExt,
    model::{
        CallToolRequestParams, ClientInfo, GetPromptRequestParams, GetPromptResult,
        ReadResourceRequestParams, ReadResourceResult, ResourceContents,
    },
};

use super::resources::ObsidianResourceUri;
use super::*;
use crate::cli::CliFuture;

#[tokio::test]
async fn rejects_paths_that_escape_vault() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    assert!(server.read_note_content("../secret.md").await.is_err());
    assert!(
        server
            .create_note_content("/tmp/secret.md", "")
            .await
            .is_err()
    );
}

#[test]
fn classifies_cli_sentinels_errors_and_truncation() {
    assert_eq!(
        classify_cli_output("tags", CliOutput::success("No tags found.\n")).unwrap(),
        ""
    );
    assert_eq!(
        classify_cli_output("tasks", CliOutput::success("No tasks found.\n")).unwrap(),
        ""
    );
    assert!(matches!(
        classify_cli_output(
            "read",
            CliOutput::success("Error: File \"Missing.md\" not found.\n")
        ),
        Err(ObsidianMcpError::NoteNotFound(_))
    ));

    let mut stderr_error = CliOutput::success("");
    stderr_error.stderr = "Error: unexpected content=secret body\n".to_string();
    let error = classify_cli_output("read", stderr_error).unwrap_err();
    assert!(matches!(error, ObsidianMcpError::CliFailed(_)));
    assert!(!error.to_string().contains("secret"));

    let mut truncated = CliOutput::success("partial");
    truncated.stdout_truncated = true;
    assert!(matches!(
        classify_cli_output("read", truncated),
        Err(ObsidianMcpError::CliProtocol(_))
    ));
}

#[tokio::test]
async fn ambiguous_preflights_block_every_write_entry_point() {
    let vault = TestVault::new();

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(server.create_note_content("Create.md", "x").await.is_err());
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(
        server
            .replace_note_content("Replace.md", "x")
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(server.append_note_content("Append.md", "x").await.is_err());
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(
        server
            .set_property_data("Note.md", "status", "done", None, false)
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(
        server
            .create_task_data(
                &TaskWriteTarget::Note {
                    path: "Note.md".to_string()
                },
                "Task"
            )
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(
        server
            .set_task_status_data("Note.md", 1, &TaskStatus::Done)
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(server.append_daily_note_content("x", false).await.is_err());
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(
        server
            .create_task_data(&TaskWriteTarget::Daily, "Task")
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(
        server
            .create_base_item_data("Projects.base", "Active", "New", None)
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);

    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    assert!(
        server
            .apply_change_set_data(
                vec![ChangeSetOperation {
                    path: "New.md".to_string(),
                    mode: NoteChangeMode::Create,
                    content: "x".to_string(),
                }],
                &format!("sha256:{}", "a".repeat(64)),
            )
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);
}

#[tokio::test]
async fn daily_range_propagates_infrastructure_failures() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::results(vec![Err(timeout_error())]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    assert!(
        server
            .read_daily_notes_data("2026-06-01", "2026-06-02", None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn validates_and_caches_vault_identity_and_rejects_mismatches() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::outputs([format!(
        "name\ttest-vault\npath\t{}\n",
        vault.path().display()
    )]);
    let server = ObsidianMcp::with_validating_runner(
        vault.path(),
        Some("test-vault".to_string()),
        cli.clone(),
    )
    .unwrap();
    server.validate_vault().await.unwrap();
    server.validate_vault().await.unwrap();
    assert_eq!(cli.calls().len(), 1);

    let other = TestVault::new();
    let cli =
        FakeObsidianCli::outputs([format!("name\tother\npath\t{}\n", other.path().display())]);
    let server = ObsidianMcp::with_validating_runner(vault.path(), None, cli).unwrap();
    assert!(matches!(
        server.validate_vault().await,
        Err(ObsidianMcpError::VaultMismatch(_))
    ));

    let cli = FakeObsidianCli::outputs([format!(
        "name\twrong-name\npath\t{}\n",
        vault.path().display()
    )]);
    let server =
        ObsidianMcp::with_validating_runner(vault.path(), Some("test-vault".to_string()), cli)
            .unwrap();
    assert!(matches!(
        server.validate_vault().await,
        Err(ObsidianMcpError::VaultMismatch(_))
    ));
}

#[cfg(unix)]
#[tokio::test]
async fn accepts_symlink_equivalent_vault_paths() {
    use std::os::unix::fs::symlink;

    let vault = TestVault::new();
    let link = vault.path().with_extension("link");
    symlink(vault.path(), &link).unwrap();
    let cli = FakeObsidianCli::outputs([format!(
        "name\ttest-vault\npath\t{}\n",
        vault.path().display()
    )]);
    let server = ObsidianMcp::with_validating_runner(link.clone(), None, cli).unwrap();

    server.validate_vault().await.unwrap();
    fs::remove_file(link).unwrap();
}

#[test]
fn task_status_and_content_validate_workflow_inputs() {
    assert_eq!(
        validate_task_status("xx").unwrap_err().to_string(),
        "task status must be a single character"
    );
    assert_eq!(format_task_line("Call bank").unwrap(), "- [ ] Call bank");
    assert_eq!(
        format_task_line("- [ ] Already formatted").unwrap(),
        "- [ ] Already formatted"
    );
}

#[test]
fn tool_contract_exposes_v030_work_system_names() {
    let vault = TestVault::new();
    let server = ObsidianMcp::with_runner(vault.path(), FakeObsidianCli::default()).unwrap();
    let tools = server.tool_router.list_all();
    let names = tools
        .iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();
    assert_eq!(tools.len(), 29);
    assert!(
        tools
            .iter()
            .all(|tool| tool.output_schema.is_some() && tool.annotations.is_some())
    );

    for expected in [
        "create_note",
        "replace_note",
        "create_task",
        "set_task_status",
        "read_daily_notes",
        "list_properties",
        "set_property",
        "list_overdue_tasks",
        "list_tasks_by_project",
        "get_project_status",
        "preview_note_change",
        "preview_change_set",
        "apply_change_set",
        "get_note_context",
        "audit_vault",
        "list_bases",
        "query_base",
        "create_base_item",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }
    for removed in [
        "write_note",
        "append_task",
        "complete_task",
        "read_daily_range",
    ] {
        assert!(!names.iter().any(|name| name == removed), "found {removed}");
    }

    let preview = tools
        .iter()
        .find(|tool| tool.name == "preview_change_set")
        .unwrap()
        .annotations
        .as_ref()
        .unwrap();
    assert_eq!(preview.read_only_hint, Some(true));
    assert_eq!(preview.idempotent_hint, Some(true));
    assert_eq!(preview.open_world_hint, Some(false));

    let apply = tools
        .iter()
        .find(|tool| tool.name == "apply_change_set")
        .unwrap()
        .annotations
        .as_ref()
        .unwrap();
    assert_eq!(apply.read_only_hint, Some(false));
    assert_eq!(apply.destructive_hint, Some(true));
    assert_eq!(apply.idempotent_hint, Some(false));
    assert_eq!(apply.open_world_hint, Some(false));

    for destructive in ["set_task_status", "set_property"] {
        assert_eq!(
            tools
                .iter()
                .find(|tool| tool.name == destructive)
                .unwrap()
                .annotations
                .as_ref()
                .unwrap()
                .destructive_hint,
            Some(true)
        );
    }

    let task_schema = tools
        .iter()
        .find(|tool| tool.name == "set_task_status")
        .unwrap()
        .input_schema
        .clone();
    assert_eq!(
        task_schema["properties"]["line"]["minimum"],
        rmcp::serde_json::json!(1)
    );
    let change_set_schema = tools
        .iter()
        .find(|tool| tool.name == "preview_change_set")
        .unwrap()
        .input_schema
        .clone();
    assert_eq!(
        change_set_schema["properties"]["changes"]["maxItems"],
        rmcp::serde_json::json!(50)
    );
}

#[test]
fn tool_output_schema_properties_are_object_schemas() {
    // MCP clients reject a top-level `outputSchema.properties.*` entry whose
    // schema is a boolean (e.g. `serde_json::Value` deriving `true`). Every
    // property must be an object-form JSON Schema.
    let vault = TestVault::new();
    let server = ObsidianMcp::with_runner(vault.path(), FakeObsidianCli::default()).unwrap();
    for tool in server.tool_router.list_all() {
        let Some(schema) = tool.output_schema.as_ref() else {
            continue;
        };
        let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) else {
            continue;
        };
        for (name, prop) in properties {
            assert!(
                prop.is_object(),
                "tool `{}` output property `{name}` must be an object schema, got: {prop}",
                tool.name
            );
        }
    }
}

#[test]
fn parses_task_tsv_rows_with_references() {
    let tasks = parse_tasks_tsv(
        " \t- [ ] Review inbox\tTodo.md\t4\nx\t- [x] Ship change\tProjects/Rust.md\t12\n",
    )
    .unwrap();

    assert_eq!(
        tasks,
        vec![
            TaskItem {
                status: " ".to_string(),
                text: "- [ ] Review inbox".to_string(),
                path: "Todo.md".to_string(),
                line: 4,
            },
            TaskItem {
                status: "x".to_string(),
                text: "- [x] Ship change".to_string(),
                path: "Projects/Rust.md".to_string(),
                line: 12,
            },
        ]
    );
}

#[tokio::test]
async fn uses_cli_for_notes_workflow() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Err("missing"),
        Ok("created"),
        Ok("Projects/Rust.md"),
        Ok("appended"),
        Ok("Rust MCP\nSecond line\nObsidian vault"),
        Ok("Projects/Rust.md\n"),
        Ok("Projects/Rust.md:3: Obsidian vault\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    server
        .create_note_content("Projects/Rust.md", "Rust MCP\nSecond line")
        .await
        .unwrap();
    server
        .append_note_content("Projects/Rust.md", "\nObsidian vault")
        .await
        .unwrap();

    let content = server.read_note_content("Projects/Rust.md").await.unwrap();
    assert!(content.contains("Rust MCP"));
    assert!(content.contains("Obsidian vault"));

    let notes = server
        .list_note_paths(Some("Projects"), Some(10))
        .await
        .unwrap();
    assert_eq!(notes, vec!["Projects/Rust.md"]);

    let matches = server
        .search_note_contents("obsidian", Some("Projects"), Some(10))
        .await
        .unwrap();
    assert_eq!(matches, vec!["Projects/Rust.md:3: Obsidian vault"]);

    let calls = cli.calls();
    let observed_args = calls
        .iter()
        .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(
        observed_args,
        vec![
            vec!["file", "path=Projects/Rust.md"],
            vec![
                "create",
                "path=Projects/Rust.md",
                "content=Rust MCP\\nSecond line",
            ],
            vec!["file", "path=Projects/Rust.md"],
            vec![
                "append",
                "path=Projects/Rust.md",
                "content=\\nObsidian vault",
                "inline",
            ],
            vec!["read", "path=Projects/Rust.md"],
            vec!["files", "ext=md", "folder=Projects"],
            vec![
                "search:context",
                "query=obsidian",
                "limit=10",
                "path=Projects",
            ],
        ]
    );
    assert!(calls.iter().all(|call| call.vault == vault.path()));
}

#[tokio::test]
async fn refuses_non_markdown_writes() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let result = server.create_note_content("image.png", "").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn create_note_refuses_existing_note() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("Projects/Rust.md")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let result = server
        .create_note_content("Projects/Rust.md", "new content")
        .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "Note already exists; use replace_note to replace it"
    );
    assert_eq!(cli.calls().len(), 1);
}

#[tokio::test]
async fn replace_note_requires_existing_note_and_overwrites_it() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("Projects/Rust.md"), Ok("replaced")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    server
        .replace_note_content("Projects/Rust.md", "new content")
        .await
        .unwrap();

    assert_eq!(
        cli.calls()[1].args,
        vec![
            "create",
            "path=Projects/Rust.md",
            "content=new content",
            "overwrite",
        ]
    );
}

#[tokio::test]
async fn replace_note_refuses_missing_note() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Err("missing")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let error = server
        .replace_note_content("Projects/Rust.md", "new content")
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Note does not exist; use create_note to create it"
    );
    assert_eq!(cli.calls().len(), 1);
}

#[tokio::test]
async fn encodes_multiline_content_for_cli_arguments() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Err("missing"), Ok("created")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    server
        .create_note_content("Projects/Rust.md", "a\nb\tc\\d")
        .await
        .unwrap();

    assert_eq!(
        cli.calls()[1].args,
        vec!["create", "path=Projects/Rust.md", "content=a\\nb\\tc\\\\d",]
    );
}

#[tokio::test]
async fn uses_cli_for_tags_backlinks_and_daily_notes() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("#rust\t3\n#mcp\t2\n"),
        Ok("Ideas/MCP.md\t2\n"),
        Ok("# Daily\n"),
        Ok("2026-06-09.md"),
        Ok("appended"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let tags = server
        .list_tags_data(Some("Projects/Rust.md"), true, true, Some(10))
        .await
        .unwrap();
    let backlinks = server
        .list_backlinks_data("Projects/Rust.md", true, Some(10))
        .await
        .unwrap();
    let daily = server.read_daily_note_content().await.unwrap();
    server
        .append_daily_note_content("- [ ] Follow up\n", true)
        .await
        .unwrap();

    assert_eq!(tags, vec!["#rust\t3", "#mcp\t2"]);
    assert_eq!(backlinks, vec!["Ideas/MCP.md\t2"]);
    assert_eq!(daily, "# Daily\n");
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["tags", "path=Projects/Rust.md", "counts", "sort=count"],
            vec!["backlinks", "path=Projects/Rust.md", "counts"],
            vec!["daily:read"],
            vec!["daily:path"],
            vec!["daily:append", "content=- [ ] Follow up\\n", "inline"],
        ]
    );
}

#[tokio::test]
async fn uses_cli_for_note_context_and_vault_audit() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("Rust Project\n"),
        Ok("# Rust MCP\n## Context\n"),
        Ok("Start.md\nKnowledge/Dead End.md\n"),
        Ok("Start.md\t2\n"),
        Ok("Missing Guide\t2\tStart.md\nMissing Guide\t2\tProjects/Rust.md\n"),
        Ok("Knowledge/Orphan.md\nimage.png\n"),
        Ok("Knowledge/Dead End.md\nTodo.md\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let context = server
        .get_note_context_data("Projects/Rust.md", Some(10))
        .await
        .unwrap();
    let audit = server.audit_vault_data(Some(10)).await.unwrap();

    assert_eq!(context.aliases, vec!["Rust Project"]);
    assert_eq!(context.outgoing_link_count, 2);
    assert_eq!(context.backlinks, vec!["Start.md\t2"]);
    assert_eq!(audit.unresolved_links[0].link, "Missing Guide");
    assert_eq!(
        audit.unresolved_links[0].sources,
        vec!["Projects/Rust.md", "Start.md"]
    );
    assert_eq!(audit.orphan_notes, vec!["Knowledge/Orphan.md"]);
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["aliases", "path=Projects/Rust.md"],
            vec!["outline", "path=Projects/Rust.md", "format=md"],
            vec!["links", "path=Projects/Rust.md"],
            vec!["backlinks", "path=Projects/Rust.md", "counts"],
            vec!["unresolved", "counts", "verbose", "format=tsv"],
            vec!["orphans"],
            vec!["deadends"],
        ]
    );
}

#[tokio::test]
async fn graph_context_validates_paths_and_clamps_limits() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("Alias A\nAlias B\n"),
        Ok("No headings found.\n"),
        Ok("No links found.\n"),
        Ok("No backlinks found.\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let context = server
        .get_note_context_data("Projects/Rust.md", Some(1))
        .await
        .unwrap();

    assert_eq!(context.aliases, vec!["Alias A"]);
    assert!(context.outline.is_empty());
    assert!(
        server
            .get_note_context_data("../secret.md", None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn uses_cli_for_bases_workflow() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("Z.base\nProjects.base\nimage.png\nProjects.base\n"),
        Ok(
            r#"[{"file.path":"Projects/Rust.md","status":"active"},{"file.path":"Projects/Home.md","status":"active"}]"#,
        ),
        Ok("[]"),
        Ok("Created Projects/New Project.md\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let bases = server.list_bases_data(Some(1)).await.unwrap();
    let query = server
        .query_base_data("Projects.base", Some("Active projects"), Some(1))
        .await
        .unwrap();
    let created = server
        .create_base_item_data(
            "Projects.base",
            "Active projects",
            "New Project",
            Some("# New\n"),
        )
        .await
        .unwrap();

    assert_eq!(bases, vec!["Projects.base"]);
    assert_eq!(query.count, 1);
    assert_eq!(query.view.as_deref(), Some("Active projects"));
    assert_eq!(created.message, "Created Projects/New Project.md");
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["bases"],
            vec![
                "base:query",
                "path=Projects.base",
                "format=json",
                "view=Active projects",
            ],
            vec![
                "base:query",
                "path=Projects.base",
                "format=json",
                "view=Active projects",
            ],
            vec![
                "base:create",
                "path=Projects.base",
                "view=Active projects",
                "name=New Project",
                "content=# New\\n",
            ],
        ]
    );
}

#[tokio::test]
async fn bases_validate_paths_names_and_empty_list() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("No base files found in vault\n")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    assert!(server.list_bases_data(None).await.unwrap().is_empty());
    assert!(
        server
            .query_base_data("../Projects.base", None, None)
            .await
            .is_err()
    );
    assert!(
        server
            .query_base_data("Projects.md", None, None)
            .await
            .is_err()
    );
    assert!(
        server
            .create_base_item_data("Projects.base", "Active", "Folder/Name", None)
            .await
            .is_err()
    );
    assert_eq!(cli.calls().len(), 1);
}

#[tokio::test]
async fn uses_cli_for_work_system_tasks_daily_range_and_projects() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok(" \t- [ ] Review inbox\tTodo.md\t4\n"),
        Ok("Todo.md"),
        Ok("appended"),
        Ok("2026-06-09.md"),
        Ok("daily appended"),
        Ok("Todo.md"),
        Ok(" \t- [ ] Review inbox\tTodo.md\t4\n"),
        Ok("updated"),
        Ok("# Monday\n"),
        Err("missing"),
        Ok("Projects/Home.md\nProjects/Rust.md\nimage.png\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let tasks = server
        .list_tasks_data(&TaskReadTarget::Vault, Some(&TaskStatus::Todo), Some(10))
        .await
        .unwrap();
    let (target, task) = server
        .create_task_data(
            &TaskWriteTarget::Note {
                path: "Todo.md".to_string(),
            },
            "Review inbox",
        )
        .await
        .unwrap();
    let (daily_target, daily_task) = server
        .create_task_data(&TaskWriteTarget::Daily, "- [ ] Daily follow up")
        .await
        .unwrap();
    let status = server
        .set_task_status_data("Todo.md", 4, &TaskStatus::Done)
        .await
        .unwrap();
    let daily_notes = server
        .read_daily_notes_data("2026-06-01", "2026-06-02", Some(10))
        .await
        .unwrap();
    let (project_directory, projects) = server
        .list_project_note_paths(Some("Projects"), Some(10))
        .await
        .unwrap();

    assert_eq!(tasks[0].path, "Todo.md");
    assert_eq!(tasks[0].line, 4);
    assert_eq!(target, "Todo.md");
    assert_eq!(task, "- [ ] Review inbox");
    assert_eq!(daily_target, "daily");
    assert_eq!(daily_task, "- [ ] Daily follow up");
    assert_eq!(status, "x");
    assert_eq!(daily_notes[0].content.as_deref(), Some("# Monday\n"));
    assert!(
        daily_notes[1]
            .error
            .as_deref()
            .is_some_and(|error| error == "Error: File not found.")
    );
    assert_eq!(project_directory, "Projects");
    assert_eq!(projects, vec!["Projects/Home.md", "Projects/Rust.md"]);
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["tasks", "format=tsv", "todo"],
            vec!["file", "path=Todo.md"],
            vec!["append", "path=Todo.md", "content=- [ ] Review inbox"],
            vec!["daily:path"],
            vec!["daily:append", "content=- [ ] Daily follow up"],
            vec!["file", "path=Todo.md"],
            vec!["task", "path=Todo.md", "line=4"],
            vec!["task", "path=Todo.md", "line=4", "done"],
            vec!["read", "path=2026-06-01.md"],
            vec!["read", "path=2026-06-02.md"],
            vec!["files", "ext=md", "folder=Projects"],
        ]
    );
}

#[tokio::test]
async fn uses_cli_for_properties_and_property_preview() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok(r#"{"status":"active","priority":2}"#),
        Ok("Projects/Rust.md"),
        Ok(r#"{"status":"active","priority":2}"#),
        Ok("Projects/Rust.md"),
        Ok(r#"{"status":"active"}"#),
        Ok("updated"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let properties = server
        .list_properties_data("Projects/Rust.md")
        .await
        .unwrap();
    let preview = server
        .set_property_data(
            "Projects/Rust.md",
            "status",
            "paused",
            Some(&PropertyType::Text),
            true,
        )
        .await
        .unwrap();
    let applied = server
        .set_property_data(
            "Projects/Rust.md",
            "status",
            "done",
            Some(&PropertyType::Text),
            false,
        )
        .await
        .unwrap();

    assert_eq!(properties.len(), 2);
    assert_eq!(
        preview.previous_value,
        Some(rmcp::serde_json::json!("active"))
    );
    assert!(!preview.applied);
    assert!(applied.applied);
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["properties", "path=Projects/Rust.md", "format=json"],
            vec!["file", "path=Projects/Rust.md"],
            vec!["properties", "path=Projects/Rust.md", "format=json"],
            vec!["file", "path=Projects/Rust.md"],
            vec!["properties", "path=Projects/Rust.md", "format=json"],
            vec![
                "property:set",
                "path=Projects/Rust.md",
                "name=status",
                "value=done",
                "type=text",
            ],
        ]
    );
}

#[tokio::test]
async fn set_property_tool_defaults_to_preview() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("Projects/Rust.md"), Ok(r#"{"status":"active"}"#)]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    let (server_transport, client_transport) = tokio::io::duplex(16_384);
    let server_handle = tokio::spawn(async move {
        server
            .serve(server_transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = TestClient.serve(client_transport).await.unwrap();
    let arguments = rmcp::serde_json::json!({
        "path": "Projects/Rust.md",
        "name": "status",
        "value": "paused",
        "property_type": "text"
    })
    .as_object()
    .unwrap()
    .clone();

    let result = client
        .peer()
        .call_tool(CallToolRequestParams::new("set_property").with_arguments(arguments))
        .await
        .unwrap();

    assert!(!result.is_error.unwrap_or(false));
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["file", "path=Projects/Rust.md"],
            vec!["properties", "path=Projects/Rust.md", "format=json"],
        ]
    );

    client.cancel().await.unwrap();
    server_handle.await.unwrap();
}

#[tokio::test]
async fn lists_overdue_and_project_tasks() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok(
            " \t- [ ] Older 📅 2026-06-01\tTodo.md\t1\n \t- [ ] Today 📅 2026-06-05\tTodo.md\t2\n \t- [ ] Inline due:: 2026-06-03\tTodo.md\t3\n \t- [ ] No date\tTodo.md\t4\n",
        ),
        Ok(" \t- [ ] Project task\tProjects/Rust.md\t8\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let overdue = server
        .list_overdue_tasks_data("2026-06-05", &TaskReadTarget::Vault, Some(10))
        .await
        .unwrap();
    let project_tasks = server
        .list_tasks_by_project_data("Projects/Rust.md", Some(&TaskStatus::Todo), Some(10))
        .await
        .unwrap();

    assert_eq!(
        overdue
            .iter()
            .map(|task| task.due_date.as_str())
            .collect::<Vec<_>>(),
        vec!["2026-06-01", "2026-06-03"]
    );
    assert_eq!(project_tasks[0].path, "Projects/Rust.md");
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["tasks", "format=tsv", "todo"],
            vec!["tasks", "format=tsv", "path=Projects/Rust.md", "todo"],
        ]
    );
}

#[tokio::test]
async fn composes_project_status_and_previews_note_changes() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("# Rust MCP\n"),
        Ok(r#"{"status":"active"}"#),
        Ok(" \t- [ ] Ship v0.3\tProjects/Rust.md\t8\n"),
        Ok("x\t- [x] Ship v0.2\tProjects/Rust.md\t7\n"),
        Ok("Ideas/MCP.md\t2\n"),
        Ok("Projects/Rust.md"),
        Ok("# Rust MCP\n"),
        Ok("Projects/Rust.md"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let status = server
        .get_project_status_data("Projects/Rust.md", Some(10))
        .await
        .unwrap();
    let append = server
        .preview_note_change_data("Projects/Rust.md", &NoteChangeMode::Append, "\n## Next\n")
        .await
        .unwrap();
    let create_error = server
        .preview_note_change_data("Projects/Rust.md", &NoteChangeMode::Create, "# New")
        .await
        .unwrap_err();

    assert_eq!(status.open_task_count, 1);
    assert_eq!(status.completed_task_count, 1);
    assert_eq!(status.backlink_count, 1);
    assert_eq!(status.properties[0].name, "status");
    assert_eq!(append.proposed_content, "# Rust MCP\n\n## Next\n");
    assert_eq!(
        create_error.to_string(),
        "Note already exists; preview replace or append instead"
    );
}

#[tokio::test]
async fn previews_normalized_create_replace_and_append_change_set() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Err("missing"),
        Ok("Projects/Rust.md"),
        Ok("# Rust"),
        Ok("Log.md"),
        Ok("# Log"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let preview = server
        .preview_change_set_data(vec![
            ChangeSetOperation {
                path: "./Ideas/New.md".to_string(),
                mode: NoteChangeMode::Create,
                content: "# New".to_string(),
            },
            ChangeSetOperation {
                path: "Projects/Rust.md".to_string(),
                mode: NoteChangeMode::Replace,
                content: "# Rewritten".to_string(),
            },
            ChangeSetOperation {
                path: "Log.md".to_string(),
                mode: NoteChangeMode::Append,
                content: "\nNext".to_string(),
            },
        ])
        .await
        .unwrap();

    assert_eq!(preview.count, 3);
    assert_eq!(preview.changes[0].path, "Ideas/New.md");
    assert_eq!(preview.changes[0].proposed_content, "# New");
    assert_eq!(preview.changes[1].proposed_content, "# Rewritten");
    assert_eq!(preview.changes[2].proposed_content, "# Log\nNext");
    assert!(preview.preview_token.starts_with("sha256:"));
}

#[tokio::test]
async fn change_set_conflict_performs_no_writes() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("Projects/Rust.md"),
        Ok("# Old"),
        Ok("Projects/Rust.md"),
        Ok("# Changed"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    let changes = vec![ChangeSetOperation {
        path: "Projects/Rust.md".to_string(),
        mode: NoteChangeMode::Replace,
        content: "# Proposed".to_string(),
    }];
    let preview = server
        .preview_change_set_data(changes.clone())
        .await
        .unwrap();

    let result = server
        .apply_change_set_data(changes, &preview.preview_token)
        .await
        .unwrap();

    assert_eq!(result.outcome, ChangeSetApplyOutcome::Conflict);
    assert_ne!(result.expected_preview_token, result.observed_preview_token);
    assert_eq!(result.skipped, vec![0]);
    assert!(result.applied.is_empty());
    assert_eq!(cli.calls().len(), 4);
}

#[tokio::test]
async fn change_set_preflights_all_notes_and_reports_partial_failure() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Err("missing"),
        Err("missing"),
        Err("missing"),
        Err("missing"),
        Err("missing"),
        Err("missing"),
        Err("missing"),
        Ok("created"),
        Err("missing"),
        Err("write failed"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    let changes = ["One.md", "Two.md", "Three.md"]
        .into_iter()
        .map(|path| ChangeSetOperation {
            path: path.to_string(),
            mode: NoteChangeMode::Create,
            content: format!("# {path}"),
        })
        .collect::<Vec<_>>();
    let preview = server
        .preview_change_set_data(changes.clone())
        .await
        .unwrap();

    let result = server
        .apply_change_set_data(changes, &preview.preview_token)
        .await
        .unwrap();

    assert_eq!(result.outcome, ChangeSetApplyOutcome::PartialFailure);
    assert_eq!(result.applied[0].index, 0);
    assert_eq!(result.failed.unwrap().index, 1);
    assert_eq!(result.skipped, vec![2]);
    let calls = cli.calls();
    assert_eq!(calls[5].args, vec!["file", "path=Three.md"]);
    assert_eq!(calls[6].args, vec!["file", "path=One.md"]);
    assert!(
        calls
            .iter()
            .all(|call| call.args != vec!["file", "path=Three.md"] || call == &calls[5])
    );
}

#[tokio::test]
async fn applies_approved_create_replace_and_append_change_set_in_order() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Err("missing"),
        Ok("Replace.md"),
        Ok("old"),
        Ok("Append.md"),
        Ok("base"),
        Err("missing"),
        Ok("Replace.md"),
        Ok("old"),
        Ok("Append.md"),
        Ok("base"),
        Err("missing"),
        Ok("created"),
        Ok("Replace.md"),
        Ok("replaced"),
        Ok("Append.md"),
        Ok("appended"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();
    let changes = vec![
        ChangeSetOperation {
            path: "Create.md".to_string(),
            mode: NoteChangeMode::Create,
            content: "new".to_string(),
        },
        ChangeSetOperation {
            path: "Replace.md".to_string(),
            mode: NoteChangeMode::Replace,
            content: "replacement".to_string(),
        },
        ChangeSetOperation {
            path: "Append.md".to_string(),
            mode: NoteChangeMode::Append,
            content: "+tail".to_string(),
        },
    ];
    let preview = server
        .preview_change_set_data(changes.clone())
        .await
        .unwrap();

    let result = server
        .apply_change_set_data(changes, &preview.preview_token)
        .await
        .unwrap();

    assert_eq!(result.outcome, ChangeSetApplyOutcome::Applied);
    assert_eq!(result.applied.len(), 3);
    assert!(result.skipped.is_empty());
    assert_eq!(
        cli.calls()[15].args,
        vec!["append", "path=Append.md", "content=+tail", "inline"]
    );
}

#[tokio::test]
async fn vault_info_uses_cli_metadata_and_total_count() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::outputs([
        format!(
            "name\tKnowledge\npath\t{}\nfiles\t57\nfolders\t8\nsize\t1234\n",
            vault.path().display()
        ),
        "Markdown files: 42\n".to_string(),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let info = server.vault_info_data().await.unwrap();

    assert_eq!(
        info,
        VaultInfoResponse {
            configured_vault_path: vault.path().display().to_string(),
            obsidian_vault_path: vault.path().display().to_string(),
            obsidian_vault_name: "Knowledge".to_string(),
            markdown_notes: 42,
        }
    );
    let calls = cli.calls();
    let observed_args = calls
        .iter()
        .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    assert_eq!(
        observed_args,
        vec![vec!["vault"], vec!["files", "ext=md", "total"],]
    );
}

#[tokio::test]
async fn vault_info_rejects_empty_metadata() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let error = server.vault_info_data().await.unwrap_err();

    assert!(error.to_string().contains("Cannot parse vault name"));
}

#[tokio::test]
async fn vault_name_prefixes_cli_calls() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("Projects/Rust.md\n")]);
    let server = ObsidianMcp::with_runner_and_vault_name(
        vault.path(),
        Some(" main ".to_string()),
        cli.clone(),
    )
    .unwrap();

    let notes = server
        .list_note_paths(Some("Projects"), Some(10))
        .await
        .unwrap();

    assert_eq!(notes, vec!["Projects/Rust.md"]);
    assert_eq!(
        cli.calls()[0].args,
        vec!["vault=main", "files", "ext=md", "folder=Projects"]
    );
}

#[test]
fn resource_descriptors_are_curated_and_do_not_query_vault() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let resources = server.list_resource_descriptors();
    let uris = resources
        .iter()
        .map(|resource| resource.uri.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        uris,
        vec![
            "obsidian://vault/info",
            "obsidian://vault/audit",
            "obsidian://bases/index",
            "obsidian://notes/index",
            "obsidian://tags/index",
            "obsidian://daily/today",
            "obsidian://tasks/open",
            "obsidian://projects/index",
        ]
    );
    assert!(cli.calls().is_empty());
}

#[tokio::test]
async fn tool_errors_use_mcp_error_codes_without_extra_cli_calls() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let invalid = tool_mcp_error(ObsidianMcpError::InvalidPath("bad path".to_string()));
    assert_eq!(invalid.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    assert!(server.read_note_content("../secret.md").await.is_err());
    assert!(cli.calls().is_empty());

    let missing = tool_mcp_error(ObsidianMcpError::NoteNotFound("missing".to_string()));
    assert_eq!(missing.code, rmcp::model::ErrorCode::RESOURCE_NOT_FOUND);
    let failed = tool_mcp_error(ObsidianMcpError::CliFailed("failed".to_string()));
    assert_eq!(failed.code, rmcp::model::ErrorCode::INTERNAL_ERROR);
}

#[test]
fn resource_templates_expose_note_uri_template() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let templates = server.list_resource_template_descriptors();

    assert_eq!(templates.len(), 6);
    assert_eq!(templates[0].uri_template, "obsidian://note/{path}");
    assert_eq!(templates[0].mime_type.as_deref(), Some("text/markdown"));
    assert_eq!(templates[1].uri_template, "obsidian://base/{path}");
    assert_eq!(templates[1].mime_type.as_deref(), Some("application/json"));
    assert_eq!(templates[2].uri_template, "obsidian://daily/{date}");
    assert_eq!(templates[2].mime_type.as_deref(), Some("text/markdown"));
    assert_eq!(templates[3].uri_template, "obsidian://tasks/overdue/{date}");
    assert_eq!(templates[3].mime_type.as_deref(), Some("application/json"));
    assert_eq!(templates[4].uri_template, "obsidian://project/{path}");
    assert_eq!(templates[5].uri_template, "obsidian://properties/{path}");
}

#[tokio::test]
async fn read_note_resource_decodes_uri_and_reads_note() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("# Space Note\n")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let result = server
        .read_resource_uri("obsidian://note/Space%20Note.md")
        .await
        .unwrap();

    assert_eq!(cli.calls()[0].args, vec!["read", "path=Space Note.md"]);
    match &result.contents[0] {
        ResourceContents::TextResourceContents {
            text, mime_type, ..
        } => {
            assert_eq!(text, "# Space Note\n");
            assert_eq!(mime_type.as_deref(), Some("text/markdown"));
        }
        _ => panic!("expected text resource contents"),
    }
}

#[tokio::test]
async fn read_static_resources_returns_vault_info_and_index() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::outputs([
        format!(
            "name\tKnowledge\npath\t{}\nfiles\t57\n",
            vault.path().display()
        ),
        "42\n".to_string(),
        "Projects/Rust.md\nSpace Note.md\n".to_string(),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let info = server
        .read_resource_uri("obsidian://vault/info")
        .await
        .unwrap();
    let index = server
        .read_resource_uri("obsidian://notes/index")
        .await
        .unwrap();

    assert_resource_text_contains(&info, "obsidian_vault_name\tKnowledge");
    assert_resource_text_contains(&info, "markdown_notes\t42");
    assert_resource_text_contains(&index, "Projects/Rust.md\nSpace Note.md");
}

#[tokio::test]
async fn read_tag_and_daily_resources() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("#rust\t3\n#mcp\t2\n"), Ok("# Daily\n")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let tags = server
        .read_resource_uri("obsidian://tags/index")
        .await
        .unwrap();
    let daily = server
        .read_resource_uri("obsidian://daily/today")
        .await
        .unwrap();

    assert_resource_text_contains(&tags, "#rust\t3");
    assert_resource_text_contains(&daily, "# Daily");
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![vec!["tags", "counts", "sort=count"], vec!["daily:read"],]
    );
}

#[tokio::test]
async fn read_vault_audit_resource() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("Missing Guide\t1\tStart.md\n"),
        Ok("Knowledge/Orphan.md\n"),
        Ok("Knowledge/Dead End.md\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let audit = server
        .read_resource_uri("obsidian://vault/audit")
        .await
        .unwrap();

    assert_resource_text_contains(&audit, r#""link": "Missing Guide""#);
    assert_resource_text_contains(&audit, r#""orphan_notes": ["#);
}

#[tokio::test]
async fn removed_backlink_and_context_resource_uris_are_not_found_without_cli_calls() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    for uri in [
        "obsidian://backlinks/Projects/Rust.md",
        "obsidian://context/Projects/Rust.md",
    ] {
        assert!(matches!(
            server.read_resource_uri(uri).await,
            Err(ObsidianMcpError::ResourceNotFound(_))
        ));
    }

    assert!(cli.calls().is_empty());
}

#[tokio::test]
async fn read_bases_resources() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("Projects.base\n"),
        Ok(r#"[{"file.path":"Projects/Rust.md","status":"active"}]"#),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let index = server
        .read_resource_uri("obsidian://bases/index")
        .await
        .unwrap();
    let base = server
        .read_resource_uri("obsidian://base/Projects.base")
        .await
        .unwrap();

    assert_resource_text_contains(&index, "Projects.base");
    assert_resource_text_contains(&base, r#""path": "Projects.base""#);
    assert_resource_text_contains(&base, r#""file.path": "Projects/Rust.md""#);
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["bases"],
            vec!["base:query", "path=Projects.base", "format=json"],
        ]
    );
}

#[tokio::test]
async fn read_work_system_resources() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok(" \t- [ ] Review inbox\tTodo.md\t4\n"),
        Ok("Projects/Home.md\nProjects/Rust.md\n"),
        Ok("# Dated daily\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let tasks = server
        .read_resource_uri("obsidian://tasks/open")
        .await
        .unwrap();
    let projects = server
        .read_resource_uri("obsidian://projects/index")
        .await
        .unwrap();
    let daily = server
        .read_resource_uri("obsidian://daily/2026-06-04")
        .await
        .unwrap();

    assert_resource_text_contains(&tasks, "Todo.md:4\t- [ ] Review inbox");
    assert_resource_text_contains(&projects, "Projects/Home.md\nProjects/Rust.md");
    assert_resource_text_contains(&daily, "# Dated daily");
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["tasks", "format=tsv", "todo"],
            vec!["files", "ext=md", "folder=Projects"],
            vec!["read", "path=2026-06-04.md"],
        ]
    );
}

#[tokio::test]
async fn read_v030_work_system_resources() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok(" \t- [ ] Past due 📅 2026-06-01\tTodo.md\t4\n"),
        Ok(r#"{"status":"active"}"#),
        Ok("# Rust MCP\n"),
        Ok(r#"{"status":"active"}"#),
        Ok(" \t- [ ] Ship v0.3\tProjects/Rust.md\t8\n"),
        Ok("x\t- [x] Ship v0.2\tProjects/Rust.md\t7\n"),
        Ok("Ideas/MCP.md\t2\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let overdue = server
        .read_resource_uri("obsidian://tasks/overdue/2026-06-05")
        .await
        .unwrap();
    let properties = server
        .read_resource_uri("obsidian://properties/Projects/Rust.md")
        .await
        .unwrap();
    let project = server
        .read_resource_uri("obsidian://project/Projects/Rust.md")
        .await
        .unwrap();

    assert_resource_text_contains(&overdue, r#""due_date": "2026-06-01""#);
    assert_resource_text_contains(&properties, r#""name": "status""#);
    assert_resource_text_contains(&project, r#""open_task_count": 1"#);
}

#[test]
fn resource_uri_display_and_parse_round_trip() {
    let note = VaultRelativePath::markdown("Folder/Space Note.md").unwrap();
    let base = VaultRelativePath::base("Folder/Space Base.base").unwrap();
    let date = DailyDate::parse("2026-06-05").unwrap();
    let cases = [
        (ObsidianResourceUri::VaultInfo, "obsidian://vault/info"),
        (ObsidianResourceUri::VaultAudit, "obsidian://vault/audit"),
        (ObsidianResourceUri::BasesIndex, "obsidian://bases/index"),
        (ObsidianResourceUri::NotesIndex, "obsidian://notes/index"),
        (ObsidianResourceUri::TagsIndex, "obsidian://tags/index"),
        (ObsidianResourceUri::DailyToday, "obsidian://daily/today"),
        (
            ObsidianResourceUri::Daily(date.clone()),
            "obsidian://daily/2026-06-05",
        ),
        (ObsidianResourceUri::TasksOpen, "obsidian://tasks/open"),
        (
            ObsidianResourceUri::TasksOverdue(date),
            "obsidian://tasks/overdue/2026-06-05",
        ),
        (
            ObsidianResourceUri::ProjectsIndex,
            "obsidian://projects/index",
        ),
        (
            ObsidianResourceUri::Note(note.clone()),
            "obsidian://note/Folder/Space%20Note.md",
        ),
        (
            ObsidianResourceUri::Base(base),
            "obsidian://base/Folder/Space%20Base.base",
        ),
        (
            ObsidianResourceUri::Project(note.clone()),
            "obsidian://project/Folder/Space%20Note.md",
        ),
        (
            ObsidianResourceUri::Properties(note),
            "obsidian://properties/Folder/Space%20Note.md",
        ),
    ];

    for (resource_uri, expected) in cases {
        assert_eq!(resource_uri.to_string(), expected);
        assert_eq!(
            expected.parse::<ObsidianResourceUri>().unwrap(),
            resource_uri
        );
    }

    assert!(matches!(
        "obsidian://backlinks/Folder/Space%20Note.md".parse::<ObsidianResourceUri>(),
        Err(ObsidianMcpError::ResourceNotFound(_))
    ));
    assert!(matches!(
        "obsidian://context/Folder/Space%20Note.md".parse::<ObsidianResourceUri>(),
        Err(ObsidianMcpError::ResourceNotFound(_))
    ));
    assert!(
        "obsidian://note/bad%"
            .parse::<ObsidianResourceUri>()
            .is_err()
    );
}

#[test]
fn prompt_descriptors_and_prompt_messages_are_available() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let prompts = server.list_prompt_descriptors();
    let prompt_names = prompts
        .iter()
        .map(|prompt| prompt.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        prompt_names,
        vec![
            "summarize_note",
            "search_and_synthesize",
            "draft_note_update",
            "draft_change_set",
            "daily_review",
            "plan_day",
            "tag_overview",
            "backlink_review",
            "weekly_review",
            "project_review",
            "inbox_triage",
            "vault_audit",
            "base_review"
        ]
    );

    let result = server
        .get_prompt_result(prompt_request(
            "summarize_note",
            [("path", "Projects/Rust.md")],
        ))
        .unwrap();

    assert_prompt_text_contains(&result, "obsidian://note/Projects/Rust.md");
    assert_prompt_text_contains(&result, "Do not modify the vault");

    let daily = server
        .get_prompt_result(GetPromptRequestParams::new("daily_review"))
        .unwrap();
    assert_prompt_text_contains(&daily, "obsidian://daily/today");

    let plan_day = server
        .get_prompt_result(prompt_request("plan_day", [("date", "2026-06-05")]))
        .unwrap();
    assert_prompt_text_contains(&plan_day, "obsidian://daily/2026-06-05");
    assert_prompt_text_contains(&plan_day, "obsidian://tasks/overdue/2026-06-05");

    let tag = server
        .get_prompt_result(prompt_request("tag_overview", [("tag", "rust")]))
        .unwrap();
    assert_prompt_text_contains(&tag, "#rust");

    let backlinks = server
        .get_prompt_result(prompt_request(
            "backlink_review",
            [("path", "Projects/Rust.md")],
        ))
        .unwrap();
    assert_prompt_text_contains(&backlinks, "list_backlinks");
    assert_prompt_text_contains(&backlinks, "obsidian://note/Projects/Rust.md");

    let weekly = server
        .get_prompt_result(prompt_request(
            "weekly_review",
            [("from", "2026-06-01"), ("to", "2026-06-07")],
        ))
        .unwrap();
    assert_prompt_text_contains(&weekly, "read_daily_notes");
    assert_prompt_text_contains(&weekly, "obsidian://tasks/overdue/2026-06-07");

    let project = server
        .get_prompt_result(prompt_request(
            "project_review",
            [("path", "Projects/Rust.md")],
        ))
        .unwrap();
    assert_prompt_text_contains(&project, "obsidian://note/Projects/Rust.md");
    assert_prompt_text_contains(&project, "get_project_status");
    assert_prompt_text_contains(&project, "list_backlinks");

    let inbox = server
        .get_prompt_result(prompt_request("inbox_triage", [("directory", "Inbox")]))
        .unwrap();
    assert_prompt_text_contains(&inbox, "list_notes");

    let audit = server
        .get_prompt_result(GetPromptRequestParams::new("vault_audit"))
        .unwrap();
    assert_prompt_text_contains(&audit, "obsidian://vault/audit");
    assert_prompt_text_contains(&audit, "get_note_context");

    let base = server
        .get_prompt_result(prompt_request(
            "base_review",
            [("path", "Projects.base"), ("view", "Active projects")],
        ))
        .unwrap();
    assert_prompt_text_contains(&base, "query_base");
    assert_prompt_text_contains(&base, "Active projects");
    assert_prompt_text_contains(&base, "Do not call `create_base_item`");

    let change_set = server
        .get_prompt_result(prompt_request(
            "draft_change_set",
            [("intent", "Split one note into two focused notes")],
        ))
        .unwrap();
    assert_prompt_text_contains(&change_set, "preview_change_set");
    assert_prompt_text_contains(&change_set, "explicitly accepts that exact preview token");
}

#[test]
fn prompt_requests_validate_required_arguments() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let error = server
        .get_prompt_result(GetPromptRequestParams::new("summarize_note"))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Prompt 'summarize_note' requires argument 'path'"
    );
    assert!(
        server
            .get_prompt_result(GetPromptRequestParams::new("draft_change_set"))
            .is_err()
    );
}

#[test]
fn server_info_advertises_all_three_capabilities() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let info = server.get_info();

    assert!(info.capabilities.tools.is_some());
    assert!(info.capabilities.resources.is_some());
    assert!(info.capabilities.prompts.is_some());
}

#[test]
fn default_vault_path_points_to_project_vault() {
    let path = ObsidianMcp::default_vault_path();

    assert!(path.ends_with("obsidian-vault"));
    assert!(
        path.is_dir(),
        "expected project vault to exist at {}",
        path.display()
    );
}

#[tokio::test]
async fn mcp_round_trip_exposes_tools_resources_and_prompts() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok(" \t- [ ] Review release\tTodo.md\t4\n"),
        Ok(" \t- [ ] Past due 📅 2026-06-01\tTodo.md\t4\n"),
        Ok("Missing Guide\t1\tStart.md\n"),
        Ok("Knowledge/Orphan.md\n"),
        Ok("Knowledge/Dead End.md\n"),
        Ok("Projects.base\n"),
        Err("missing"),
        Ok("Projects/Rust.md\n"),
        Ok(" \t- [ ] Past due 📅 2026-06-01\tTodo.md\t4\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();
    let (server_transport, client_transport) = tokio::io::duplex(16_384);
    let server_handle = tokio::spawn(async move {
        server
            .serve(server_transport)
            .await
            .unwrap()
            .waiting()
            .await
            .unwrap();
    });
    let client = TestClient.serve(client_transport).await.unwrap();

    let tools = client.peer().list_all_tools().await.unwrap();
    let resource_page = client.peer().list_resources(None).await.unwrap();
    assert!(resource_page.next_cursor.is_none());
    let resources = resource_page.resources;
    let templates = client.peer().list_all_resource_templates().await.unwrap();
    let prompts = client.peer().list_all_prompts().await.unwrap();
    let task_args = rmcp::serde_json::json!({
        "target": {"type": "vault"},
        "status": {"type": "todo"},
        "limit": 10
    })
    .as_object()
    .unwrap()
    .clone();
    let task_result = client
        .peer()
        .call_tool(CallToolRequestParams::new("list_tasks").with_arguments(task_args))
        .await
        .unwrap();
    let overdue_args = rmcp::serde_json::json!({
        "as_of": "2026-06-05",
        "target": {"type": "vault"},
        "limit": 10
    })
    .as_object()
    .unwrap()
    .clone();
    let overdue_result = client
        .peer()
        .call_tool(CallToolRequestParams::new("list_overdue_tasks").with_arguments(overdue_args))
        .await
        .unwrap();
    let audit_result = client
        .peer()
        .call_tool(CallToolRequestParams::new("audit_vault"))
        .await
        .unwrap();
    let bases_result = client
        .peer()
        .call_tool(CallToolRequestParams::new("list_bases"))
        .await
        .unwrap();
    let preview_change_set_args = rmcp::serde_json::json!({
        "changes": [{
            "path": "Draft.md",
            "mode": "create",
            "content": "# Draft"
        }]
    })
    .as_object()
    .unwrap()
    .clone();
    let preview_change_set_result = client
        .peer()
        .call_tool(
            CallToolRequestParams::new("preview_change_set")
                .with_arguments(preview_change_set_args),
        )
        .await
        .unwrap();
    let note_index = client
        .peer()
        .read_resource(ReadResourceRequestParams::new("obsidian://notes/index"))
        .await
        .unwrap();
    let overdue_resource = client
        .peer()
        .read_resource(ReadResourceRequestParams::new(
            "obsidian://tasks/overdue/2026-06-05",
        ))
        .await
        .unwrap();
    let weekly = client
        .peer()
        .get_prompt(prompt_request(
            "weekly_review",
            [("from", "2026-06-01"), ("to", "2026-06-07")],
        ))
        .await
        .unwrap();
    let plan_day = client
        .peer()
        .get_prompt(prompt_request("plan_day", [("date", "2026-06-05")]))
        .await
        .unwrap();

    assert!(tools.iter().any(|tool| tool.name == "list_tasks"));
    assert!(tools.iter().any(|tool| tool.name == "get_project_status"));
    assert!(tools.iter().any(|tool| tool.name == "list_backlinks"));
    assert!(tools.iter().any(|tool| tool.name == "get_note_context"));
    assert!(tools.iter().any(|tool| tool.name == "audit_vault"));
    assert!(tools.iter().any(|tool| tool.name == "list_bases"));
    assert!(tools.iter().any(|tool| tool.name == "query_base"));
    assert!(tools.iter().any(|tool| tool.name == "create_base_item"));
    assert!(tools.iter().any(|tool| tool.name == "preview_change_set"));
    assert!(tools.iter().any(|tool| tool.name == "apply_change_set"));
    assert_eq!(
        resources
            .iter()
            .map(|resource| resource.uri.as_str())
            .collect::<Vec<_>>(),
        vec![
            "obsidian://vault/info",
            "obsidian://vault/audit",
            "obsidian://bases/index",
            "obsidian://notes/index",
            "obsidian://tags/index",
            "obsidian://daily/today",
            "obsidian://tasks/open",
            "obsidian://projects/index",
        ]
    );
    assert!(
        templates
            .iter()
            .any(|template| template.uri_template == "obsidian://daily/{date}")
    );
    assert!(
        templates
            .iter()
            .any(|template| template.uri_template == "obsidian://project/{path}")
    );
    assert!(!templates.iter().any(|template| {
        matches!(
            template.uri_template.as_str(),
            "obsidian://backlinks/{path}" | "obsidian://context/{path}"
        )
    }));
    assert!(
        templates
            .iter()
            .any(|template| template.uri_template == "obsidian://base/{path}")
    );
    assert!(prompts.iter().any(|prompt| prompt.name == "weekly_review"));
    assert!(prompts.iter().any(|prompt| prompt.name == "plan_day"));
    assert!(prompts.iter().any(|prompt| prompt.name == "vault_audit"));
    assert!(prompts.iter().any(|prompt| prompt.name == "base_review"));
    assert!(
        prompts
            .iter()
            .any(|prompt| prompt.name == "draft_change_set")
    );
    assert!(!task_result.is_error.unwrap_or(false));
    assert!(!overdue_result.is_error.unwrap_or(false));
    assert!(!audit_result.is_error.unwrap_or(false));
    assert!(!bases_result.is_error.unwrap_or(false));
    assert!(!preview_change_set_result.is_error.unwrap_or(false));
    assert_resource_text_contains(&note_index, "Projects/Rust.md");
    assert_resource_text_contains(&overdue_resource, "2026-06-01");
    assert_prompt_text_contains(&weekly, "read_daily_notes");
    assert_prompt_text_contains(&plan_day, "get_project_status");

    client.cancel().await.unwrap();
    server_handle.await.unwrap();
}

#[derive(Debug, Clone, Default)]
struct TestClient;

impl ClientHandler for TestClient {
    fn get_info(&self) -> ClientInfo {
        ClientInfo::default()
    }
}

#[tokio::test]
#[ignore = "requires Obsidian to be running with CLI enabled and OBSIDIAN_VAULT_PATH set"]
async fn real_cli_smoke_vault_info() {
    let server = guarded_real_cli_server();

    server.validate_vault().await.unwrap();
    server.vault_info_data().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Obsidian to be running with CLI enabled and OBSIDIAN_VAULT_PATH set"]
async fn real_cli_smoke_work_system_reads() {
    let note_path = env::var("OBSIDIAN_SMOKE_NOTE_PATH")
        .unwrap_or_else(|_| "RemediationFixtures/Existing.md".to_string());
    let server = guarded_real_cli_server();

    server.validate_vault().await.unwrap();
    server.list_properties_data(&note_path).await.unwrap();
    server
        .list_overdue_tasks_data("2026-06-05", &TaskReadTarget::Vault, Some(10))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires Obsidian to be running with CLI enabled and OBSIDIAN_VAULT_PATH set"]
async fn real_cli_smoke_knowledge_graph_reads() {
    let note_path = env::var("OBSIDIAN_SMOKE_NOTE_PATH")
        .unwrap_or_else(|_| "RemediationFixtures/Existing.md".to_string());
    let server = guarded_real_cli_server();

    server.validate_vault().await.unwrap();
    server
        .get_note_context_data(&note_path, Some(10))
        .await
        .unwrap();
    server.audit_vault_data(Some(10)).await.unwrap();
}

#[tokio::test]
#[ignore = "requires Obsidian to be running with CLI enabled and OBSIDIAN_VAULT_PATH set"]
async fn real_cli_smoke_bases_reads() {
    let server = guarded_real_cli_server();

    server.validate_vault().await.unwrap();
    let bases = server.list_bases_data(Some(10)).await.unwrap();
    if let Some(path) = bases.first() {
        server.query_base_data(path, None, Some(10)).await.unwrap();
    }
}

#[tokio::test]
#[ignore = "requires guarded fixtures in /Users/lukasz/Desktop/test-vault"]
async fn real_cli_remediation_reads_writes_and_large_output() {
    let server = guarded_real_cli_server();
    server.validate_vault().await.unwrap();

    assert!(matches!(
        server
            .read_note_content("RemediationFixtures/Missing.md")
            .await,
        Err(ObsidianMcpError::NoteNotFound(_))
    ));
    assert!(
        server
            .search_note_contents("__no_remediation_match__", None, Some(10))
            .await
            .unwrap()
            .is_empty()
    );

    let live_path = "RemediationFixtures/Live.md";
    let live_file = server.vault_path().join(live_path);
    let _ = fs::remove_file(&live_file);
    server
        .create_note_content(live_path, "# Live")
        .await
        .unwrap();
    server
        .replace_note_content(live_path, "# Live Replaced")
        .await
        .unwrap();
    server
        .append_note_content(live_path, "\nTail")
        .await
        .unwrap();
    server
        .set_property_data(
            live_path,
            "status",
            "active",
            Some(&PropertyType::Text),
            false,
        )
        .await
        .unwrap();
    server
        .create_task_data(
            &TaskWriteTarget::Note {
                path: live_path.to_string(),
            },
            "Live task",
        )
        .await
        .unwrap();

    assert_eq!(server.list_resource_descriptors().len(), 8);
    let large_read = server
        .read_note_content("RemediationFixtures/Large.md")
        .await;
    assert!(
        large_read.is_ok() || matches!(large_read, Err(ObsidianMcpError::CliProtocol(_))),
        "large output must complete or fail with the configured capture limit"
    );
    fs::remove_file(live_file).unwrap();
}

fn guarded_real_cli_server() -> ObsidianMcp {
    let expected = PathBuf::from("/Users/lukasz/Desktop/test-vault")
        .canonicalize()
        .unwrap();
    let configured = env::var_os("OBSIDIAN_VAULT_PATH")
        .map(PathBuf::from)
        .expect("OBSIDIAN_VAULT_PATH must be set")
        .canonicalize()
        .unwrap();
    assert_eq!(
        configured, expected,
        "live tests may only target /Users/lukasz/Desktop/test-vault"
    );
    assert_eq!(
        env::var("OBSIDIAN_VAULT_NAME").as_deref(),
        Ok("test-vault"),
        "live tests require OBSIDIAN_VAULT_NAME=test-vault"
    );
    ObsidianMcp::new(configured).unwrap()
}

fn prompt_request<const N: usize>(
    name: &str,
    arguments: [(&str, &str); N],
) -> GetPromptRequestParams {
    let mut values = rmcp::model::JsonObject::new();
    for (key, value) in arguments {
        values.insert(
            key.to_string(),
            rmcp::serde_json::Value::String(value.to_string()),
        );
    }
    GetPromptRequestParams::new(name).with_arguments(values)
}

fn assert_resource_text_contains(result: &ReadResourceResult, expected: &str) {
    match &result.contents[0] {
        ResourceContents::TextResourceContents { text, .. } => {
            assert!(
                text.contains(expected),
                "expected resource text to contain {expected:?}, got {text:?}"
            );
        }
        _ => panic!("expected text resource contents"),
    }
}

fn assert_prompt_text_contains(result: &GetPromptResult, expected: &str) {
    match &result.messages[0].content {
        rmcp::model::PromptMessageContent::Text { text } => {
            assert!(
                text.contains(expected),
                "expected prompt text to contain {expected:?}, got {text:?}"
            );
        }
        _ => panic!("expected text prompt message"),
    }
}

#[derive(Debug, Clone, Default)]
struct FakeObsidianCli {
    calls: Arc<Mutex<Vec<FakeCall>>>,
    responses: Arc<Mutex<VecDeque<AppResult<CliOutput>>>>,
}

impl FakeObsidianCli {
    fn new<const N: usize>(responses: [Result<&str, &str>; N]) -> Self {
        Self {
            calls: Arc::default(),
            responses: Arc::new(Mutex::new(
                responses
                    .into_iter()
                    .map(|result| match result {
                        Ok(output) => Ok(CliOutput::success(output)),
                        Err("missing") => Err(ObsidianMcpError::NoteNotFound(
                            "Error: File not found.".to_string(),
                        )),
                        Err(error) => Err(ObsidianMcpError::CliFailed(error.to_string())),
                    })
                    .collect(),
            )),
        }
    }

    fn calls(&self) -> Vec<FakeCall> {
        self.calls.lock().unwrap().clone()
    }

    fn outputs<const N: usize>(responses: [String; N]) -> Self {
        Self {
            calls: Arc::default(),
            responses: Arc::new(Mutex::new(
                responses
                    .into_iter()
                    .map(CliOutput::success)
                    .map(Ok)
                    .collect(),
            )),
        }
    }

    fn results(responses: Vec<AppResult<CliOutput>>) -> Self {
        Self {
            calls: Arc::default(),
            responses: Arc::new(Mutex::new(responses.into())),
        }
    }
}

fn timeout_error() -> ObsidianMcpError {
    ObsidianMcpError::CliTimeout("timed out".to_string())
}

impl ObsidianCliRunner for FakeObsidianCli {
    fn run<'a>(&'a self, vault: &'a Path, args: Vec<OsString>) -> CliFuture<'a> {
        self.calls.lock().unwrap().push(FakeCall {
            vault: vault.to_path_buf(),
            args: args
                .iter()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
        });
        let response = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                Err(ObsidianMcpError::CliFailed(
                    "missing fake Obsidian CLI response".to_string(),
                ))
            });

        Box::pin(async move { response })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FakeCall {
    vault: PathBuf,
    args: Vec<String>,
}

struct TestVault {
    path: PathBuf,
}

impl TestVault {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let mut path = env::temp_dir();
        path.push(format!(
            "obsidian_mcp_test_{}_{}_{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        let path = path.canonicalize().unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestVault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
