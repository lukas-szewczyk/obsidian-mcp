mod knowledge_graph;
mod prompts;
mod resources;
mod tools;
mod work_system;

#[cfg(test)]
mod tests;

use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{
        GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams,
        ReadResourceRequestParams, ReadResourceResult, ServerCapabilities, ServerInfo,
    },
    service::RequestContext,
    tool_handler,
};

use crate::{
    AppResult, ObsidianMcpError,
    cli::{ObsidianCliRunner, ObsidianCommand, RealObsidianCli, encode_cli_text, truncate_error},
    domain::{DailyDate, PropertyName, TaskLine, VaultRelativePath, has_markdown_extension},
    error_message,
    models::*,
};

pub struct ObsidianMcp {
    vault: Arc<PathBuf>,
    vault_name: Option<String>,
    cli: Arc<dyn ObsidianCliRunner>,
    tool_router: ToolRouter<Self>,
}

impl ObsidianMcp {
    pub fn from_env() -> AppResult<Self> {
        let path = env::var_os("OBSIDIAN_VAULT_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_vault_path);
        Self::new(path)
    }

    pub fn new(vault: impl Into<PathBuf>) -> AppResult<Self> {
        Self::with_runner_and_vault_name(vault, vault_name_from_env(), RealObsidianCli::from_env())
    }

    pub fn default_vault_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("obsidian-vault")
    }

    #[cfg(test)]
    fn with_runner<R>(vault: impl Into<PathBuf>, cli: R) -> AppResult<Self>
    where
        R: ObsidianCliRunner + 'static,
    {
        Self::with_runner_and_vault_name(vault, None, cli)
    }

    fn with_runner_and_vault_name<R>(
        vault: impl Into<PathBuf>,
        vault_name: Option<String>,
        cli: R,
    ) -> AppResult<Self>
    where
        R: ObsidianCliRunner + 'static,
    {
        let vault = vault.into();
        let vault = vault.canonicalize().map_err(|error| {
            ObsidianMcpError::InvalidPath(format!(
                "Cannot access vault path '{}': {error}",
                vault.display()
            ))
        })?;

        if !vault.is_dir() {
            return Err(ObsidianMcpError::InvalidPath(format!(
                "Vault path '{}' is not a directory",
                vault.display()
            )));
        }

        Ok(Self {
            vault: Arc::new(vault),
            vault_name: normalize_vault_name(vault_name),
            cli: Arc::new(cli),
            tool_router: Self::tool_router(),
        })
    }

    pub fn vault_path(&self) -> &Path {
        self.vault.as_ref()
    }

    pub async fn vault_info_data(&self) -> AppResult<VaultInfoResponse> {
        let vault_metadata =
            parse_vault_metadata(&self.run_cli(ObsidianCommand::new("vault")).await?)?;
        let markdown_notes = parse_count(
            &self
                .run_cli(
                    ObsidianCommand::new("files")
                        .parameter("ext", "md")
                        .flag("total"),
                )
                .await?,
        )?;

        Ok(VaultInfoResponse {
            configured_vault_path: self.vault_path().display().to_string(),
            obsidian_vault_path: vault_metadata.path,
            obsidian_vault_name: vault_metadata.name,
            markdown_notes,
        })
    }

    pub async fn list_note_paths(
        &self,
        directory: Option<&str>,
        limit: Option<usize>,
    ) -> AppResult<Vec<String>> {
        let directory = safe_directory(directory)?;
        let mut command = ObsidianCommand::new("files").parameter("ext", "md");
        if let Some(directory) = &directory {
            command = command.parameter("folder", directory.as_cli_arg());
        }

        let mut notes = parse_output_lines(&self.run_cli(command).await?);
        notes.retain(|note| has_markdown_extension(note));
        notes.sort();
        notes.truncate(clamp_limit(limit, 200, 2_000));
        Ok(notes)
    }

    pub async fn read_note_content(&self, path: &str) -> AppResult<String> {
        let path = VaultRelativePath::markdown(path)?;
        self.read_note_content_at(&path).await
    }

