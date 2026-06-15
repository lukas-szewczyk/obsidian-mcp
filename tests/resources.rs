//! Integration tests for MCP resources against a real Obsidian CLI.
//!
//! Requires Obsidian running with the CLI enabled and the test vault registered:
//!   OBSIDIAN_VAULT_NAME="test-vault"
//!   OBSIDIAN_VAULT_PATH="/Users/lukasz/Desktop/obsidian-mcp/test-vault"
//! Tests skip when OBSIDIAN_VAULT_PATH is not set.

use obsidian_mcp::ObsidianMcp;
use rmcp::model::ResourceContents;

async fn connected_server() -> Option<ObsidianMcp> {
    let Some(vault_path) = std::env::var_os("OBSIDIAN_VAULT_PATH") else {
        eprintln!("skipping: OBSIDIAN_VAULT_PATH is not set");
        return None;
    };

    let server =
        ObsidianMcp::new(vault_path).expect("OBSIDIAN_VAULT_PATH must be a vault directory");

    // The CLI sporadically returns empty output when Obsidian is busy; retry
    // validation a few times before failing the whole suite.
    let mut last_error = None;
    for _ in 0..3 {
        match server.validate_vault().await {
            Ok(()) => return Some(server),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    panic!("Obsidian CLI must be running with the configured vault registered: {last_error:?}");
}

fn text_contents(contents: &ResourceContents) -> (&str, Option<&str>, &str) {
    let ResourceContents::TextResourceContents {
        uri,
        mime_type,
        text,
        ..
    } = contents
    else {
        panic!("expected text resource contents, got {contents:?}");
    };
    (uri, mime_type.as_deref(), text)
}

#[tokio::test]
async fn note_resource_reads_markdown_through_obsidian_cli() {
    let Some(server) = connected_server().await else {
        return;
    };

    let result = server
        .read_resource_uri("workos://note/Welcome.md")
        .await
        .expect("note resource must resolve through the Obsidian CLI");

    assert_eq!(result.contents.len(), 1);
    let (uri, mime_type, text) = text_contents(&result.contents[0]);
    assert_eq!(uri, "workos://note/Welcome.md");
    assert_eq!(mime_type, Some("text/markdown"));
    assert!(
        text.contains("This is your new *vault*."),
        "unexpected note content: {text}"
    );
}

#[tokio::test]
async fn workspace_profile_resource_aggregates_vault_state() {
    let Some(server) = connected_server().await else {
        return;
    };

    let result = server
        .read_resource_uri("workos://workspace/profile")
        .await
        .expect("workspace profile resource must resolve through the Obsidian CLI");

    assert_eq!(result.contents.len(), 1);
    let (uri, mime_type, text) = text_contents(&result.contents[0]);
    assert_eq!(uri, "workos://workspace/profile");
    assert_eq!(mime_type, Some("application/json"));

    let profile: rmcp::serde_json::Value =
        rmcp::serde_json::from_str(text).expect("profile must be valid JSON");

    assert_eq!(profile["contract"], "workos.v1");
    assert_eq!(profile["server"]["name"], "obsidian-mcp");
    assert_eq!(profile["vault"]["name"], "test-vault");
    assert!(
        profile["vault"]["files"].as_u64().unwrap_or_default() >= 2,
        "test vault must report its notes: {profile}"
    );
    assert!(
        profile["system"]["obsidian_version"]
            .as_str()
            .is_some_and(|version| !version.is_empty()),
        "Obsidian version must be reported: {profile}"
    );
    assert_eq!(profile["conventions"]["projects_dir"], "Projects");
    assert!(profile["capabilities"]["daily"].is_boolean());
    assert!(profile["capabilities"]["projects"].is_boolean());
    assert!(profile["bases"].is_array());
    assert!(profile["system"]["warnings"].is_array());
}

#[tokio::test]
async fn workspace_today_resource_splits_tasks_by_due_date() {
    let Some(server) = connected_server().await else {
        return;
    };

    let result = server
        .read_resource_uri("workos://workspace/today")
        .await
        .expect("today resource must resolve through the Obsidian CLI");

    assert_eq!(result.contents.len(), 1);
    let (uri, mime_type, text) = text_contents(&result.contents[0]);
    assert_eq!(uri, "workos://workspace/today");
    assert_eq!(mime_type, Some("application/json"));

    let today: rmcp::serde_json::Value =
        rmcp::serde_json::from_str(text).expect("today must be valid JSON");

    assert_eq!(today["contract"], "workos.v1");
    let date = today["date"].as_str().expect("date must be a string");
    assert!(
        today["daily_note"]["path"]
            .as_str()
            .is_some_and(|path| path.contains(date)),
        "daily note path must match the reported date: {today}"
    );
    assert!(today["daily_note"]["exists"].is_boolean());

    // The fixture test-vault/Tasks.md pins one open overdue task.
    let overdue = today["tasks"]["overdue"]
        .as_array()
        .expect("overdue must be an array");
    let invoice = overdue
        .iter()
        .find(|task| task["path"] == "Tasks.md" && task["text"] == "Pay invoice")
        .unwrap_or_else(|| panic!("fixture overdue task must be reported: {today}"));
    assert_eq!(invoice["due"], "2020-01-01");
    assert_eq!(invoice["status"], " ");
    assert_eq!(invoice["raw"], "- [ ] Pay invoice 📅 2020-01-01");

    assert_eq!(
        today["counts"]["overdue"].as_u64().unwrap_or_default(),
        overdue.len() as u64
    );
    assert_eq!(
        today["counts"]["due_today"].as_u64().unwrap_or_default(),
        today["tasks"]["due_today"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default() as u64
    );
    assert!(
        today["counts"]["open_total"].as_u64().unwrap_or_default() >= 2,
        "open fixture tasks must count toward open_total: {today}"
    );
}

async fn read_resource(server: &ObsidianMcp, uri: &str) -> rmcp::model::ReadResourceResult {
    // The Obsidian CLI sporadically returns empty stdout under rapid sequential
    // load, which surfaces as a transient parse/EOF error; retry a few times
    // before failing, mirroring connected_server's validation retry.
    let mut last_error = None;
    for _ in 0..3 {
        match server.read_resource_uri(uri).await {
            Ok(result) => return result,
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    panic!("{uri} must resolve: {last_error:?}");
}

async fn read_json(server: &ObsidianMcp, uri: &str) -> rmcp::serde_json::Value {
    let result = read_resource(server, uri).await;
    let (resolved_uri, mime_type, text) = text_contents(&result.contents[0]);
    assert_eq!(resolved_uri, uri);
    assert_eq!(mime_type, Some("application/json"));
    rmcp::serde_json::from_str(text).unwrap_or_else(|error| panic!("{uri} must be JSON: {error}"))
}

#[tokio::test]
async fn open_and_dated_task_resources_filter_by_due_date() {
    let Some(server) = connected_server().await else {
        return;
    };

    let open = read_json(&server, "workos://tasks/open").await;
    assert_eq!(open["contract"], "workos.v1");
    assert_eq!(open["truncated"], false);
    let tasks = open["tasks"].as_array().expect("tasks must be an array");
    assert_eq!(
        open["count"].as_u64().unwrap_or_default(),
        tasks.len() as u64
    );
    assert!(
        tasks
            .iter()
            .any(|task| task["text"] == "Pay invoice" && task["due"] == "2020-01-01"),
        "fixture task must be listed: {open}"
    );

    let due_on = read_json(&server, "workos://tasks/due-on/2020-01-01").await;
    assert_eq!(due_on["op"], "due_on");
    assert_eq!(due_on["date"], "2020-01-01");
    assert!(
        due_on["tasks"]
            .as_array()
            .is_some_and(|tasks| tasks.iter().any(|task| task["text"] == "Pay invoice")),
        "due-on must match the fixture date exactly: {due_on}"
    );

    let due_before = read_json(&server, "workos://tasks/due-before/2020-01-01").await;
    assert_eq!(due_before["op"], "due_before");
    assert!(
        due_before["tasks"]
            .as_array()
            .is_some_and(|tasks| tasks.iter().all(|task| task["text"] != "Pay invoice")),
        "due-before must be strict: {due_before}"
    );

    let overdue = read_json(&server, "workos://tasks/due-before/2020-01-02").await;
    assert!(
        overdue["tasks"]
            .as_array()
            .is_some_and(|tasks| tasks.iter().any(|task| task["text"] == "Pay invoice")),
        "due-before must include earlier due dates: {overdue}"
    );
}

#[tokio::test]
async fn note_context_resource_aggregates_note_facets() {
    let Some(server) = connected_server().await else {
        return;
    };

    let context = read_json(&server, "workos://note/Tasks.md/context").await;
    assert_eq!(context["contract"], "workos.v1");
    assert_eq!(context["path"], "Tasks.md");
    assert!(
        context["content"]
            .as_str()
            .is_some_and(|content| content.contains("Pay invoice")),
        "context must embed note content: {context}"
    );
    assert!(context["properties"].is_object());
    assert!(context["tags"].is_array());
    assert!(context["links"].is_array());
    assert!(context["backlinks"].is_array());
    let tasks = context["tasks"].as_array().expect("tasks must be an array");
    assert!(
        tasks.len() >= 3,
        "context tasks must include done tasks too: {context}"
    );
    assert!(
        tasks
            .iter()
            .any(|task| task["status"] == "x" && task["text"] == "Archived chore"),
        "completed fixture task must appear in context: {context}"
    );
}

#[tokio::test]
async fn project_status_resource_reports_compact_status() {
    let Some(server) = connected_server().await else {
        return;
    };

    let status = read_json(&server, "workos://project/Tasks.md/status").await;
    assert_eq!(status["contract"], "workos.v1");
    assert_eq!(status["path"], "Tasks.md");
    assert_eq!(status["title"], "Tasks");
    assert!(status["properties"].is_object());
    let open_tasks = status["open_tasks"]
        .as_array()
        .expect("open_tasks must be an array");
    assert_eq!(
        status["open_task_count"].as_u64().unwrap_or_default(),
        open_tasks.len() as u64
    );
    assert!(
        open_tasks.iter().all(|task| task["status"] == " "),
        "completed tasks must be excluded: {status}"
    );
    assert!(status["backlink_count"].is_u64());
}

#[tokio::test]
async fn index_and_audit_resources_render() {
    let Some(server) = connected_server().await else {
        return;
    };

    let result = server
        .read_resource_uri("workos://notes/index")
        .await
        .expect("notes index must resolve");
    let (_, mime_type, text) = text_contents(&result.contents[0]);
    assert_eq!(mime_type, Some("text/plain"));
    assert!(text.lines().any(|line| line == "Welcome.md"));
    assert!(text.lines().any(|line| line == "Tasks.md"));

    let result = server
        .read_resource_uri("workos://tags/index")
        .await
        .expect("tags index must resolve");
    let (_, mime_type, _) = text_contents(&result.contents[0]);
    assert_eq!(mime_type, Some("text/plain"));

    let projects = read_json(&server, "workos://projects/index").await;
    assert_eq!(projects["contract"], "workos.v1");
    assert!(
        projects["projects"]
            .as_array()
            .is_some_and(|projects| projects.iter().any(|project| {
                project["path"] == "Projects/WorkOS MCP.md" && project["title"] == "WorkOS MCP"
            })),
        "seeded project must be indexed: {projects}"
    );

    let audit = read_json(&server, "workos://vault/audit").await;
    assert_eq!(audit["contract"], "workos.v1");
    assert!(audit["unresolved"].is_array());
    assert!(audit["orphans"].is_array());
    assert!(audit["deadends"].is_array());
    assert_eq!(audit["truncated"], false);
}

#[tokio::test]
async fn base_template_queries_project_properties() {
    let Some(server) = connected_server().await else {
        return;
    };

    let base = read_json(&server, "workos://base/Projects.base").await;
    assert_eq!(base["contract"], "workos.v1");
    assert_eq!(base["path"], "Projects.base");
    assert_eq!(base["truncated"], false);
    let results = base["results"]
        .as_array()
        .expect("results must be an array");
    assert_eq!(
        base["count"].as_u64().unwrap_or_default(),
        results.len() as u64
    );
    assert!(
        results
            .iter()
            .any(|row| row["path"] == "Projects/WorkOS MCP.md" && row["status"] == "active"),
        "base view must expose project properties: {base}"
    );
}

#[tokio::test]
async fn daily_template_reads_existing_daily_note() {
    let Some(server) = connected_server().await else {
        return;
    };

    let result = server
        .read_resource_uri("workos://daily/2026-06-12")
        .await
        .expect("existing daily note must resolve");
    let (uri, mime_type, text) = text_contents(&result.contents[0]);
    assert_eq!(uri, "workos://daily/2026-06-12");
    assert_eq!(mime_type, Some("text/markdown"));
    assert!(!text.is_empty());

    let missing = server.read_resource_uri("workos://daily/1999-01-01").await;
    assert!(missing.is_err(), "missing daily note must be an error");
}
