use rmcp::schemars;

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListNotesRequest {
    pub directory: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ReadNoteRequest {
    pub path: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct CreateNoteRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ReplaceNoteRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct AppendNoteRequest {
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
    pub path: String,
    pub counts: Option<bool>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct GetNoteContextRequest {
    pub path: String,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct AuditVaultRequest {
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct AppendDailyNoteRequest {
    pub content: String,
    pub inline: Option<bool>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ReadDailyNotesRequest {
    pub from: String,
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
    pub path: String,
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
    pub path: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct SetPropertyRequest {
    pub path: String,
    pub name: String,
    pub value: String,
    pub property_type: Option<PropertyType>,
    pub preview: Option<bool>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListOverdueTasksRequest {
    pub as_of: String,
    pub target: Option<TaskReadTarget>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct ListTasksByProjectRequest {
    pub path: String,
    pub status: Option<TaskStatus>,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct GetProjectStatusRequest {
    pub path: String,
    pub limit: Option<usize>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct PreviewNoteChangeRequest {
    pub path: String,
    pub mode: NoteChangeMode,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct VaultInfoResponse {
    pub configured_vault_path: String,
    pub obsidian_vault_path: String,
    pub obsidian_vault_name: String,
    pub markdown_notes: usize,
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
    Note { path: String },
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
    Custom { value: String },
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