    pub async fn create_note_content(&self, path: &str, content: &str) -> AppResult<String> {
        let path = VaultRelativePath::markdown(path)?;
        if self.note_exists_at(&path).await {
            return Err(ObsidianMcpError::InvalidInput(
                "Note already exists; use replace_note to replace it".to_string(),
            ));
        }

        self.run_cli(
            ObsidianCommand::new("create")
                .parameter("path", path.as_cli_arg())
                .parameter("content", encode_cli_text(content)),
        )
        .await?;
        Ok(format!("Created {}", path.as_cli_arg()))
    }

    pub async fn replace_note_content(&self, path: &str, content: &str) -> AppResult<String> {
        let path = VaultRelativePath::markdown(path)?;
        if !self.note_exists_at(&path).await {
            return Err(ObsidianMcpError::InvalidInput(
                "Note does not exist; use create_note to create it".to_string(),
            ));
        }

        self.run_cli(
            ObsidianCommand::new("create")
                .parameter("path", path.as_cli_arg())
                .parameter("content", encode_cli_text(content))
                .flag("overwrite"),
        )
        .await?;
        Ok(format!("Replaced {}", path.as_cli_arg()))
    }

    pub async fn append_note_content(&self, path: &str, content: &str) -> AppResult<String> {
        let path = VaultRelativePath::markdown(path)?;
        self.run_cli(
            ObsidianCommand::new("append")
                .parameter("path", path.as_cli_arg())
                .parameter("content", encode_cli_text(content))
                .flag("inline"),
        )
        .await?;

        Ok(format!("Appended to {}", path.as_cli_arg()))
    }

    pub async fn search_note_contents(
        &self,
        query: &str,
        directory: Option<&str>,
        limit: Option<usize>,
    ) -> AppResult<Vec<String>> {
        let query = query.trim();
        if query.is_empty() {
            return Err(ObsidianMcpError::InvalidInput(
                "query cannot be empty".to_string(),
            ));
        }

        let directory = safe_directory(directory)?;
        let limit = clamp_limit(limit, 50, 500);
        let mut command = ObsidianCommand::new("search:context")
            .parameter("query", query)
            .parameter("limit", limit.to_string());
        if let Some(directory) = &directory {
            command = command.parameter("path", directory.as_cli_arg());
        }

        let mut matches = parse_output_lines(&self.run_cli(command).await?);
        matches.truncate(limit);
        Ok(matches)
    }

    pub async fn list_tags_data(
        &self,
        path: Option<&str>,
        counts: bool,
        sort_by_count: bool,
        limit: Option<usize>,
    ) -> AppResult<Vec<String>> {
        let path = path.map(VaultRelativePath::markdown).transpose()?;
        let mut command = ObsidianCommand::new("tags");
        if let Some(path) = &path {
            command = command.parameter("path", path.as_cli_arg());
        }
        if counts {
            command = command.flag("counts");
        }
        if sort_by_count {
            command = command.parameter("sort", "count");
        }

        let mut tags = parse_output_lines(&self.run_cli(command).await?);
        tags.truncate(clamp_limit(limit, 200, 2_000));
        Ok(tags)
    }

    pub async fn list_backlinks_data(
        &self,
        path: &str,
        counts: bool,
        limit: Option<usize>,
    ) -> AppResult<Vec<String>> {
        let path = VaultRelativePath::markdown(path)?;
        let mut command = ObsidianCommand::new("backlinks").parameter("path", path.as_cli_arg());
        if counts {
            command = command.flag("counts");
        }

        let mut backlinks = parse_output_lines(&self.run_cli(command).await?);
        backlinks.truncate(clamp_limit(limit, 100, 1_000));
        Ok(backlinks)
    }

    pub async fn read_daily_note_content(&self) -> AppResult<String> {
        self.run_cli(ObsidianCommand::new("daily:read")).await
    }

    async fn read_daily_note_for_date(&self, date: &DailyDate) -> AppResult<String> {
        let path = date.note_path()?;
        self.read_note_content_at(&path).await
    }

