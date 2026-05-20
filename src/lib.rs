use std::{
    env,
    ffi::{OsStr, OsString},
    future::Future,
    path::{Component, Path, PathBuf},
    pin::Pin,
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use rmcp::{
    ServerHandler,
    handler::server::{
        router::tool::ToolRouter,
        wrapper::{Json, Parameters},
    },
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};

type CliFuture<'a> = Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>>;

trait ObsidianCliRunner: std::fmt::Debug + Send + Sync {
    fn run<'a>(&'a self, vault: &'a Path, args: Vec<OsString>) -> CliFuture<'a>;
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
    ) -> Result<String, String> {
        let command_text = format_command(&program, &args);
        let mut child = Command::new(&program)
            .current_dir(vault)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    format!(
                        "Cannot run Obsidian CLI '{}': command not found. Install or enable the Obsidian CLI, or set OBSIDIAN_CLI to the CLI path.",
                        program.to_string_lossy()
                    )
                } else {
                    format!("Cannot run Obsidian CLI command '{command_text}': {error}")
                }
            })?;

        let started_at = Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|error| {
                    format!("Cannot wait for Obsidian CLI command '{command_text}': {error}")
                })?
                .is_some()
            {
                let output = child.wait_with_output().map_err(|error| {
                    format!("Cannot collect Obsidian CLI output for '{command_text}': {error}")
                })?;

                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                if output.status.success() {
                    return Ok(stdout);
                }

                let stderr = String::from_utf8_lossy(&output.stderr);
                let details = first_non_empty([stderr.as_ref(), stdout.as_str()])
                    .map(truncate_error)
                    .unwrap_or_else(|| format!("exit status {}", output.status));
                return Err(format!(
                    "Obsidian CLI command failed: {command_text}\n{details}"
                ));
            }

            if started_at.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "Obsidian CLI command timed out after {}s: {command_text}",
                    timeout.as_secs()
                ));
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
                .map_err(|error| format!("Obsidian CLI worker failed: {error}"))?
        })
    }
}

#[derive(Debug, Clone)]
pub struct ObsidianMcp {
    vault: Arc<PathBuf>,
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

impl ObsidianMcp {
    pub fn from_env() -> Result<Self, String> {
        let path = env::var_os("OBSIDIAN_VAULT_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(Self::default_vault_path);
        Self::new(path)
    }

    pub fn new(vault: impl Into<PathBuf>) -> Result<Self, String> {
        Self::with_runner(vault, RealObsidianCli::from_env())
    }

    pub fn default_vault_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("obsidian-vault")
    }

    fn with_runner<R>(vault: impl Into<PathBuf>, cli: R) -> Result<Self, String>
    where
        R: ObsidianCliRunner + 'static,
    {
        let vault = vault.into();
        let vault = vault
            .canonicalize()
            .map_err(|error| format!("Cannot access vault path '{}': {error}", vault.display()))?;

        if !vault.is_dir() {
            return Err(format!(
                "Vault path '{}' is not a directory",
                vault.display()
            ));
        }

        Ok(Self {
            vault: Arc::new(vault),
            cli: Arc::new(cli),
            tool_router: Self::tool_router(),
        })
    }

    pub fn vault_path(&self) -> &Path {
        self.vault.as_ref()
    }

    pub async fn vault_info_data(&self) -> Result<VaultInfoResponse, String> {
        let obsidian_vault_path = self
            .run_cli(vec!["vault".into(), "info=path".into()])
            .await?
            .trim()
            .to_string();
        let obsidian_vault_name = self
            .run_cli(vec!["vault".into(), "info=name".into()])
            .await?
            .trim()
            .to_string();
        let markdown_notes = parse_count(
            &self
                .run_cli(vec!["files".into(), "ext=md".into(), "total".into()])
                .await?,
        )?;

        Ok(VaultInfoResponse {
            configured_vault_path: self.vault_path().display().to_string(),
            obsidian_vault_path,
            obsidian_vault_name,
            markdown_notes,
        })
    }

    pub async fn list_note_paths(
        &self,
        directory: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<String>, String> {
        let directory = self.safe_directory(directory)?;
        let mut args = vec!["files".into(), "ext=md".into()];
        if let Some(directory) = &directory {
            args.push(format!("folder={directory}").into());
        }

        let mut notes = parse_output_lines(&self.run_cli(args).await?);
        notes.retain(|note| has_markdown_extension(note));
        notes.sort();
        notes.truncate(clamp_limit(limit, 200, 2_000));
        Ok(notes)
    }

    pub async fn read_note_content(&self, path: &str) -> Result<String, String> {
        let path = self.safe_note_path(path)?;
        self.run_cli(vec!["read".into(), format!("path={path}").into()])
            .await
    }

    pub async fn write_note_content(
        &self,
        path: &str,
        content: &str,
        overwrite: bool,
    ) -> Result<String, String> {
        let path = self.safe_note_path(path)?;
        if !overwrite
            && self
                .run_cli(vec!["file".into(), format!("path={path}").into()])
                .await
                .is_ok()
        {
            return Err("Note already exists; pass overwrite=true to replace it".to_string());
        }

        let mut args = vec![
            "create".into(),
            format!("path={path}").into(),
            format!("content={}", encode_cli_text(content)).into(),
        ];
        if overwrite {
            args.push("overwrite".into());
        }

        self.run_cli(args).await?;
        Ok(format!("Wrote {path}"))
    }

    pub async fn append_note_content(&self, path: &str, content: &str) -> Result<String, String> {
        let path = self.safe_note_path(path)?;
        self.run_cli(vec![
            "append".into(),
            format!("path={path}").into(),
            format!("content={}", encode_cli_text(content)).into(),
            "inline".into(),
        ])
        .await?;

        Ok(format!("Appended to {path}"))
    }

    pub async fn search_note_contents(
        &self,
        query: &str,
        directory: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<String>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("query cannot be empty".to_string());
        }

        let directory = self.safe_directory(directory)?;
        let limit = clamp_limit(limit, 50, 500);
        let mut args = vec![
            "search:context".into(),
            format!("query={query}").into(),
            format!("limit={limit}").into(),
        ];
        if let Some(directory) = &directory {
            args.push(format!("path={directory}").into());
        }

        let mut matches = parse_output_lines(&self.run_cli(args).await?);
        matches.truncate(limit);
        Ok(matches)
    }

