use rmcp::schemars;

/// Schema for an arbitrary JSON value as a top-level output property.
///
/// `serde_json::Value` derives the boolean schema `true`, which MCP clients
/// reject when it appears directly under `outputSchema.properties` (they expect
/// an object-form schema). An empty object schema accepts any value, including
/// `null`, while remaining valid JSON Schema.
fn any_json_value_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({})
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListNotesRequest {
    /// Optional vault-relative directory. Absolute paths and traversal are rejected.
    pub directory: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ReadNoteRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct CreateNoteRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub content: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ReplaceNoteRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub content: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct AppendNoteRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub content: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct SearchNotesRequest {
    pub query: String,
    pub directory: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListTagsRequest {
    pub path: Option<String>,
    pub counts: Option<bool>,
    pub sort_by_count: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListBacklinksRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub counts: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct GetNoteContextRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct AuditVaultRequest {
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListBasesRequest {
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct QueryBaseRequest {
    /// Vault-relative `.base` path. Absolute paths and traversal are rejected.
    pub path: String,
    pub view: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct CreateBaseItemRequest {
    /// Vault-relative `.base` path. Absolute paths and traversal are rejected.
    pub path: String,
    pub view: String,
    pub name: String,
    pub content: Option<String>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct AppendDailyNoteRequest {
    pub content: String,
    pub inline: Option<bool>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ReadDailyNotesRequest {
    /// Inclusive start date in `YYYY-MM-DD` format.
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    pub from: String,
    /// Inclusive end date in `YYYY-MM-DD` format.
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    pub to: String,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListTasksRequest {
    pub target: Option<TaskReadTarget>,
    pub status: Option<TaskStatus>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct CreateTaskRequest {
    pub target: TaskWriteTarget,
    pub text: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct SetTaskStatusRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    /// Positive one-based line number.
    #[schemars(range(min = 1))]
    pub line: usize,
    pub status: TaskStatus,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListProjectsRequest {
    pub directory: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListPropertiesRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct SetPropertyRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub name: String,
    pub value: String,
    pub property_type: Option<PropertyType>,
    pub preview: Option<bool>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListOverdueTasksRequest {
    /// Deterministic comparison date in `YYYY-MM-DD` format.
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    pub as_of: String,
    pub target: Option<TaskReadTarget>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListTasksByProjectRequest {
    /// Vault-relative Markdown project path. Absolute paths and traversal are rejected.
    pub path: String,
    pub status: Option<TaskStatus>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct GetProjectStatusRequest {
    /// Vault-relative Markdown project path. Absolute paths and traversal are rejected.
    pub path: String,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct PreviewNoteChangeRequest {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub mode: NoteChangeMode,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ChangeSetOperation {
    /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
    pub path: String,
    pub mode: NoteChangeMode,
    pub content: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct PreviewChangeSetRequest {
    /// One to fifty ordered note changes.
    #[schemars(length(min = 1, max = 50))]
    pub changes: Vec<ChangeSetOperation>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ApplyChangeSetRequest {
    /// One to fifty ordered note changes.
    #[schemars(length(min = 1, max = 50))]
    pub changes: Vec<ChangeSetOperation>,
    pub preview_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct VaultInfoResponse {
    pub configured_vault_path: String,
    pub obsidian_vault_path: String,
    pub obsidian_vault_name: String,
    pub markdown_notes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ProfileServer {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ProfileVault {
    pub name: String,
    pub path: String,
    pub files: usize,
    pub folders: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ProfileSync {
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ProfileConventions {
    pub projects_dir: String,
    pub daily_path_format: String,
    pub task_date_syntax: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ProfileCapabilities {
    pub projects: bool,
    pub daily: bool,
    pub bases: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ProfileSystem {
    pub obsidian_version: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct WorkspaceProfileResponse {
    pub contract: String,
    pub server: ProfileServer,
    pub vault: ProfileVault,
    pub sync: ProfileSync,
    pub conventions: ProfileConventions,
    pub bases: Vec<String>,
    pub capabilities: ProfileCapabilities,
    pub system: ProfileSystem,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListNotesResponse {
    pub directory: Option<String>,
    pub notes: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ReadNoteResponse {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct CreateNoteResponse {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ReplaceNoteResponse {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct AppendNoteResponse {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct SearchNotesResponse {
    pub query: String,
    pub directory: Option<String>,
    pub matches: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListTagsResponse {
    pub path: Option<String>,
    pub tags: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListBacklinksResponse {
    pub path: String,
    pub backlinks: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct NoteContextResponse {
    pub path: String,
    pub aliases: Vec<String>,
    pub outline: Vec<String>,
    pub outgoing_links: Vec<String>,
    pub backlinks: Vec<String>,
    pub alias_count: usize,
    pub outline_count: usize,
    pub outgoing_link_count: usize,
    pub backlink_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct UnresolvedLinkItem {
    pub link: String,
    pub count: usize,
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct VaultAuditResponse {
    pub unresolved_links: Vec<UnresolvedLinkItem>,
    pub orphan_notes: Vec<String>,
    pub dead_ends: Vec<String>,
    pub unresolved_link_count: usize,
    pub orphan_note_count: usize,
    pub dead_end_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListBasesResponse {
    pub bases: Vec<String>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct QueryBaseResponse {
    pub path: String,
    pub view: Option<String>,
    pub results: Vec<rmcp::serde_json::Value>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct CreateBaseItemResponse {
    pub path: String,
    pub view: String,
    pub name: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ReadDailyNoteResponse {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct AppendDailyNoteResponse {
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct DailyNoteEntry {
    pub date: String,
    pub path: String,
    pub content: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ReadDailyNotesResponse {
    pub from: String,
    pub to: String,
    pub notes: Vec<DailyNoteEntry>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct TaskItem {
    pub status: String,
    pub text: String,
    pub path: String,
    pub line: usize,
}

#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Eq,
    rmcp::serde::Deserialize,
    rmcp::serde::Serialize,
    schemars::JsonSchema,
)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskReadTarget {
    #[default]
    Vault,
    Daily,
    Note {
        /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
        path: String,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rmcp::serde::Deserialize,
    rmcp::serde::Serialize,
    schemars::JsonSchema,
)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskWriteTarget {
    Daily,
    Note {
        /// Vault-relative Markdown path. Absolute paths and traversal are rejected.
        path: String,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rmcp::serde::Deserialize,
    rmcp::serde::Serialize,
    schemars::JsonSchema,
)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    Done,
    Custom {
        /// Exactly one task status character.
        #[schemars(length(equal = 1))]
        value: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListTasksResponse {
    pub target: TaskReadTarget,
    pub status: Option<TaskStatus>,
    pub tasks: Vec<TaskItem>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct CreateTaskResponse {
    pub target: String,
    pub task: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct SetTaskStatusResponse {
    pub path: String,
    pub line: usize,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListProjectsResponse {
    pub directory: String,
    pub projects: Vec<String>,
    pub count: usize,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rmcp::serde::Deserialize,
    rmcp::serde::Serialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum PropertyType {
    Text,
    List,
    Number,
    Checkbox,
    Date,
    Datetime,
}

#[derive(Debug, Clone, PartialEq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct NoteProperty {
    pub name: String,
    pub value: rmcp::serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListPropertiesResponse {
    pub path: String,
    pub properties: Vec<NoteProperty>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct SetPropertyResponse {
    pub path: String,
    pub name: String,
    pub value: String,
    pub property_type: Option<PropertyType>,
    #[schemars(schema_with = "any_json_value_schema")]
    pub previous_value: Option<rmcp::serde_json::Value>,
    pub applied: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct OverdueTaskItem {
    pub due_date: String,
    pub status: String,
    pub text: String,
    pub path: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListOverdueTasksResponse {
    pub as_of: String,
    pub target: TaskReadTarget,
    pub tasks: Vec<OverdueTaskItem>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ListTasksByProjectResponse {
    pub path: String,
    pub status: Option<TaskStatus>,
    pub tasks: Vec<TaskItem>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ProjectStatusResponse {
    pub path: String,
    pub content: String,
    pub properties: Vec<NoteProperty>,
    pub open_tasks: Vec<TaskItem>,
    pub completed_tasks: Vec<TaskItem>,
    pub backlinks: Vec<String>,
    pub open_task_count: usize,
    pub completed_task_count: usize,
    pub backlink_count: usize,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    rmcp::serde::Deserialize,
    rmcp::serde::Serialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum NoteChangeMode {
    Create,
    Replace,
    Append,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct PreviewNoteChangeResponse {
    pub path: String,
    pub mode: NoteChangeMode,
    pub exists: bool,
    pub current_content: Option<String>,
    pub proposed_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct PreviewChangeSetItem {
    pub index: usize,
    pub path: String,
    pub mode: NoteChangeMode,
    pub exists: bool,
    pub current_content: Option<String>,
    pub proposed_content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct PreviewChangeSetResponse {
    pub preview_token: String,
    pub changes: Vec<PreviewChangeSetItem>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSetApplyOutcome {
    Applied,
    Conflict,
    PartialFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct AppliedChangeSetItem {
    pub index: usize,
    pub path: String,
    pub mode: NoteChangeMode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct FailedChangeSetItem {
    pub index: usize,
    pub path: String,
    pub mode: NoteChangeMode,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct ApplyChangeSetResponse {
    pub outcome: ChangeSetApplyOutcome,
    pub expected_preview_token: String,
    pub observed_preview_token: String,
    pub applied: Vec<AppliedChangeSetItem>,
    pub failed: Option<FailedChangeSetItem>,
    pub skipped: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_task_models_deserialize_tagged_inputs() {
        let target: TaskReadTarget = rmcp::serde_json::from_value(rmcp::serde_json::json!({
            "type": "note",
            "path": "Todo.md"
        }))
        .unwrap();
        let status: TaskStatus = rmcp::serde_json::from_value(rmcp::serde_json::json!({
            "type": "custom",
            "value": "-"
        }))
        .unwrap();

        assert_eq!(
            target,
            TaskReadTarget::Note {
                path: "Todo.md".to_string()
            }
        );
        assert_eq!(
            status,
            TaskStatus::Custom {
                value: "-".to_string()
            }
        );
        assert!(
            rmcp::serde_json::from_value::<TaskWriteTarget>(
                rmcp::serde_json::json!({"type": "vault"})
            )
            .is_err()
        );

        let property_type: PropertyType =
            rmcp::serde_json::from_value(rmcp::serde_json::json!("date")).unwrap();
        let change_mode: NoteChangeMode =
            rmcp::serde_json::from_value(rmcp::serde_json::json!("append")).unwrap();
        assert_eq!(property_type, PropertyType::Date);
        assert_eq!(change_mode, NoteChangeMode::Append);
    }
}