    pub async fn append_daily_note_content(
        &self,
        content: &str,
        inline: bool,
    ) -> AppResult<String> {
        if content.trim().is_empty() {
            return Err(ObsidianMcpError::InvalidInput(
                "content cannot be empty".to_string(),
            ));
        }

        let mut command =
            ObsidianCommand::new("daily:append").parameter("content", encode_cli_text(content));
        if inline {
            command = command.flag("inline");
        }

        self.run_cli(command).await?;
        Ok("Appended to daily note".to_string())
    }

    pub async fn read_daily_notes_data(
        &self,
        from: &str,
        to: &str,
        limit: Option<usize>,
    ) -> AppResult<Vec<DailyNoteEntry>> {
        let from = DailyDate::parse(from)?;
        let to = DailyDate::parse(to)?;
        if from > to {
            return Err(ObsidianMcpError::InvalidInput(
                "from date must be before or equal to to date".to_string(),
            ));
        }

        let limit = clamp_limit(limit, 7, 31);
        let mut entries = Vec::new();
        let mut current = from;
        while current <= to && entries.len() < limit {
            let path = current.note_path()?;
            let path_text = path.as_cli_arg();
            let date_text = current.to_string();
            let entry = match self.read_note_content_at(&path).await {
                Ok(content) => DailyNoteEntry {
                    date: date_text,
                    path: path_text,
                    content: Some(content),
                    error: None,
                },
                Err(error) => DailyNoteEntry {
                    date: date_text,
                    path: path_text,
                    content: None,
                    error: Some(error.to_string()),
                },
            };
            entries.push(entry);
            current = current.next();
        }

        Ok(entries)
    }

    pub async fn list_tasks_data(
        &self,
        target: &TaskReadTarget,
        status: Option<&TaskStatus>,
        limit: Option<usize>,
    ) -> AppResult<Vec<TaskItem>> {
        let mut command = ObsidianCommand::new("tasks").parameter("format", "tsv");
        match target {
            TaskReadTarget::Vault => {}
            TaskReadTarget::Daily => command = command.flag("daily"),
            TaskReadTarget::Note { path } => {
                let path = VaultRelativePath::markdown(path)?;
                command = command.parameter("path", path.as_cli_arg());
            }
        }
        if let Some(status) = status {
            command = task_status_command(command, status)?;
        }

        let mut tasks = parse_tasks_tsv(&self.run_cli(command).await?)?;
        tasks.truncate(clamp_limit(limit, 100, 1_000));
        Ok(tasks)
    }

    pub async fn create_task_data(
        &self,
        target: &TaskWriteTarget,
        text: &str,
    ) -> AppResult<(String, String)> {
        let task = format_task_line(text)?;
        match target {
            TaskWriteTarget::Daily => {
                self.run_cli(
                    ObsidianCommand::new("daily:append")
                        .parameter("content", encode_cli_text(&task)),
                )
                .await?;
                Ok(("daily".to_string(), task))
            }
            TaskWriteTarget::Note { path } => {
                let path = VaultRelativePath::markdown(path)?;
                self.run_cli(
                    ObsidianCommand::new("append")
                        .parameter("path", path.as_cli_arg())
                        .parameter("content", encode_cli_text(&task)),
                )
                .await?;
                Ok((path.as_cli_arg(), task))
            }
        }
    }

    pub async fn set_task_status_data(
        &self,
        path: &str,
        line: usize,
        status: &TaskStatus,
    ) -> AppResult<String> {
        let path = VaultRelativePath::markdown(path)?;
        let line = TaskLine::parse(line)?;
        let command = ObsidianCommand::new("task")
            .parameter("path", path.as_cli_arg())
            .parameter("line", line.as_usize().to_string());
        let command = task_status_command(command, status)?;
        let status = task_status_value(status)?;

        self.run_cli(command).await?;
        Ok(status)
    }

