use std::{
    collections::VecDeque,
    ffi::OsString,
    fs,
    sync::{Arc, Mutex},
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
    let names = server
        .tool_router
        .list_all()
        .into_iter()
        .map(|tool| tool.name.to_string())
        .collect::<Vec<_>>();

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
        "get_note_context",
        "audit_vault",
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
async fn uses_cli_for_work_system_tasks_daily_range_and_projects() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok(" \t- [ ] Review inbox\tTodo.md\t4\n"),
        Ok("appended"),
        Ok("daily appended"),
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
            .is_some_and(|error| error == "missing")
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
            vec!["append", "path=Todo.md", "content=- [ ] Review inbox"],
            vec!["daily:append", "content=- [ ] Daily follow up"],
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
async fn vault_info_uses_cli_metadata_and_total_count() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("name\tKnowledge\npath\t/Users/example/Vault\nfiles\t57\nfolders\t8\nsize\t1234\n"),
        Ok("Markdown files: 42\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let info = server.vault_info_data().await.unwrap();

    assert_eq!(
        info,
        VaultInfoResponse {
            configured_vault_path: vault.path().display().to_string(),
            obsidian_vault_path: "/Users/example/Vault".to_string(),
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

#[tokio::test]
async fn resource_descriptors_include_static_resources_and_notes() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([Ok("Projects/Rust.md\nSpace Note.md\nimage.png\n")]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let resources = server.list_resource_descriptors().await.unwrap();
    let uris = resources
        .iter()
        .map(|resource| resource.uri.as_str())
        .collect::<Vec<_>>();

    assert!(uris.contains(&"obsidian://vault/info"));
    assert!(uris.contains(&"obsidian://vault/audit"));
    assert!(uris.contains(&"obsidian://notes/index"));
    assert!(uris.contains(&"obsidian://tags/index"));
    assert!(uris.contains(&"obsidian://daily/today"));
    assert!(uris.contains(&"obsidian://tasks/open"));
    assert!(uris.contains(&"obsidian://projects/index"));
    assert!(uris.contains(&"obsidian://note/Projects/Rust.md"));
    assert!(uris.contains(&"obsidian://note/Space%20Note.md"));
    assert!(uris.contains(&"obsidian://backlinks/Projects/Rust.md"));
    assert!(uris.contains(&"obsidian://context/Projects/Rust.md"));
    assert!(!uris.iter().any(|uri| uri.contains("image.png")));
}

#[test]
fn resource_templates_expose_note_uri_template() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::default();
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let templates = server.list_resource_template_descriptors();

    assert_eq!(templates.len(), 7);
    assert_eq!(templates[0].uri_template, "obsidian://note/{path}");
    assert_eq!(templates[0].mime_type.as_deref(), Some("text/markdown"));
    assert_eq!(templates[1].uri_template, "obsidian://backlinks/{path}");
    assert_eq!(templates[1].mime_type.as_deref(), Some("text/plain"));
    assert_eq!(templates[2].uri_template, "obsidian://context/{path}");
    assert_eq!(templates[2].mime_type.as_deref(), Some("application/json"));
    assert_eq!(templates[3].uri_template, "obsidian://daily/{date}");
    assert_eq!(templates[3].mime_type.as_deref(), Some("text/markdown"));
    assert_eq!(templates[4].uri_template, "obsidian://tasks/overdue/{date}");
    assert_eq!(templates[4].mime_type.as_deref(), Some("application/json"));
    assert_eq!(templates[5].uri_template, "obsidian://project/{path}");
    assert_eq!(templates[6].uri_template, "obsidian://properties/{path}");
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
    let cli = FakeObsidianCli::new([
        Ok("name\tKnowledge\npath\t/Users/example/Vault\nfiles\t57\n"),
        Ok("42\n"),
        Ok("Projects/Rust.md\nSpace Note.md\n"),
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
async fn read_tag_daily_and_backlink_resources() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("#rust\t3\n#mcp\t2\n"),
        Ok("# Daily\n"),
        Ok("Ideas/MCP.md\t2\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

    let tags = server
        .read_resource_uri("obsidian://tags/index")
        .await
        .unwrap();
    let daily = server
        .read_resource_uri("obsidian://daily/today")
        .await
        .unwrap();
    let backlinks = server
        .read_resource_uri("obsidian://backlinks/Projects/Rust.md")
        .await
        .unwrap();

    assert_resource_text_contains(&tags, "#rust\t3");
    assert_resource_text_contains(&daily, "# Daily");
    assert_resource_text_contains(&backlinks, "Ideas/MCP.md\t2");
    assert_eq!(
        cli.calls()
            .iter()
            .map(|call| call.args.iter().map(String::as_str).collect::<Vec<_>>())
            .collect::<Vec<_>>(),
        vec![
            vec!["tags", "counts", "sort=count"],
            vec!["daily:read"],
            vec!["backlinks", "path=Projects/Rust.md", "counts"],
        ]
    );
}

#[tokio::test]
async fn read_knowledge_graph_resources() {
    let vault = TestVault::new();
    let cli = FakeObsidianCli::new([
        Ok("Alias\n"),
        Ok("# Heading\n"),
        Ok("Start.md\n"),
        Ok("Start.md\t1\n"),
        Ok("Missing Guide\t1\tStart.md\n"),
        Ok("Knowledge/Orphan.md\n"),
        Ok("Knowledge/Dead End.md\n"),
    ]);
    let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

    let context = server
        .read_resource_uri("obsidian://context/Projects/Rust.md")
        .await
        .unwrap();
    let audit = server
        .read_resource_uri("obsidian://vault/audit")
        .await
        .unwrap();

    assert_resource_text_contains(&context, r#""alias_count": 1"#);
    assert_resource_text_contains(&context, r#""outgoing_links": ["#);
    assert_resource_text_contains(&audit, r#""link": "Missing Guide""#);
    assert_resource_text_contains(&audit, r#""orphan_notes": ["#);
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
fn resource_uri_round_trips_percent_encoded_note_paths() {
    let path = VaultRelativePath::markdown("Folder/Space Note.md").unwrap();
    let uri = ObsidianResourceUri::note(&path);

    assert_eq!(uri, "obsidian://note/Folder/Space%20Note.md");
    assert_eq!(
        ObsidianResourceUri::parse(&uri).unwrap(),
        ObsidianResourceUri::Note(path)
    );
    assert_eq!(
        ObsidianResourceUri::parse("obsidian://project/Folder/Space%20Note.md").unwrap(),
        ObsidianResourceUri::Project(VaultRelativePath::markdown("Folder/Space Note.md").unwrap())
    );
    assert_eq!(
        ObsidianResourceUri::parse("obsidian://properties/Folder/Space%20Note.md").unwrap(),
        ObsidianResourceUri::Properties(
            VaultRelativePath::markdown("Folder/Space Note.md").unwrap()
        )
    );
    assert_eq!(
        ObsidianResourceUri::parse("obsidian://context/Folder/Space%20Note.md").unwrap(),
        ObsidianResourceUri::Context(VaultRelativePath::markdown("Folder/Space Note.md").unwrap())
    );
    assert_eq!(
        ObsidianResourceUri::parse("obsidian://vault/audit").unwrap(),
        ObsidianResourceUri::VaultAudit
    );
    assert_eq!(
        ObsidianResourceUri::parse("obsidian://tasks/overdue/2026-06-05").unwrap(),
        ObsidianResourceUri::TasksOverdue(DailyDate::parse("2026-06-05").unwrap())
    );
    assert!(ObsidianResourceUri::parse("obsidian://note/bad%").is_err());
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
            "daily_review",
            "plan_day",
            "tag_overview",
            "backlink_review",
            "weekly_review",
            "project_review",
            "inbox_triage",
            "vault_audit"
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
    assert_prompt_text_contains(&backlinks, "obsidian://backlinks/Projects/Rust.md");

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

    let inbox = server
        .get_prompt_result(prompt_request("inbox_triage", [("directory", "Inbox")]))
        .unwrap();
    assert_prompt_text_contains(&inbox, "list_notes");

    let audit = server
        .get_prompt_result(GetPromptRequestParams::new("vault_audit"))
        .unwrap();
    assert_prompt_text_contains(&audit, "obsidian://vault/audit");
    assert_prompt_text_contains(&audit, "get_note_context");
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
        Ok("Projects/Rust.md\n"),
        Ok(" \t- [ ] Review release\tTodo.md\t4\n"),
        Ok(" \t- [ ] Past due 📅 2026-06-01\tTodo.md\t4\n"),
        Ok("Missing Guide\t1\tStart.md\n"),
        Ok("Knowledge/Orphan.md\n"),
        Ok("Knowledge/Dead End.md\n"),
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
    let resources = client.peer().list_all_resources().await.unwrap();
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
    assert!(tools.iter().any(|tool| tool.name == "get_note_context"));
    assert!(tools.iter().any(|tool| tool.name == "audit_vault"));
    assert!(
        resources
            .iter()
            .any(|resource| resource.uri == "obsidian://tasks/open")
    );
    assert!(
        resources
            .iter()
            .any(|resource| resource.uri == "obsidian://vault/audit")
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
    assert!(
        templates
            .iter()
            .any(|template| template.uri_template == "obsidian://context/{path}")
    );
    assert!(prompts.iter().any(|prompt| prompt.name == "weekly_review"));
    assert!(prompts.iter().any(|prompt| prompt.name == "plan_day"));
    assert!(prompts.iter().any(|prompt| prompt.name == "vault_audit"));
    assert!(!task_result.is_error.unwrap_or(false));
    assert!(!overdue_result.is_error.unwrap_or(false));
    assert!(!audit_result.is_error.unwrap_or(false));
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
    let vault = env::var_os("OBSIDIAN_VAULT_PATH").expect("OBSIDIAN_VAULT_PATH must be set");
    let server = ObsidianMcp::new(PathBuf::from(vault)).unwrap();

    server.vault_info_data().await.unwrap();
}

#[tokio::test]
#[ignore = "requires Obsidian to be running with CLI enabled and OBSIDIAN_VAULT_PATH set"]
async fn real_cli_smoke_work_system_reads() {
    let vault = env::var_os("OBSIDIAN_VAULT_PATH").expect("OBSIDIAN_VAULT_PATH must be set");
    let note_path = env::var("OBSIDIAN_SMOKE_NOTE_PATH").unwrap_or_else(|_| "Todo.md".to_string());
    let server = ObsidianMcp::new(PathBuf::from(vault)).unwrap();

    server.list_properties_data(&note_path).await.unwrap();
    server
        .list_overdue_tasks_data("2026-06-05", &TaskReadTarget::Vault, Some(10))
        .await
        .unwrap();
}

#[tokio::test]
#[ignore = "requires Obsidian to be running with CLI enabled and OBSIDIAN_VAULT_PATH set"]
async fn real_cli_smoke_knowledge_graph_reads() {
    let vault = env::var_os("OBSIDIAN_VAULT_PATH").expect("OBSIDIAN_VAULT_PATH must be set");
    let note_path = env::var("OBSIDIAN_SMOKE_NOTE_PATH").unwrap_or_else(|_| "Todo.md".to_string());
    let server = ObsidianMcp::new(PathBuf::from(vault)).unwrap();

    server
        .get_note_context_data(&note_path, Some(10))
        .await
        .unwrap();
    server.audit_vault_data(Some(10)).await.unwrap();
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
    responses: Arc<Mutex<VecDeque<AppResult<String>>>>,
}

impl FakeObsidianCli {
    fn new<const N: usize>(responses: [Result<&str, &str>; N]) -> Self {
        Self {
            calls: Arc::default(),
            responses: Arc::new(Mutex::new(
                responses
                    .into_iter()
                    .map(|result| result.map(str::to_string).map_err(str::to_string))
                    .map(|result| result.map_err(ObsidianMcpError::CliFailed))
                    .collect(),
            )),
        }
    }

    fn calls(&self) -> Vec<FakeCall> {
        self.calls.lock().unwrap().clone()
    }
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
        let mut path = env::temp_dir();
        path.push(format!(
            "obsidian_mcp_test_{}_{}",
            std::process::id(),
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
