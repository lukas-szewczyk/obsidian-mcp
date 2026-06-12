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
    server
        .validate_vault()
        .await
        .expect("Obsidian CLI must be running with the configured vault registered");
    Some(server)
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
        .read_resource_uri("obsidian://note/Welcome.md")
        .await
        .expect("note resource must resolve through the Obsidian CLI");

    assert_eq!(result.contents.len(), 1);
    let (uri, mime_type, text) = text_contents(&result.contents[0]);
    assert_eq!(uri, "obsidian://note/Welcome.md");
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