    pub async fn list_project_note_paths(
        &self,
        directory: Option<&str>,
        limit: Option<usize>,
    ) -> AppResult<(String, Vec<String>)> {
        let directory = directory
            .map(str::trim)
            .filter(|directory| !directory.is_empty())
            .map(str::to_string)
            .unwrap_or_else(project_directory_from_env);
        let directory = VaultRelativePath::parse(&directory)?;
        let projects = self
            .list_note_paths(Some(&directory.as_cli_arg()), limit)
            .await?;

        Ok((directory.as_cli_arg(), projects))
    }

    async fn read_note_content_at(&self, path: &VaultRelativePath) -> AppResult<String> {
        self.run_cli(ObsidianCommand::new("read").parameter("path", path.as_cli_arg()))
            .await
    }

    async fn note_exists_at(&self, path: &VaultRelativePath) -> bool {
        self.run_cli(ObsidianCommand::new("file").parameter("path", path.as_cli_arg()))
            .await
            .is_ok()
    }

    async fn run_cli(&self, command: ObsidianCommand) -> AppResult<String> {
        self.cli
            .run(
                self.vault_path(),
                command.into_args(self.vault_name.as_deref()),
            )
            .await
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ObsidianMcp {
    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        self.list_resource_descriptors()
            .await
            .map(ListResourcesResult::with_all_items)
            .map_err(internal_mcp_error)
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, McpError> {
        Ok(ListResourceTemplatesResult::with_all_items(
            self.list_resource_template_descriptors(),
        ))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        self.read_resource_uri(&request.uri)
            .await
            .map_err(resource_mcp_error)
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, McpError> {
        Ok(ListPromptsResult::with_all_items(
            self.list_prompt_descriptors(),
        ))
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, McpError> {
        self.get_prompt_result(request).map_err(prompt_mcp_error)
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_resources()
                .enable_prompts()
                .build(),
        )
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions("Use these tools, resources, and prompts to work with Markdown notes, frontmatter properties, daily notes, tasks, overdue work, knowledge graph context, vault graph audits, backlinks, and project status through the Obsidian CLI. Preview note and property changes before applying uncertain writes. Use create_note only for missing notes and replace_note only for existing notes. Obsidian must be running with the CLI enabled. Paths must be relative to the configured vault.")
    }
}

fn vault_name_from_env() -> Option<String> {
    env::var("OBSIDIAN_VAULT_NAME").ok()
}

fn normalize_vault_name(vault_name: Option<String>) -> Option<String> {
    vault_name
        .map(|vault_name| vault_name.trim().to_string())
        .filter(|vault_name| !vault_name.is_empty())
}

fn safe_directory(directory: Option<&str>) -> AppResult<Option<VaultRelativePath>> {
    match directory
        .map(str::trim)
        .filter(|directory| !directory.is_empty())
    {
        Some(directory) => Ok(Some(VaultRelativePath::parse(directory)?)),
        None => Ok(None),
    }
}

fn project_directory_from_env() -> String {
    env::var("OBSIDIAN_PROJECTS_PATH")
        .ok()
        .map(|directory| directory.trim().to_string())
        .filter(|directory| !directory.is_empty())
        .unwrap_or_else(|| "Projects".to_string())
}

fn validate_task_status(status: &str) -> AppResult<char> {
    let status = status.trim();
    let mut chars = status.chars();
    let Some(status) = chars.next() else {
        return Err(ObsidianMcpError::InvalidInput(
            "task status cannot be empty".to_string(),
        ));
    };
    if chars.next().is_some() {
        return Err(ObsidianMcpError::InvalidInput(
            "task status must be a single character".to_string(),
        ));
    }

    Ok(status)
}

fn task_status_command(
    command: ObsidianCommand,
    status: &TaskStatus,
) -> AppResult<ObsidianCommand> {
    match status {
        TaskStatus::Todo => Ok(command.flag("todo")),
        TaskStatus::Done => Ok(command.flag("done")),
        TaskStatus::Custom { value } => {
            Ok(command.parameter("status", validate_task_status(value)?.to_string()))
        }
    }
}

fn task_status_value(status: &TaskStatus) -> AppResult<String> {
    match status {
        TaskStatus::Todo => Ok(" ".to_string()),
        TaskStatus::Done => Ok("x".to_string()),
        TaskStatus::Custom { value } => Ok(validate_task_status(value)?.to_string()),
    }
}

fn format_task_line(text: &str) -> AppResult<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(ObsidianMcpError::InvalidInput(
            "task text cannot be empty".to_string(),
        ));
    }
    if text.contains('\n') || text.contains('\r') {
        return Err(ObsidianMcpError::InvalidInput(
            "task text must be a single line".to_string(),
        ));
    }

    if text.starts_with("- [") {
        Ok(text.to_string())
    } else {
        Ok(format!("- [ ] {text}"))
    }
}

fn parse_tasks_tsv(output: &str) -> AppResult<Vec<TaskItem>> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(parse_task_tsv_line)
        .collect()
}

