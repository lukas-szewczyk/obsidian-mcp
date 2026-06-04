use std::{
    env,
    ffi::{OsStr, OsString},
    fmt,
    future::Future,
    path::{Component, Path, PathBuf},
    pin::Pin,
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use rmcp::{
    ErrorData as McpError, RoleServer, ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{
        AnnotateAble, GetPromptRequestParams, GetPromptResult, Implementation, ListPromptsResult,
        ListResourceTemplatesResult, ListResourcesResult, PaginatedRequestParams, Prompt,
        PromptArgument, PromptMessage, PromptMessageRole, RawResource, RawResourceTemplate,
        ReadResourceRequestParams, ReadResourceResult, Resource, ResourceContents,
        ResourceTemplate, ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router,
};

type CliFuture<'a> = Pin<Box<dyn Future<Output = AppResult<String>> + Send + 'a>>;

trait ObsidianCliRunner: std::fmt::Debug + Send + Sync {
    fn run<'a>(&'a self, vault: &'a Path, args: Vec<OsString>) -> CliFuture<'a>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObsidianMcpError {
    InvalidInput(String),
    InvalidPath(String),
    CliUnavailable(String),
    CliFailed(String),
    Parse(String),
    ResourceNotFound(String),
}

impl fmt::Display for ObsidianMcpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidInput(message)
            | Self::InvalidPath(message)
            | Self::CliUnavailable(message)
            | Self::CliFailed(message)
            | Self::Parse(message)
            | Self::ResourceNotFound(message) => message,
        };
        formatter.write_str(message)
    }
}

type AppResult<T> = Result<T, ObsidianMcpError>;

