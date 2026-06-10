use std::{fmt, str::FromStr};

use rmcp::model::{
    AnnotateAble, RawResource, RawResourceTemplate, ReadResourceResult, Resource, ResourceContents,
    ResourceTemplate,
};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ObsidianResourceUri {
    VaultInfo,
    VaultAudit,
    BasesIndex,
    NotesIndex,
    TagsIndex,
    DailyToday,
    Daily(DailyDate),
    TasksOpen,
    TasksOverdue(DailyDate),
    ProjectsIndex,
    Note(VaultRelativePath),
    Base(VaultRelativePath),
    Project(VaultRelativePath),
    Properties(VaultRelativePath),
}

impl ObsidianResourceUri {
    const VAULT_INFO: &'static str = "obsidian://vault/info";
    const VAULT_AUDIT: &'static str = "obsidian://vault/audit";
    const BASES_INDEX: &'static str = "obsidian://bases/index";
    const NOTES_INDEX: &'static str = "obsidian://notes/index";
    const TAGS_INDEX: &'static str = "obsidian://tags/index";
    const DAILY_TODAY: &'static str = "obsidian://daily/today";
    const DAILY_PREFIX: &'static str = "obsidian://daily/";
    const TASKS_OPEN: &'static str = "obsidian://tasks/open";
    const TASKS_OVERDUE_PREFIX: &'static str = "obsidian://tasks/overdue/";
    const PROJECTS_INDEX: &'static str = "obsidian://projects/index";
    const NOTE_PREFIX: &'static str = "obsidian://note/";
    const BASE_PREFIX: &'static str = "obsidian://base/";
    const PROJECT_PREFIX: &'static str = "obsidian://project/";
    const PROPERTIES_PREFIX: &'static str = "obsidian://properties/";
}

impl FromStr for ObsidianResourceUri {
    type Err = ObsidianMcpError;

    fn from_str(uri: &str) -> Result<Self, Self::Err> {
        match uri {
            Self::VAULT_INFO => Ok(Self::VaultInfo),
            Self::VAULT_AUDIT => Ok(Self::VaultAudit),
            Self::BASES_INDEX => Ok(Self::BasesIndex),
            Self::NOTES_INDEX => Ok(Self::NotesIndex),
            Self::TAGS_INDEX => Ok(Self::TagsIndex),
            Self::DAILY_TODAY => Ok(Self::DailyToday),
            Self::TASKS_OPEN => Ok(Self::TasksOpen),
            Self::PROJECTS_INDEX => Ok(Self::ProjectsIndex),
            _ => {
                if let Some(encoded_path) = uri.strip_prefix(Self::NOTE_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Note(VaultRelativePath::markdown(&decoded_path)?))
                } else if let Some(encoded_path) = uri.strip_prefix(Self::BASE_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Base(VaultRelativePath::base(&decoded_path)?))
                } else if let Some(encoded_path) = uri.strip_prefix(Self::PROJECT_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Project(VaultRelativePath::markdown(&decoded_path)?))
                } else if let Some(encoded_path) = uri.strip_prefix(Self::PROPERTIES_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Properties(VaultRelativePath::markdown(
                        &decoded_path,
                    )?))
                } else if let Some(date) = uri.strip_prefix(Self::TASKS_OVERDUE_PREFIX) {
                    Ok(Self::TasksOverdue(DailyDate::parse(date)?))
                } else if let Some(date) = uri.strip_prefix(Self::DAILY_PREFIX) {
                    Ok(Self::Daily(DailyDate::parse(date)?))
                } else {
                    Err(ObsidianMcpError::ResourceNotFound(format!(
                        "Unsupported Obsidian resource URI: {uri}"
                    )))
                }
            }
        }
    }
}

impl fmt::Display for ObsidianResourceUri {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VaultInfo => formatter.write_str(Self::VAULT_INFO),
            Self::VaultAudit => formatter.write_str(Self::VAULT_AUDIT),
            Self::BasesIndex => formatter.write_str(Self::BASES_INDEX),
            Self::NotesIndex => formatter.write_str(Self::NOTES_INDEX),
            Self::TagsIndex => formatter.write_str(Self::TAGS_INDEX),
            Self::DailyToday => formatter.write_str(Self::DAILY_TODAY),
            Self::Daily(date) => write!(formatter, "{}{date}", Self::DAILY_PREFIX),
            Self::TasksOpen => formatter.write_str(Self::TASKS_OPEN),
            Self::TasksOverdue(date) => {
                write!(formatter, "{}{date}", Self::TASKS_OVERDUE_PREFIX)
            }
            Self::ProjectsIndex => formatter.write_str(Self::PROJECTS_INDEX),
            Self::Note(path) => write_resource_path(formatter, Self::NOTE_PREFIX, path),
            Self::Base(path) => write_resource_path(formatter, Self::BASE_PREFIX, path),
            Self::Project(path) => write_resource_path(formatter, Self::PROJECT_PREFIX, path),
            Self::Properties(path) => write_resource_path(formatter, Self::PROPERTIES_PREFIX, path),
        }
    }
}