fn parse_task_tsv_line(line: &str) -> AppResult<TaskItem> {
    let mut tail = line.rsplitn(3, '\t');
    let line_number = tail.next().unwrap_or_default();
    let path = tail.next().unwrap_or_default();
    let head = tail.next().unwrap_or_default();
    let Some((status, text)) = head.split_once('\t') else {
        return Err(ObsidianMcpError::Parse(format!(
            "Cannot parse task row from Obsidian CLI output: {}",
            truncate_error(line)
        )));
    };
    let line = line_number.parse::<usize>().map_err(|_| {
        ObsidianMcpError::Parse(format!(
            "Cannot parse task line number from Obsidian CLI output: {}",
            truncate_error(line)
        ))
    })?;

    Ok(TaskItem {
        status: status.to_string(),
        text: text.to_string(),
        path: path.to_string(),
        line,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VaultMetadata {
    name: String,
    path: String,
}

fn parse_vault_metadata(output: &str) -> AppResult<VaultMetadata> {
    let mut name = None;
    let mut path = None;

    for line in output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.splitn(2, char::is_whitespace);
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default().trim();

        match key {
            "name" if !value.is_empty() => name = Some(value.to_string()),
            "path" if !value.is_empty() => path = Some(value.to_string()),
            _ => {}
        }
    }

    let name = name.ok_or_else(|| {
        ObsidianMcpError::Parse(format!(
            "Cannot parse vault name from Obsidian CLI output: {}",
            truncate_error(output)
        ))
    })?;
    let path = path.ok_or_else(|| {
        ObsidianMcpError::Parse(format!(
            "Cannot parse vault path from Obsidian CLI output: {}",
            truncate_error(output)
        ))
    })?;

    Ok(VaultMetadata { name, path })
}

fn internal_mcp_error(error: ObsidianMcpError) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

fn resource_mcp_error(error: ObsidianMcpError) -> McpError {
    match error {
        ObsidianMcpError::InvalidInput(message) => McpError::invalid_params(message, None),
        ObsidianMcpError::InvalidPath(message) | ObsidianMcpError::ResourceNotFound(message) => {
            McpError::resource_not_found(message, None)
        }
        error => McpError::internal_error(error.to_string(), None),
    }
}

fn prompt_mcp_error(error: ObsidianMcpError) -> McpError {
    match error {
        ObsidianMcpError::InvalidInput(message) | ObsidianMcpError::InvalidPath(message) => {
            McpError::invalid_params(message, None)
        }
        ObsidianMcpError::ResourceNotFound(message) => McpError::resource_not_found(message, None),
        error => McpError::internal_error(error.to_string(), None),
    }
}

fn parse_output_lines(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_count(output: &str) -> AppResult<usize> {
    output
        .split_whitespace()
        .filter_map(|word| word.parse::<usize>().ok())
        .next_back()
        .ok_or_else(|| {
            ObsidianMcpError::Parse(format!(
                "Cannot parse Markdown note count from Obsidian CLI output: {}",
                truncate_error(output)
            ))
        })
}

fn clamp_limit(limit: Option<usize>, default: usize, maximum: usize) -> usize {
    limit.unwrap_or(default).min(maximum)
}