fn error_message(error: ObsidianMcpError) -> String {
    error.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VaultRelativePath(PathBuf);

impl VaultRelativePath {
    fn parse(raw_path: &str) -> AppResult<Self> {
        let normalized = raw_path.trim().replace('\\', "/");
        if normalized.is_empty() {
            return Err(ObsidianMcpError::InvalidPath(
                "path cannot be empty".to_string(),
            ));
        }

        let path = Path::new(&normalized);
        if path.is_absolute() {
            return Err(ObsidianMcpError::InvalidPath(
                "path must be relative to the vault".to_string(),
            ));
        }

        let mut safe_path = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(segment) => safe_path.push(segment),
                Component::CurDir => {}
                _ => {
                    return Err(ObsidianMcpError::InvalidPath(
                        "path cannot escape the vault".to_string(),
                    ));
                }
            }
        }

        if safe_path.as_os_str().is_empty() {
            return Err(ObsidianMcpError::InvalidPath(
                "path cannot be empty".to_string(),
            ));
        }

        Ok(Self(safe_path))
    }

    fn markdown(raw_path: &str) -> AppResult<Self> {
        let path = Self::parse(raw_path)?;
        let extension = path
            .0
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();

        if !extension.eq_ignore_ascii_case("md") {
            return Err(ObsidianMcpError::InvalidPath(
                "Only Markdown notes with the .md extension are supported".to_string(),
            ));
        }

        Ok(path)
    }

    fn as_cli_arg(&self) -> String {
        path_to_cli_arg(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ObsidianResourceUri {
    VaultInfo,
    NotesIndex,
    TagsIndex,
    DailyToday,
    Note(VaultRelativePath),
    Backlinks(VaultRelativePath),
}

impl ObsidianResourceUri {
    const VAULT_INFO: &'static str = "obsidian://vault/info";
    const NOTES_INDEX: &'static str = "obsidian://notes/index";
    const TAGS_INDEX: &'static str = "obsidian://tags/index";
    const DAILY_TODAY: &'static str = "obsidian://daily/today";
    const NOTE_PREFIX: &'static str = "obsidian://note/";
    const BACKLINKS_PREFIX: &'static str = "obsidian://backlinks/";

    fn parse(uri: &str) -> AppResult<Self> {
        match uri {
            Self::VAULT_INFO => Ok(Self::VaultInfo),
            Self::NOTES_INDEX => Ok(Self::NotesIndex),
            Self::TAGS_INDEX => Ok(Self::TagsIndex),
            Self::DAILY_TODAY => Ok(Self::DailyToday),
            _ => {
                if let Some(encoded_path) = uri.strip_prefix(Self::NOTE_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Note(VaultRelativePath::markdown(&decoded_path)?))
                } else if let Some(encoded_path) = uri.strip_prefix(Self::BACKLINKS_PREFIX) {
                    let decoded_path = percent_decode_uri_path(encoded_path)?;
                    Ok(Self::Backlinks(VaultRelativePath::markdown(&decoded_path)?))
                } else {
                    Err(ObsidianMcpError::ResourceNotFound(format!(
                        "Unsupported Obsidian resource URI: {uri}"
                    )))
                }
            }
        }
    }

    fn note(path: &VaultRelativePath) -> String {
        format!(
            "{}{}",
            Self::NOTE_PREFIX,
            percent_encode_uri_path(&path.as_cli_arg())
        )
    }

    fn backlinks(path: &VaultRelativePath) -> String {
        format!(
            "{}{}",
            Self::BACKLINKS_PREFIX,
            percent_encode_uri_path(&path.as_cli_arg())
        )
    }
}

#[derive(Debug, Clone)]
struct ObsidianCommand {
    args: Vec<OsString>,
}

impl ObsidianCommand {
    fn new(command: impl Into<OsString>) -> Self {
        Self {
            args: vec![command.into()],
        }
    }

    fn parameter(mut self, key: &str, value: impl AsRef<str>) -> Self {
        self.args.push(format!("{key}={}", value.as_ref()).into());
        self
    }

    fn flag(mut self, flag: impl Into<OsString>) -> Self {
        self.args.push(flag.into());
        self
    }

    fn into_args(self, vault_name: Option<&str>) -> Vec<OsString> {
        match vault_name {
            Some(vault_name) => std::iter::once(OsString::from(format!("vault={vault_name}")))
                .chain(self.args)
                .collect(),
            None => self.args,
        }
    }
}

#[derive(Debug, Clone)]
struct RealObsidianCli {
    program: OsString,
    timeout: Duration,
}

impl RealObsidianCli {
    fn from_env() -> Self {
        let program = env::var_os("OBSIDIAN_CLI").unwrap_or_else(|| OsString::from("obsidian"));
        Self {
            program,
            timeout: Duration::from_secs(15),
        }
    }

    fn run_blocking(
        program: OsString,
        vault: PathBuf,
        args: Vec<OsString>,
        timeout: Duration,
    ) -> AppResult<String> {
        let command_text = format_command(&program, &args);
        let mut child = Command::new(&program)
            .current_dir(vault)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    ObsidianMcpError::CliUnavailable(format!(
                        "Cannot run Obsidian CLI '{}': command not found. Install or enable the Obsidian CLI, or set OBSIDIAN_CLI to the CLI path.",
                        program.to_string_lossy()
                    ))
                } else {
                    ObsidianMcpError::CliFailed(format!(
                        "Cannot run Obsidian CLI command '{command_text}': {error}"
                    ))
                }
            })?;

        let started_at = Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|error| {
                    ObsidianMcpError::CliFailed(format!(
                        "Cannot wait for Obsidian CLI command '{command_text}': {error}"
                    ))
                })?
                .is_some()
            {
                let output = child.wait_with_output().map_err(|error| {
                    ObsidianMcpError::CliFailed(format!(
                        "Cannot collect Obsidian CLI output for '{command_text}': {error}"
                    ))
                })?;

                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                if output.status.success() {
                    return Ok(stdout);
                }

                let stderr = String::from_utf8_lossy(&output.stderr);
                let details = first_non_empty([stderr.as_ref(), stdout.as_str()])
                    .map(truncate_error)
                    .unwrap_or_else(|| format!("exit status {}", output.status));
                return Err(ObsidianMcpError::CliFailed(format!(
                    "Obsidian CLI command failed: {command_text}\n{details}"
                )));
            }

            if started_at.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ObsidianMcpError::CliFailed(format!(
                    "Obsidian CLI command timed out after {}s: {command_text}",
                    timeout.as_secs()
                )));
            }

            thread::sleep(Duration::from_millis(25));
        }
    }
}