impl ObsidianMcp {
    pub fn list_resource_descriptors(&self) -> Vec<Resource> {
        vec![
            vault_info_resource(),
            vault_audit_resource(),
            bases_index_resource(),
            notes_index_resource(),
            tags_index_resource(),
            daily_today_resource(),
            tasks_open_resource(),
            projects_index_resource(),
        ]
    }

    pub fn list_resource_template_descriptors(&self) -> Vec<ResourceTemplate> {
        vec![
            RawResourceTemplate::new("obsidian://note/{path}", "obsidian_note_by_path")
                .with_title("Obsidian note")
                .with_description("Read a Markdown note by vault-relative path.")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResourceTemplate::new("obsidian://base/{path}", "obsidian_base_by_path")
                .with_title("Obsidian Base query")
                .with_description(
                    "Query the default view of an Obsidian Base by vault-relative path.",
                )
                .with_mime_type("application/json")
                .no_annotation(),
            RawResourceTemplate::new("obsidian://daily/{date}", "obsidian_daily_by_date")
                .with_title("Obsidian daily note")
                .with_description("Read a daily note by YYYY-MM-DD date.")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResourceTemplate::new(
                "obsidian://tasks/overdue/{date}",
                "obsidian_overdue_tasks_by_date",
            )
            .with_title("Overdue Obsidian tasks")
            .with_description("Read incomplete tasks due before a YYYY-MM-DD date.")
            .with_mime_type("application/json")
            .no_annotation(),
            RawResourceTemplate::new(
                "obsidian://project/{path}",
                "obsidian_project_status_by_path",
            )
            .with_title("Obsidian project status")
            .with_description("Read a project note with properties, tasks, and backlinks.")
            .with_mime_type("application/json")
            .no_annotation(),
            RawResourceTemplate::new(
                "obsidian://properties/{path}",
                "obsidian_note_properties_by_path",
            )
            .with_title("Obsidian note properties")
            .with_description("Read structured frontmatter properties for one Markdown note.")
            .with_mime_type("application/json")
            .no_annotation(),
        ]
    }

    pub async fn read_resource_uri(&self, uri: &str) -> AppResult<ReadResourceResult> {
        let resource_uri = uri.parse::<ObsidianResourceUri>()?;
        let normalized_uri = resource_uri.to_string();
        let contents = match resource_uri {
            ObsidianResourceUri::VaultInfo => {
                let info = self.vault_info_data().await?;
                text_resource(
                    format_vault_info_resource(&info),
                    normalized_uri,
                    "text/plain",
                )
            }
            ObsidianResourceUri::VaultAudit => {
                let audit = self.audit_vault_data(Some(1_000)).await?;
                text_resource(
                    serialize_resource_json(&audit)?,
                    normalized_uri,
                    "application/json",
                )
            }
            ObsidianResourceUri::BasesIndex => {
                let bases = self.list_bases_data(Some(1_000)).await?;
                text_resource(bases.join("\n"), normalized_uri, "text/plain")
            }
            ObsidianResourceUri::NotesIndex => {
                let notes = self.discover_note_paths(None).await?;
                text_resource(notes.join("\n"), normalized_uri, "text/plain")
            }
            ObsidianResourceUri::TagsIndex => {
                let tags = self.list_tags_data(None, true, true, Some(2_000)).await?;
                text_resource(tags.join("\n"), normalized_uri, "text/plain")
            }
            ObsidianResourceUri::DailyToday => {
                let content = self.read_daily_note_content().await?;
                text_resource(content, normalized_uri, "text/markdown")
            }
            ObsidianResourceUri::Daily(date) => {
                let content = self.read_daily_note_for_date(&date).await?;
                text_resource(content, normalized_uri, "text/markdown")
            }
            ObsidianResourceUri::TasksOpen => {
                let tasks = self
                    .list_tasks_data(&TaskReadTarget::Vault, Some(&TaskStatus::Todo), Some(1_000))
                    .await?;
                text_resource(format_tasks_resource(&tasks), normalized_uri, "text/plain")
            }
            ObsidianResourceUri::TasksOverdue(date) => {
                let tasks = self
                    .list_overdue_tasks_data(&date.to_string(), &TaskReadTarget::Vault, Some(1_000))
                    .await?;
                let response = ListOverdueTasksResponse {
                    as_of: date.to_string(),
                    target: TaskReadTarget::Vault,
                    count: tasks.len(),
                    tasks,
                };
                text_resource(
                    serialize_resource_json(&response)?,
                    normalized_uri,
                    "application/json",
                )
            }
            ObsidianResourceUri::ProjectsIndex => {
                let (_, projects) = self.list_project_note_paths(None, Some(1_000)).await?;
                text_resource(projects.join("\n"), normalized_uri, "text/plain")
            }
            ObsidianResourceUri::Note(path) => {
                let content = self.read_note_content_at(&path).await?;
                text_resource(content, normalized_uri, "text/markdown")
            }
            ObsidianResourceUri::Base(path) => {
                let result = self
                    .query_base_data(&path.as_cli_arg(), None, Some(1_000))
                    .await?;
                text_resource(
                    serialize_resource_json(&result)?,
                    normalized_uri,
                    "application/json",
                )
            }
            ObsidianResourceUri::Project(path) => {
                let status = self
                    .get_project_status_data(&path.as_cli_arg(), Some(500))
                    .await?;
                text_resource(
                    serialize_resource_json(&status)?,
                    normalized_uri,
                    "application/json",
                )
            }
            ObsidianResourceUri::Properties(path) => {
                let properties = self.list_properties_data(&path.as_cli_arg()).await?;
                let response = ListPropertiesResponse {
                    path: path.as_cli_arg(),
                    count: properties.len(),
                    properties,
                };
                text_resource(
                    serialize_resource_json(&response)?,
                    normalized_uri,
                    "application/json",
                )
            }
        };

        Ok(ReadResourceResult::new(vec![contents]))
    }
}

