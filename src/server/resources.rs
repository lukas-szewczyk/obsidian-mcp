use std::{fmt, str::FromStr};

use rmcp::model::{
    AnnotateAble, RawResource, RawResourceTemplate, ReadResourceResult, Resource, ResourceContents,
    ResourceTemplate,
};

use super::{workspace::DueDateFilter, *};

const NOTES_INDEX_LIMIT: usize = 5_000;
const TAGS_INDEX_LIMIT: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ObsidianResourceUri {
    WorkspaceProfile,
    Today,
    TasksOpen,
    ProjectsIndex,
    NotesIndex,
    TagsIndex,
    VaultAudit,
    Note(VaultRelativePath),
    NoteContext(VaultRelativePath),
    Daily(DailyDate),
    Base(VaultRelativePath),
    TasksDueOn(DailyDate),
    TasksDueBefore(DailyDate),
    ProjectStatus(VaultRelativePath),
}

impl ObsidianResourceUri {
    const WORKSPACE_PROFILE: &'static str = "workos://workspace/profile";
    const TODAY: &'static str = "workos://workspace/today";
    const TASKS_OPEN: &'static str = "workos://tasks/open";
    const PROJECTS_INDEX: &'static str = "workos://projects/index";
    const NOTES_INDEX: &'static str = "workos://notes/index";
    const TAGS_INDEX: &'static str = "workos://tags/index";
    const VAULT_AUDIT: &'static str = "workos://vault/audit";
    const NOTE_PREFIX: &'static str = "workos://note/";
    const NOTE_CONTEXT_SUFFIX: &'static str = "/context";
    const DAILY_PREFIX: &'static str = "workos://daily/";
    const BASE_PREFIX: &'static str = "workos://base/";
    const TASKS_DUE_ON_PREFIX: &'static str = "workos://tasks/due-on/";
    const TASKS_DUE_BEFORE_PREFIX: &'static str = "workos://tasks/due-before/";
    const PROJECT_PREFIX: &'static str = "workos://project/";
    const PROJECT_STATUS_SUFFIX: &'static str = "/status";
}

impl FromStr for ObsidianResourceUri {
    type Err = ObsidianMcpError;

    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        match uri {
            Self::WORKSPACE_PROFILE => Ok(Self::WorkspaceProfile),
            Self::TODAY => Ok(Self::Today),
            Self::TASKS_OPEN => Ok(Self::TasksOpen),
            Self::PROJECTS_INDEX => Ok(Self::ProjectsIndex),
            Self::NOTES_INDEX => Ok(Self::NotesIndex),
            Self::TAGS_INDEX => Ok(Self::TagsIndex),
            Self::VAULT_AUDIT => Ok(Self::VaultAudit),
            _ => {
                if let Some(encoded_path) = uri.strip_prefix(Self::NOTE_PREFIX) {
                    if let Some(encoded_path) = encoded_path.strip_suffix(Self::NOTE_CONTEXT_SUFFIX)
                    {
                        let decoded_path = percent_decode_uri_path(encoded_path)?;
                        Ok(Self::NoteContext(VaultRelativePath::markdown(
                            &decoded_path,
                        )?))
                    } else {
                        let decoded_path = percent_decode_uri_path(encoded_path)?;
                        Ok(Self::Note(VaultRelativePath::markdown(&decoded_path)?))
                    }
                } else if let Some(encoded_path) = uri.strip_prefix(Self::BASE_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Base(VaultRelativePath::base(&decoded_path)?))
                } else if let Some(encoded_path) = uri.strip_prefix(Self::PROJECT_PREFIX) {
                    let encoded_path = encoded_path
                        .strip_suffix(Self::PROJECT_STATUS_SUFFIX)
                        .ok_or_else(|| {
                            ObsidianMcpError::ResourceNotFound(format!(
                                "Project resources require the /status facet: {uri}"
                            ))
                        })?;
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::ProjectStatus(VaultRelativePath::markdown(
                        &decoded_path,
                    )?))
                } else if let Some(date) = uri.strip_prefix(Self::TASKS_DUE_ON_PREFIX) {
                    Ok(Self::TasksDueOn(DailyDate::parse(date)?))
                } else if let Some(date) = uri.strip_prefix(Self::TASKS_DUE_BEFORE_PREFIX) {
                    Ok(Self::TasksDueBefore(DailyDate::parse(date)?))
                } else if let Some(date) = uri.strip_prefix(Self::DAILY_PREFIX) {
                    Ok(Self::Daily(DailyDate::parse(date)?))
                } else {
                    Err(ObsidianMcpError::ResourceNotFound(format!(
                        "Unsupported WorkOS resource URI: {uri}"
                    )))
                }
            }
        }
    }
}

