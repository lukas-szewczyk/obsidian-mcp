use rmcp::model::{
    AnnotateAble, RawResource, RawResourceTemplate, ReadResourceResult, Resource, ResourceContents,
    ResourceTemplate,
};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ObsidianResourceUri {
    VaultInfo,
    NotesIndex,
    TagsIndex,
    DailyToday,
    Daily(DailyDate),
    TasksOpen,
    ProjectsIndex,
    Note(VaultRelativePath),
    Backlinks(VaultRelativePath),
}

impl ObsidianResourceUri {
    const VAULT_INFO: &'static str = "obsidian://vault/info";
    const NOTES_INDEX: &'static str = "obsidian://notes/index";
    const TAGS_INDEX: &'static str = "obsidian://tags/index";
    const DAILY_TODAY: &'static str = "obsidian://daily/today";
    const DAILY_PREFIX: &'static str = "obsidian://daily/";
    const TASKS_OPEN: &'static str = "obsidian://tasks/open";
    const PROJECTS_INDEX: &'static str = "obsidian://projects/index";
    const NOTE_PREFIX: &'static str = "obsidian://note/";
    const BACKLINKS_PREFIX: &'static str = "obsidian://backlinks/";

    pub(super) fn parse(uri: &str) -> AppResult<Self> {
        match uri {
            Self::VAULT_INFO => Ok(Self::VaultInfo),
            Self::NOTES_INDEX => Ok(Self::NotesIndex),
            Self::TAGS_INDEX => Ok(Self::TagsIndex),
            Self::DAILY_TODAY => Ok(Self::DailyToday),
            Self::TASKS_OPEN => Ok(Self::TasksOpen),
            Self::PROJECTS_INDEX => Ok(Self::ProjectsIndex),
            _ => {
                if let Some(encoded_path) = uri.strip_prefix(Self::NOTE_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Note(VaultRelativePath::markdown(&decoded_path)?))
                } else if let Some(encoded_path) = uri.strip_prefix(Self::BACKLINKS_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Backlinks(VaultRelativePath::markdown(&decoded_path)?))
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

    pub(super) fn note(path: &VaultRelativePath) -> String {
        format!(
            "{}{}",
            Self::NOTE_PREFIX,
            percent_encode_uri_path(&path.as_cli_arg())
        )
    }

    pub(super) fn daily(date: &DailyDate) -> String {
        format!("{}{date}", Self::DAILY_PREFIX)
    }

    pub(super) fn backlinks(path: &VaultRelativePath) -> String {
        format!(
            "{}{}",
            Self::BACKLINKS_PREFIX,
            percent_encode_uri_path(&path.as_cli_arg())
        )
    }
}

impl ObsidianMcp {
    pub async fn list_resource_descriptors(&self) -> AppResult<Vec<Resource>> {
        let mut resources = vec![
            vault_info_resource(),
            notes_index_resource(),
            tags_index_resource(),
            daily_today_resource(),
            tasks_open_resource(),
            projects_index_resource(),
        ];
        for note in self.list_note_paths(None, Some(200)).await? {
            let path = VaultRelativePath::markdown(&note)?;
            resources.push(note_resource(&path));
            resources.push(backlinks_resource(&path));
        }
        Ok(resources)
    }

    pub fn list_resource_template_descriptors(&self) -> Vec<ResourceTemplate> {
        vec![
            RawResourceTemplate::new("obsidian://note/{path}", "obsidian_note_by_path")
                .with_title("Obsidian note")
                .with_description("Read a Markdown note by vault-relative path.")
                .with_mime_type("text/markdown")
                .no_annotation(),
            RawResourceTemplate::new("obsidian://backlinks/{path}", "obsidian_backlinks_by_path")
                .with_title("Obsidian backlinks")
                .with_description("Read backlinks for a Markdown note by vault-relative path.")
                .with_mime_type("text/plain")
                .no_annotation(),
            RawResourceTemplate::new("obsidian://daily/{date}", "obsidian_daily_by_date")
                .with_title("Obsidian daily note")
                .with_description("Read a daily note by YYYY-MM-DD date.")
                .with_mime_type("text/markdown")
                .no_annotation(),
        ]
    }

    pub async fn read_resource_uri(&self, uri: &str) -> AppResult<ReadResourceResult> {
        let resource_uri = ObsidianResourceUri::parse(uri)?;
        let contents = match resource_uri {
            ObsidianResourceUri::VaultInfo => {
                let info = self.vault_info_data().await?;
                ResourceContents::text(format_vault_info_resource(&info), uri)
                    .with_mime_type("text/plain")
            }
            ObsidianResourceUri::NotesIndex => {
                let notes = self.list_note_paths(None, Some(2_000)).await?;
                ResourceContents::text(notes.join("\n"), uri).with_mime_type("text/plain")
            }
            ObsidianResourceUri::TagsIndex => {
                let tags = self.list_tags_data(None, true, true, Some(2_000)).await?;
                ResourceContents::text(tags.join("\n"), uri).with_mime_type("text/plain")
            }
            ObsidianResourceUri::DailyToday => {
                let content = self.read_daily_note_content().await?;
                ResourceContents::text(content, uri).with_mime_type("text/markdown")
            }
            ObsidianResourceUri::Daily(date) => {
                let content = self.read_daily_note_for_date(&date).await?;
                ResourceContents::text(content, ObsidianResourceUri::daily(&date))
                    .with_mime_type("text/markdown")
            }
            ObsidianResourceUri::TasksOpen => {
                let tasks = self
                    .list_tasks_data(&TaskReadTarget::Vault, Some(&TaskStatus::Todo), Some(1_000))
                    .await?;
                ResourceContents::text(format_tasks_resource(&tasks), uri)
                    .with_mime_type("text/plain")
            }
            ObsidianResourceUri::ProjectsIndex => {
                let (_, projects) = self.list_project_note_paths(None, Some(1_000)).await?;
                ResourceContents::text(projects.join("\n"), uri).with_mime_type("text/plain")
            }
            ObsidianResourceUri::Note(path) => {
                let content = self.read_note_content_at(&path).await?;
                ResourceContents::text(content, ObsidianResourceUri::note(&path))
                    .with_mime_type("text/markdown")
            }
            ObsidianResourceUri::Backlinks(path) => {
                let backlinks = self
                    .list_backlinks_data(&path.as_cli_arg(), true, Some(1_000))
                    .await?;
                ResourceContents::text(backlinks.join("\n"), ObsidianResourceUri::backlinks(&path))
                    .with_mime_type("text/plain")
            }
        };

        Ok(ReadResourceResult::new(vec![contents]))
    }
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

fn note_resource(path: &VaultRelativePath) -> Resource {
    let uri = ObsidianResourceUri::note(path);
    let path = path.as_cli_arg();
    RawResource::new(uri, format!("obsidian_note:{path}"))
        .with_title(path)
        .with_description("Markdown note in the configured Obsidian vault.")
        .with_mime_type("text/markdown")
        .no_annotation()
}

fn backlinks_resource(path: &VaultRelativePath) -> Resource {
    let uri = ObsidianResourceUri::backlinks(path);
    let path = path.as_cli_arg();
    RawResource::new(uri, format!("obsidian_backlinks:{path}"))
        .with_title(format!("Backlinks for {path}"))
        .with_description("Backlinks to this Markdown note in the configured Obsidian vault.")
        .with_mime_type("text/plain")
        .no_annotation()
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