fn serialize_resource_json(value: &impl rmcp::serde::Serialize) -> AppResult<String> {
    rmcp::serde_json::to_string_pretty(value).map_err(|error| {
        ObsidianMcpError::Parse(format!("Cannot serialize Obsidian resource: {error}"))
    })
}

fn text_resource(text: String, uri: String, mime_type: &str) -> ResourceContents {
    ResourceContents::text(text, uri).with_mime_type(mime_type)
}

fn format_tasks_resource(tasks: &[TaskItem]) -> String {
    tasks
        .iter()
        .map(|task| format!("{}:{}\t{}", task.path, task.line, task.text))
        .collect::<Vec<_>>()
        .join("\n")
}

fn vault_info_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::VAULT_INFO, "obsidian_vault_info")
        .with_title("Obsidian vault info")
        .with_description(
            "Configured vault path, Obsidian-reported vault identity, and note count.",
        )
        .with_mime_type("text/plain")
        .no_annotation()
}

fn vault_audit_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::VAULT_AUDIT, "obsidian_vault_audit")
        .with_title("Obsidian vault graph audit")
        .with_description("Unresolved links, orphan notes, and dead ends in the Markdown vault.")
        .with_mime_type("application/json")
        .no_annotation()
}

fn bases_index_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::BASES_INDEX, "obsidian_bases_index")
        .with_title("Obsidian Bases index")
        .with_description("Newline-delimited list of Obsidian Base paths in the vault.")
        .with_mime_type("text/plain")
        .no_annotation()
}

fn notes_index_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::NOTES_INDEX, "obsidian_notes_index")
        .with_title("Obsidian notes index")
        .with_description("Newline-delimited list of Markdown note paths in the vault.")
        .with_mime_type("text/plain")
        .no_annotation()
}

fn tags_index_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::TAGS_INDEX, "obsidian_tags_index")
        .with_title("Obsidian tags index")
        .with_description("Newline-delimited tags in the vault, optionally with counts.")
        .with_mime_type("text/plain")
        .no_annotation()
}

fn daily_today_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::DAILY_TODAY, "obsidian_daily_today")
        .with_title("Today's daily note")
        .with_description("Markdown contents of today's Obsidian daily note.")
        .with_mime_type("text/markdown")
        .no_annotation()
}

fn tasks_open_resource() -> Resource {
    RawResource::new(ObsidianResourceUri::TASKS_OPEN, "obsidian_tasks_open")
        .with_title("Open Obsidian tasks")
        .with_description("Open Markdown tasks with vault-relative path and line references.")
        .with_mime_type("text/plain")
        .no_annotation()
}

fn projects_index_resource() -> Resource {
    RawResource::new(
        ObsidianResourceUri::PROJECTS_INDEX,
        "obsidian_projects_index",
    )
    .with_title("Obsidian projects index")
    .with_description("Markdown project notes under the configured projects directory.")
    .with_mime_type("text/plain")
    .no_annotation()
}

fn write_resource_path(
    formatter: &mut fmt::Formatter<'_>,
    prefix: &str,
    path: &VaultRelativePath,
) -> fmt::Result {
    write!(
        formatter,
        "{prefix}{}",
        percent_encode_uri_path(&path.as_cli_arg())
    )
}

fn format_vault_info_resource(info: &VaultInfoResponse) -> String {
    format!(
        "configured_vault_path\t{}\nobsidian_vault_path\t{}\nobsidian_vault_name\t{}\nmarkdown_notes\t{}",
        info.configured_vault_path,
        info.obsidian_vault_path,
        info.obsidian_vault_name,
        info.markdown_notes
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