impl fmt::Display for ObsidianResourceUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkspaceProfile => formatter.write_str(Self::WORKSPACE_PROFILE),
            Self::Today => formatter.write_str(Self::TODAY),
            Self::TasksOpen => formatter.write_str(Self::TASKS_OPEN),
            Self::ProjectsIndex => formatter.write_str(Self::PROJECTS_INDEX),
            Self::NotesIndex => formatter.write_str(Self::NOTES_INDEX),
            Self::TagsIndex => formatter.write_str(Self::TAGS_INDEX),
            Self::VaultAudit => formatter.write_str(Self::VAULT_AUDIT),
            Self::Note(path) => write_resource_path(formatter, Self::NOTE_PREFIX, path, ""),
            Self::NoteContext(path) => write_resource_path(
                formatter,
                Self::NOTE_PREFIX,
                path,
                Self::NOTE_CONTEXT_SUFFIX,
            ),
            Self::Daily(date) => write!(formatter, "{}{date}", Self::DAILY_PREFIX),
            Self::Base(path) => write_resource_path(formatter, Self::BASE_PREFIX, path, ""),
            Self::TasksDueOn(date) => {
                write!(formatter, "{}{date}", Self::TASKS_DUE_ON_PREFIX)
            }
            Self::TasksDueBefore(date) => {
                write!(formatter, "{}{date}", Self::TASKS_DUE_BEFORE_PREFIX)
            }
            Self::ProjectStatus(path) => write_resource_path(
                formatter,
                Self::PROJECT_PREFIX,
                path,
                Self::PROJECT_STATUS_SUFFIX,
            ),
        }
    }
}

impl ObsidianMcp {
    pub fn list_resource_descriptors(&self) -> Vec<Resource> {
        vec![
            workspace_profile_resource(),
            workspace_today_resource(),
            tasks_open_resource(),
            projects_index_resource(),
            notes_index_resource(),
            tags_index_resource(),
            vault_audit_resource(),
        ]
    }

    pub fn list_resource_template_descriptors(&self) -> Vec<ResourceTemplate> {
        vec![
            RawResourceTemplate::new("workos://note/{path}", "workos_note")
                .with_title("Note")
                .with_description("Raw Markdown note by vault-relative path.")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResourceTemplate::new("workos://note/{path}/context", "workos_note_context")
                .with_title("Note context")
                .with_description(
                    "Full structured context for one note: content, properties, tags, tasks, links, and backlinks.",
                )
                .with_mime_type("application/json")
                .no_annotation(),
            RawResourceTemplate::new("workos://daily/{date}", "workos_daily")
                .with_title("Daily note")
                .with_description("Raw daily note for a YYYY-MM-DD date.")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResourceTemplate::new("workos://base/{path}", "workos_base")
                .with_title("Base query")
                .with_description(
                    "Query the default view of an Obsidian Base; the batch mechanism for property queries.",
                )
                .with_mime_type("application/json")
                .no_annotation(),
            RawResourceTemplate::new("workos://tasks/due-on/{date}", "workos_tasks_due_on")
                .with_title("Tasks due on date")
                .with_description("Open tasks due exactly on a YYYY-MM-DD date.")
                .with_mime_type("application/json")
                .no_annotation(),
            RawResourceTemplate::new(
                "workos://tasks/due-before/{date}",
                "workos_tasks_due_before",
            )
            .with_title("Tasks due before date")
            .with_description("Open tasks due strictly before a YYYY-MM-DD date (overdue as-of).")
            .with_mime_type("application/json")
            .no_annotation(),
            RawResourceTemplate::new("workos://project/{path}/status", "workos_project_status")
                .with_title("Project status")
                .with_description(
                    "Compact project status: properties, open tasks, and backlink count.",
                )
                .with_mime_type("application/json")
                .no_annotation(),
        ]
    }