impl ObsidianCliRunner for RealObsidianCli {
    fn run<'a>(&'a self, vault: &'a Path, args: Vec<OsString>) -> CliFuture<'a> {
        let program = self.program.clone();
        let timeout = self.timeout;
        let vault = vault.to_path_buf();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || Self::run_blocking(program, vault, args, timeout))
                .await
                .map_err(|error| {
                    ObsidianMcpError::CliFailed(format!("Obsidian CLI worker failed: {error}"))
                })?
        })
    }
}

#[derive(Debug, Clone)]
pub struct ObsidianMcp {
    vault: Arc<PathBuf>,
    vault_name: Option<String>,
    cli: Arc<dyn ObsidianCliRunner>,
    tool_router: ToolRouter<Self>,
}

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
pub struct WriteNoteRequest {
    pub path: String,
    pub content: String,
    pub overwrite: Option<bool>,
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
pub struct AppendDailyNoteRequest {
    pub content: String,
    pub inline: Option<bool>,
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
pub struct WriteNoteResponse {
    pub path: String,
    pub overwritten: bool,
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
pub struct ReadDailyNoteResponse {
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, rmcp::serde::Serialize, schemars::JsonSchema)]
pub struct AppendDailyNoteResponse {
    pub message: String,
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

    pub async fn write_note_content(
        &self,
        path: &str,
        content: &str,
        overwrite: bool,
    ) -> AppResult<String> {
        let path = VaultRelativePath::markdown(path)?;
        if !overwrite
            && self
                .run_cli(ObsidianCommand::new("file").parameter("path", path.as_cli_arg()))
                .await
                .is_ok()
        {
            return Err(ObsidianMcpError::InvalidInput(
                "Note already exists; pass overwrite=true to replace it".to_string(),
            ));
        }

        let mut command = ObsidianCommand::new("create")
            .parameter("path", path.as_cli_arg())
            .parameter("content", encode_cli_text(content));
        if overwrite {
            command = command.flag("overwrite");
        }

        self.run_cli(command).await?;
        Ok(format!("Wrote {}", path.as_cli_arg()))
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

    pub async fn list_resource_descriptors(&self) -> AppResult<Vec<Resource>> {
        let mut resources = vec![
            vault_info_resource(),
            notes_index_resource(),
            tags_index_resource(),
            daily_today_resource(),
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

    pub fn list_prompt_descriptors(&self) -> Vec<Prompt> {
        vec![
            Prompt::new(
                "summarize_note",
                Some("Summarize one Obsidian note and extract useful follow-up items."),
                Some(vec![required_prompt_argument(
                    "path",
                    "Vault-relative Markdown path, for example Projects/Rust.md.",
                )]),
            )
            .with_title("Summarize note"),
            Prompt::new(
                "search_and_synthesize",
                Some("Search the vault and synthesize the most relevant note context."),
                Some(vec![
                    required_prompt_argument("query", "Text to search for in the vault."),
                    optional_prompt_argument(
                        "directory",
                        "Optional vault-relative directory to narrow the search.",
                    ),
                ]),
            )
            .with_title("Search and synthesize"),
            Prompt::new(
                "draft_note_update",
                Some("Draft a safe update to an existing or new Obsidian note."),
                Some(vec![
                    required_prompt_argument(
                        "path",
                        "Vault-relative Markdown path to read or update.",
                    ),
                    required_prompt_argument(
                        "intent",
                        "What the user wants to add, change, or capture.",
                    ),
                ]),
            )
            .with_title("Draft note update"),
            Prompt::new(
                "daily_review",
                Some("Review today's daily note and prepare a grounded plan."),
                None,
            )
            .with_title("Daily review"),
            Prompt::new(
                "tag_overview",
                Some("Summarize how a tag is used across the vault."),
                Some(vec![required_prompt_argument(
                    "tag",
                    "Tag to investigate, with or without leading #.",
                )]),
            )
            .with_title("Tag overview"),
            Prompt::new(
                "backlink_review",
                Some("Review backlinks for one note and identify related context."),
                Some(vec![required_prompt_argument(
                    "path",
                    "Vault-relative Markdown path, for example Projects/Rust.md.",
                )]),
            )
            .with_title("Backlink review"),
        ]
    }

    pub fn get_prompt_result(&self, request: GetPromptRequestParams) -> AppResult<GetPromptResult> {
        match request.name.as_str() {
            "summarize_note" => {
                let path = required_prompt_string(&request, "path")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let uri = ObsidianResourceUri::note(&normalized_path);
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Read the Obsidian note resource `{uri}` and summarize it. Include: concise summary, key facts, open questions, and action items. Do not modify the vault."
                    ),
                )])
                .with_description("Summarize one Obsidian note."))
            }
            "search_and_synthesize" => {
                let query = required_prompt_string(&request, "query")?;
                let directory = optional_prompt_string(&request, "directory");
                let directory_instruction = directory
                    .as_deref()
                    .filter(|directory| !directory.trim().is_empty())
                    .map(|directory| format!(" Limit the search to `{directory}`."))
                    .unwrap_or_default();
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Use the `search_notes` tool to search for `{query}` in the Obsidian vault.{directory_instruction} Read the most relevant `obsidian://note/{{path}}` resources before answering. Cite note paths and keep the synthesis grounded in note contents."
                    ),
                )])
                .with_description("Search the vault and synthesize matching notes."))
            }
            "draft_note_update" => {
                let path = required_prompt_string(&request, "path")?;
                let intent = required_prompt_string(&request, "intent")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let uri = ObsidianResourceUri::note(&normalized_path);
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Prepare a Markdown update for `{}` based on this intent: {intent}\n\nFirst read `{uri}` if it exists. Draft the exact text to append or write. Do not call `write_note` or `append_note` until the user approves the final text.",
                        normalized_path.as_cli_arg()
                    ),
                )])
                .with_description("Draft a safe note update."))
            }
            "daily_review" => Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "Read `obsidian://daily/today`, summarize today's note, extract commitments and open loops, then propose a short prioritized plan. Do not modify the vault.",
            )])
            .with_description("Review today's daily note.")),
            "tag_overview" => {
                let tag = required_prompt_string(&request, "tag")?;
                let normalized_tag = if tag.trim_start().starts_with('#') {
                    tag.trim().to_string()
                } else {
                    format!("#{}", tag.trim())
                };
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Use `list_tags` and `search_notes` to investigate `{normalized_tag}`. Read relevant `obsidian://note/{{path}}` resources, then summarize the theme, key notes, stale items, and suggested cleanup. Do not modify the vault."
                    ),
                )])
                .with_description("Summarize tag usage across the vault."))
            }
            "backlink_review" => {
                let path = required_prompt_string(&request, "path")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let backlinks_uri = ObsidianResourceUri::backlinks(&normalized_path);
                let note_uri = ObsidianResourceUri::note(&normalized_path);
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Read `{backlinks_uri}` and the target note `{note_uri}`. Summarize incoming context, important relationships, and follow-up notes worth reading. Do not modify the vault."
                    ),
                )])
                .with_description("Review backlinks for one note."))
            }
            _ => Err(ObsidianMcpError::ResourceNotFound(format!(
                "Unknown Obsidian prompt: {}",
                request.name
            ))),
        }
    }

    async fn read_note_content_at(&self, path: &VaultRelativePath) -> AppResult<String> {
        self.run_cli(ObsidianCommand::new("read").parameter("path", path.as_cli_arg()))
            .await
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

#[tool_router]
impl ObsidianMcp {
    #[tool(
        description = "Return the configured Obsidian vault path, Obsidian-reported vault identity, and Markdown note count.",
        annotations(
            title = "Vault info",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn vault_info(&self) -> Result<Json<VaultInfoResponse>, String> {
        self.vault_info_data()
            .await
            .map(Json)
            .map_err(error_message)
    }

    #[tool(
        description = "List Markdown notes in the vault or in a relative vault directory.",
        annotations(
            title = "List notes",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_notes(
        &self,
        Parameters(ListNotesRequest { directory, limit }): Parameters<ListNotesRequest>,
    ) -> Result<Json<ListNotesResponse>, String> {
        let notes = self
            .list_note_paths(directory.as_deref(), limit)
            .await
            .map_err(error_message)?;
        Ok(Json(ListNotesResponse {
            directory,
            count: notes.len(),
            notes,
        }))
    }

    #[tool(
        description = "Read a Markdown note by relative vault path.",
        annotations(
            title = "Read note",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_note(
        &self,
        Parameters(ReadNoteRequest { path }): Parameters<ReadNoteRequest>,
    ) -> Result<Json<ReadNoteResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let content = self
            .read_note_content_at(&normalized_path)
            .await
            .map_err(error_message)?;
        Ok(Json(ReadNoteResponse {
            path: normalized_path.as_cli_arg(),
            content,
        }))
    }

    #[tool(
        description = "Create or explicitly overwrite a Markdown note by relative vault path.",
        annotations(
            title = "Write note",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn write_note(
        &self,
        Parameters(WriteNoteRequest {
            path,
            content,
            overwrite,
        }): Parameters<WriteNoteRequest>,
    ) -> Result<Json<WriteNoteResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let overwrite = overwrite.unwrap_or(false);
        let message = self
            .write_note_content(&normalized_path.as_cli_arg(), &content, overwrite)
            .await
            .map_err(error_message)?;
        Ok(Json(WriteNoteResponse {
            path: normalized_path.as_cli_arg(),
            overwritten: overwrite,
            message,
        }))
    }

    #[tool(
        description = "Append text to a Markdown note by relative vault path.",
        annotations(
            title = "Append note",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn append_note(
        &self,
        Parameters(AppendNoteRequest { path, content }): Parameters<AppendNoteRequest>,
    ) -> Result<Json<AppendNoteResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let message = self
            .append_note_content(&normalized_path.as_cli_arg(), &content)
            .await
            .map_err(error_message)?;
        Ok(Json(AppendNoteResponse {
            path: normalized_path.as_cli_arg(),
            message,
        }))
    }

    #[tool(
        description = "Search Markdown notes for a text query using Obsidian search context.",
        annotations(
            title = "Search notes",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn search_notes(
        &self,
        Parameters(SearchNotesRequest {
            query,
            directory,
            limit,
        }): Parameters<SearchNotesRequest>,
    ) -> Result<Json<SearchNotesResponse>, String> {
        let matches = self
            .search_note_contents(&query, directory.as_deref(), limit)
            .await
            .map_err(error_message)?;
        Ok(Json(SearchNotesResponse {
            query: query.trim().to_string(),
            directory,
            count: matches.len(),
            matches,
        }))
    }

    #[tool(
        description = "List tags in the vault or in one Markdown note.",
        annotations(
            title = "List tags",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_tags(
        &self,
        Parameters(ListTagsRequest {
            path,
            counts,
            sort_by_count,
            limit,
        }): Parameters<ListTagsRequest>,
    ) -> Result<Json<ListTagsResponse>, String> {
        let tags = self
            .list_tags_data(
                path.as_deref(),
                counts.unwrap_or(false),
                sort_by_count.unwrap_or(false),
                limit,
            )
            .await
            .map_err(error_message)?;
        Ok(Json(ListTagsResponse {
            path,
            count: tags.len(),
            tags,
        }))
    }

    #[tool(
        description = "List backlinks to a Markdown note by relative vault path.",
        annotations(
            title = "List backlinks",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_backlinks(
        &self,
        Parameters(ListBacklinksRequest {
            path,
            counts,
            limit,
        }): Parameters<ListBacklinksRequest>,
    ) -> Result<Json<ListBacklinksResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let backlinks = self
            .list_backlinks_data(
                &normalized_path.as_cli_arg(),
                counts.unwrap_or(false),
                limit,
            )
            .await
            .map_err(error_message)?;
        Ok(Json(ListBacklinksResponse {
            path: normalized_path.as_cli_arg(),
            count: backlinks.len(),
            backlinks,
        }))
    }

    #[tool(
        description = "Read today's Obsidian daily note.",
        annotations(
            title = "Read daily note",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_daily_note(&self) -> Result<Json<ReadDailyNoteResponse>, String> {
        let content = self
            .read_daily_note_content()
            .await
            .map_err(error_message)?;
        Ok(Json(ReadDailyNoteResponse { content }))
    }

    #[tool(
        description = "Append Markdown text to today's Obsidian daily note.",
        annotations(
            title = "Append daily note",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn append_daily_note(
        &self,
        Parameters(AppendDailyNoteRequest { content, inline }): Parameters<AppendDailyNoteRequest>,
    ) -> Result<Json<AppendDailyNoteResponse>, String> {
        let message = self
            .append_daily_note_content(&content, inline.unwrap_or(false))
            .await
            .map_err(error_message)?;
        Ok(Json(AppendDailyNoteResponse { message }))
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
            .with_instructions("Use these tools, resources, and prompts to read, create, append, list, and search Markdown notes through the Obsidian CLI. Obsidian must be running with the CLI enabled. Paths must be relative to the configured vault.")
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

fn required_prompt_argument(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(true)
}

fn optional_prompt_argument(name: &str, description: &str) -> PromptArgument {
    PromptArgument::new(name)
        .with_description(description)
        .with_required(false)
}

fn required_prompt_string(request: &GetPromptRequestParams, name: &str) -> AppResult<String> {
    optional_prompt_string(request, name)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ObsidianMcpError::InvalidInput(format!(
                "Prompt '{}' requires argument '{name}'",
                request.name
            ))
        })
}

fn optional_prompt_string(request: &GetPromptRequestParams, name: &str) -> Option<String> {
    request
        .arguments
        .as_ref()
        .and_then(|arguments| arguments.get(name))
        .and_then(|value| value.as_str())
        .map(str::to_string)
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

fn path_to_cli_arg(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
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

fn has_markdown_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn clamp_limit(limit: Option<usize>, default: usize, maximum: usize) -> usize {
    limit.unwrap_or(default).min(maximum)
}

fn encode_cli_text(content: &str) -> String {
    content
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn format_command(program: &OsStr, args: &[OsString]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(OsString::as_os_str))
        .map(display_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_arg(arg: &OsStr) -> String {
    let value = arg.to_string_lossy();
    if value.contains(char::is_whitespace) {
        format!("{value:?}")
    } else {
        value.into_owned()
    }
}

fn first_non_empty<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn truncate_error(message: &str) -> String {
    const MAX_CHARS: usize = 1_000;
    let mut chars = message.trim().chars();
    let truncated: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        fs,
        sync::{Arc, Mutex},
    };

    #[tokio::test]
    async fn rejects_paths_that_escape_vault() {
        let vault = TestVault::new();
        let cli = FakeObsidianCli::default();
        let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

        assert!(server.read_note_content("../secret.md").await.is_err());
        assert!(
            server
                .write_note_content("/tmp/secret.md", "", true)
                .await
                .is_err()
        );
    }

    #[test]
    fn vault_relative_path_normalizes_and_validates_paths() {
        assert_eq!(
            VaultRelativePath::markdown(r"Projects\Rust.md")
                .unwrap()
                .as_cli_arg(),
            "Projects/Rust.md"
        );
        assert_eq!(
            VaultRelativePath::parse("./Projects/../Rust.md")
                .unwrap_err()
                .to_string(),
            "path cannot escape the vault"
        );
        assert_eq!(
            VaultRelativePath::markdown("Projects/Rust.txt")
                .unwrap_err()
                .to_string(),
            "Only Markdown notes with the .md extension are supported"
        );
        assert_eq!(
            VaultRelativePath::parse("/tmp/Rust.md")
                .unwrap_err()
                .to_string(),
            "path must be relative to the vault"
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
            .write_note_content("Projects/Rust.md", "Rust MCP\nSecond line", false)
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

        let result = server.write_note_content("image.png", "", false).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn write_without_overwrite_refuses_existing_note() {
        let vault = TestVault::new();
        let cli = FakeObsidianCli::new([Ok("Projects/Rust.md")]);
        let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

        let result = server
            .write_note_content("Projects/Rust.md", "new content", false)
            .await;

        assert_eq!(
            result.unwrap_err().to_string(),
            "Note already exists; pass overwrite=true to replace it"
        );
        assert_eq!(cli.calls().len(), 1);
    }

    #[tokio::test]
    async fn write_with_overwrite_skips_existing_note_preflight() {
        let vault = TestVault::new();
        let cli = FakeObsidianCli::new([Ok("created")]);
        let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

        server
            .write_note_content("Projects/Rust.md", "new content", true)
            .await
            .unwrap();

        assert_eq!(
            cli.calls()[0].args,
            vec![
                "create",
                "path=Projects/Rust.md",
                "content=new content",
                "overwrite",
            ]
        );
    }

    #[tokio::test]
    async fn encodes_multiline_content_for_cli_arguments() {
        let vault = TestVault::new();
        let cli = FakeObsidianCli::new([Err("missing"), Ok("created")]);
        let server = ObsidianMcp::with_runner(vault.path(), cli.clone()).unwrap();

        server
            .write_note_content("Projects/Rust.md", "a\nb\tc\\d", false)
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
        assert!(uris.contains(&"obsidian://notes/index"));
        assert!(uris.contains(&"obsidian://tags/index"));
        assert!(uris.contains(&"obsidian://daily/today"));
        assert!(uris.contains(&"obsidian://note/Projects/Rust.md"));
        assert!(uris.contains(&"obsidian://note/Space%20Note.md"));
        assert!(uris.contains(&"obsidian://backlinks/Projects/Rust.md"));
        assert!(!uris.iter().any(|uri| uri.contains("image.png")));
    }

    #[test]
    fn resource_templates_expose_note_uri_template() {
        let vault = TestVault::new();
        let cli = FakeObsidianCli::default();
        let server = ObsidianMcp::with_runner(vault.path(), cli).unwrap();

        let templates = server.list_resource_template_descriptors();

        assert_eq!(templates.len(), 2);
        assert_eq!(templates[0].uri_template, "obsidian://note/{path}");
        assert_eq!(templates[0].mime_type.as_deref(), Some("text/markdown"));
        assert_eq!(templates[1].uri_template, "obsidian://backlinks/{path}");
        assert_eq!(templates[1].mime_type.as_deref(), Some("text/plain"));
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

    #[test]
    fn resource_uri_round_trips_percent_encoded_note_paths() {
        let path = VaultRelativePath::markdown("Folder/Space Note.md").unwrap();
        let uri = ObsidianResourceUri::note(&path);

        assert_eq!(uri, "obsidian://note/Folder/Space%20Note.md");
        assert_eq!(
            ObsidianResourceUri::parse(&uri).unwrap(),
            ObsidianResourceUri::Note(path)
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
                "tag_overview",
                "backlink_review"
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
    #[ignore = "requires Obsidian to be running with CLI enabled and OBSIDIAN_VAULT_PATH set"]
    async fn real_cli_smoke_vault_info() {
        let vault = env::var_os("OBSIDIAN_VAULT_PATH").expect("OBSIDIAN_VAULT_PATH must be set");
        let server = ObsidianMcp::new(PathBuf::from(vault)).unwrap();

        server.vault_info_data().await.unwrap();
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
}