    async fn run_cli(&self, args: Vec<OsString>) -> Result<String, String> {
        self.cli.run(self.vault_path(), args).await
    }

    fn safe_note_path(&self, path: &str) -> Result<String, String> {
        let path = self.safe_relative_path(path)?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();

        if !extension.eq_ignore_ascii_case("md") {
            return Err("Only Markdown notes with the .md extension are supported".to_string());
        }

        Ok(path_to_cli_arg(&path))
    }

    fn safe_directory(&self, directory: Option<&str>) -> Result<Option<String>, String> {
        match directory
            .map(str::trim)
            .filter(|directory| !directory.is_empty())
        {
            Some(directory) => Ok(Some(path_to_cli_arg(&self.safe_relative_path(directory)?))),
            None => Ok(None),
        }
    }

    fn safe_relative_path(&self, raw_path: &str) -> Result<PathBuf, String> {
        let normalized = raw_path.trim().replace('\\', "/");
        if normalized.is_empty() {
            return Err("path cannot be empty".to_string());
        }

        let path = Path::new(&normalized);
        if path.is_absolute() {
            return Err("path must be relative to the vault".to_string());
        }

        let mut safe_path = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(segment) => safe_path.push(segment),
                Component::CurDir => {}
                _ => return Err("path cannot escape the vault".to_string()),
            }
        }

        if safe_path.as_os_str().is_empty() {
            return Err("path cannot be empty".to_string());
        }

        Ok(safe_path)
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
        self.vault_info_data().await.map(Json)
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
        let notes = self.list_note_paths(directory.as_deref(), limit).await?;
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
        let normalized_path = self.safe_note_path(&path)?;
        let content = self.read_note_content(&normalized_path).await?;
        Ok(Json(ReadNoteResponse {
            path: normalized_path,
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
        let normalized_path = self.safe_note_path(&path)?;
        let overwrite = overwrite.unwrap_or(false);
        let message = self
            .write_note_content(&normalized_path, &content, overwrite)
            .await?;
        Ok(Json(WriteNoteResponse {
            path: normalized_path,
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
        let normalized_path = self.safe_note_path(&path)?;
        let message = self.append_note_content(&normalized_path, &content).await?;
        Ok(Json(AppendNoteResponse {
            path: normalized_path,
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
            .await?;
        Ok(Json(SearchNotesResponse {
            query: query.trim().to_string(),
            directory,
            count: matches.len(),
            matches,
        }))
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for ObsidianMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions("Use these tools to read, create, append, list, and search Markdown notes through the Obsidian CLI. Obsidian must be running with the CLI enabled. Paths must be relative to the configured vault.")
    }
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

fn parse_count(output: &str) -> Result<usize, String> {
    output
        .split_whitespace()
        .filter_map(|word| word.parse::<usize>().ok())
        .next_back()
        .ok_or_else(|| {
            format!(
                "Cannot parse Markdown note count from Obsidian CLI output: {}",
                truncate_error(output)
            )
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
            result.unwrap_err(),
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
    async fn vault_info_uses_cli_metadata_and_total_count() {
        let vault = TestVault::new();
        let cli = FakeObsidianCli::new([
            Ok("/Users/example/Vault\n"),
            Ok("Knowledge\n"),
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
            vec![
                vec!["vault", "info=path"],
                vec!["vault", "info=name"],
                vec!["files", "ext=md", "total"],
            ]
        );
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

    #[derive(Debug, Clone, Default)]
    struct FakeObsidianCli {
        calls: Arc<Mutex<Vec<FakeCall>>>,
        responses: Arc<Mutex<VecDeque<Result<String, String>>>>,
    }

    impl FakeObsidianCli {
        fn new<const N: usize>(responses: [Result<&str, &str>; N]) -> Self {
            Self {
                calls: Arc::default(),
                responses: Arc::new(Mutex::new(
                    responses
                        .into_iter()
                        .map(|result| result.map(str::to_string).map_err(str::to_string))
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
                .unwrap_or_else(|| Err("missing fake Obsidian CLI response".to_string()));

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