    pub async fn read_resource_uri(&self, uri: &str) -> AppResult<ReadResourceResult> {
        let resource_uri = uri.parse::<ObsidianResourceUri>()?;
        let normalized_uri = resource_uri.to_string();
        let contents = match resource_uri {
            ObsidianResourceUri::WorkspaceProfile => {
                let profile = self.workspace_profile_data().await?;
                json_resource(&profile, normalized_uri)?
            }
            ObsidianResourceUri::Today => {
                let today = self.workspace_today_data().await?;
                json_resource(&today, normalized_uri)?
            }
            ObsidianResourceUri::TasksOpen => {
                let tasks = self.open_tasks_resource_data().await?;
                json_resource(&tasks, normalized_uri)?
            }
            ObsidianResourceUri::ProjectsIndex => {
                let projects = self.projects_index_resource_data().await?;
                json_resource(&projects, normalized_uri)?
            }
            ObsidianResourceUri::NotesIndex => {
                let notes = self.discover_note_paths(None).await?;
                text_resource(
                    plain_index(notes, NOTES_INDEX_LIMIT),
                    normalized_uri,
                    "text/plain",
                )
            }
            ObsidianResourceUri::TagsIndex => {
                let tags = self
                    .scan_tags_data(None, true, true)
                    .await?
                    .into_iter()
                    .map(|line| {
                        if line.starts_with('#') {
                            line
                        } else {
                            format!("#{line}")
                        }
                    })
                    .collect();
                text_resource(
                    plain_index(tags, TAGS_INDEX_LIMIT),
                    normalized_uri,
                    "text/plain",
                )
            }
            ObsidianResourceUri::VaultAudit => {
                let audit = self.vault_audit_resource_data().await?;
                json_resource(&audit, normalized_uri)?
            }
            ObsidianResourceUri::Note(path) => {
                let content = self.read_note_content_at(&path).await?;
                text_resource(content, normalized_uri, "text/markdown")
            }
            ObsidianResourceUri::NoteContext(path) => {
                let context = self.note_context_resource_data(&path).await?;
                json_resource(&context, normalized_uri)?
            }
            ObsidianResourceUri::Daily(date) => {
                let content = self.read_daily_note_for_date(&date).await?;
                text_resource(content, normalized_uri, "text/markdown")
            }
            ObsidianResourceUri::Base(path) => {
                let result = self.base_query_resource_data(&path).await?;
                json_resource(&result, normalized_uri)?
            }
            ObsidianResourceUri::TasksDueOn(date) => {
                let tasks = self
                    .dated_tasks_resource_data(DueDateFilter::On, &date)
                    .await?;
                json_resource(&tasks, normalized_uri)?
            }
            ObsidianResourceUri::TasksDueBefore(date) => {
                let tasks = self
                    .dated_tasks_resource_data(DueDateFilter::Before, &date)
                    .await?;
                json_resource(&tasks, normalized_uri)?
            }
            ObsidianResourceUri::ProjectStatus(path) => {
                let status = self.project_status_resource_data(&path).await?;
                json_resource(&status, normalized_uri)?
            }
        };

        Ok(ReadResourceResult::new(vec![contents]))
    }
}

fn serialize_resource_json(value: &impl rmcp::serde::Serialize) -> AppResult<String> {
    rmcp::serde_json::to_string_pretty(value).map_err(|error| {
        ObsidianMcpError::Parse(format!("Cannot serialize WorkOS resource: {error}"))
    })
}

fn json_resource(value: &impl rmcp::serde::Serialize, uri: String) -> AppResult<ResourceContents> {
    Ok(text_resource(
        serialize_resource_json(value)?,
        uri,
        "application/json",
    ))
}

fn text_resource(text: String, uri: String, mime_type: &str) -> ResourceContents {
    ResourceContents::text(text, uri).with_mime_type(mime_type)
}

fn plain_index(lines: Vec<String>, limit: usize) -> String {
    let total = lines.len();
    if total <= limit {
        lines.join("\n")
    } else {
        let mut index = lines[..limit].join("\n");
        index.push_str(&format!("\n# truncated: showing {limit} of {total}"));
        index
    }
}

fn workspace_profile_resource() -> Resource {
    RawResource::new(
        ObsidianResourceUri::WORKSPACE_PROFILE,
        "workos_workspace_profile",
    )
    .with_title("WorkOS workspace profile")
    .with_description(
        "Workspace configuration, vault status, conventions, bases, and capabilities.",
    )
    .with_mime_type("application/json")
    .no_annotation()
}

fn workspace_today_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::TODAY, "workos_workspace_today")
        .with_title("WorkOS today")
        .with_description(
            "Today's operational snapshot: daily note plus due, overdue, and daily-note tasks.",
        )
        .with_mime_type("application/json")
        .no_annotation()
}

fn tasks_open_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::TASKS_OPEN, "workos_tasks_open")
        .with_title("Open tasks")
        .with_description("All open tasks, normalized with due and scheduled dates.")
        .with_mime_type("application/json")
        .no_annotation()
}

fn projects_index_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::PROJECTS_INDEX, "workos_projects_index")
        .with_title("Projects index")
        .with_description("Project notes under the configured projects directory.")
        .with_mime_type("application/json")
        .no_annotation()
}

fn notes_index_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::NOTES_INDEX, "workos_notes_index")
        .with_title("Notes index")
        .with_description("Newline-delimited vault-relative paths of all Markdown notes.")
        .with_mime_type("text/plain")
        .no_annotation()
}

fn tags_index_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::TAGS_INDEX, "workos_tags_index")
        .with_title("Tags index")
        .with_description("Newline-delimited tags with occurrence counts, sorted by count.")
        .with_mime_type("text/plain")
        .no_annotation()
}

fn vault_audit_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::VAULT_AUDIT, "workos_vault_audit")
        .with_title("Vault graph audit")
        .with_description("Knowledge-graph hygiene: unresolved links, orphans, and dead ends.")
        .with_mime_type("application/json")
        .no_annotation()
}

fn write_resource_path(
    formatter: &mut fmt::Formatter<'_>,
    prefix: &str,
    path: &VaultRelativePath,
    suffix: &str,
) -> fmt::Result {
    write!(
        formatter,
        "{prefix}{}{suffix}",
        percent_encode_uri_path(&path.as_cli_arg())
    )
}

fn percent_encode_uri_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn percent_decode_uri_path(path: &str) -> AppResult<String> {
    let mut decoded = Vec::new();
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(ObsidianMcpError::InvalidInput(
                    "resource URI contains incomplete percent encoding".to_string(),
                ));
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).map_err(|_| {
                ObsidianMcpError::InvalidInput(
                    "resource URI contains invalid percent encoding".to_string(),
                )
            })?;
            let value = u8::from_str_radix(hex, 16).map_err(|_| {
                ObsidianMcpError::InvalidInput(
                    "resource URI contains invalid percent encoding".to_string(),
                )
            })?;
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).map_err(|_| {
        ObsidianMcpError::InvalidInput(
            "resource URI percent-decoded path is not valid UTF-8".to_string(),
        )
    })
}
