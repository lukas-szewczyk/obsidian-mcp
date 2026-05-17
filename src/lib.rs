use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};

#[derive(Debug, Clone)]
pub struct ObsidianMcp {
    vault: Arc<PathBuf>,
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

impl ObsidianMcp {
    pub fn from_env() -> Result<Self, String> {
        let path = env::var_os("OBSIDIAN_VAULT_PATH")
            .ok_or("Set OBSIDIAN_VAULT_PATH to the Obsidian vault directory")?;
        Self::new(PathBuf::from(path))
    }

    pub fn new(vault: impl Into<PathBuf>) -> Result<Self, String> {
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
            tool_router: Self::tool_router(),
        })
    }

    pub fn vault_path(&self) -> &Path {
        self.vault.as_ref()
    }

    pub fn list_note_paths(
        &self,
        directory: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<String>, String> {
        let root = self.safe_directory(directory)?;
        let limit = clamp_limit(limit, 200, 2_000);
        let mut notes = Vec::new();
        collect_markdown_files(self.vault_path(), &root, limit, &mut notes)?;
        notes.sort();
        Ok(notes)
    }

    pub fn read_note_content(&self, path: &str) -> Result<String, String> {
        let path = self.safe_note_path(path)?;
        fs::read_to_string(&path).map_err(|error| format!("Cannot read note: {error}"))
    }

    pub fn write_note_content(
        &self,
        path: &str,
        content: &str,
        overwrite: bool,
    ) -> Result<String, String> {
        let path = self.safe_note_path(path)?;
        if path.exists() && !overwrite {
            return Err("Note already exists; pass overwrite=true to replace it".to_string());
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Cannot create note directory: {error}"))?;
        }

        fs::write(&path, content).map_err(|error| format!("Cannot write note: {error}"))?;
        Ok(format!("Wrote {}", self.relative_display(&path)))
    }

    pub fn append_note_content(&self, path: &str, content: &str) -> Result<String, String> {
        let path = self.safe_note_path(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Cannot create note directory: {error}"))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|error| format!("Cannot open note for append: {error}"))?;
        file.write_all(content.as_bytes())
            .map_err(|error| format!("Cannot append to note: {error}"))?;

        Ok(format!("Appended to {}", self.relative_display(&path)))
    }

    pub fn search_note_contents(
        &self,
        query: &str,
        directory: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<String>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("query cannot be empty".to_string());
        }

        let root = self.safe_directory(directory)?;
        let limit = clamp_limit(limit, 50, 500);
        let mut notes = Vec::new();
        collect_markdown_files(self.vault_path(), &root, usize::MAX, &mut notes)?;
        notes.sort();

        let needle = query.to_lowercase();
        let mut matches = Vec::new();

        for note in notes {
            if matches.len() >= limit {
                break;
            }

            let note_path = self.vault_path().join(&note);
            let content = match fs::read_to_string(&note_path) {
                Ok(content) => content,
                Err(_) => continue,
            };

            for (line_index, line) in content.lines().enumerate() {
                if line.to_lowercase().contains(&needle) {
                    matches.push(format!(
                        "{}:{}: {}",
                        note,
                        line_index + 1,
                        truncate_line(line.trim())
                    ));

                    if matches.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(matches)
    }

    fn safe_note_path(&self, path: &str) -> Result<PathBuf, String> {
        let path = self.safe_relative_path(path)?;
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();

        if !extension.eq_ignore_ascii_case("md") {
            return Err("Only Markdown notes with the .md extension are supported".to_string());
        }

        Ok(self.vault_path().join(path))
    }

    fn safe_directory(&self, directory: Option<&str>) -> Result<PathBuf, String> {
        match directory
            .map(str::trim)
            .filter(|directory| !directory.is_empty())
        {
            Some(directory) => Ok(self.vault_path().join(self.safe_relative_path(directory)?)),
            None => Ok(self.vault_path().to_path_buf()),
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

    fn relative_display(&self, path: &Path) -> String {
        path.strip_prefix(self.vault_path())
            .unwrap_or(path)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
    }
}

#[tool_router]
impl ObsidianMcp {
    #[tool(description = "Return the configured Obsidian vault path and Markdown note count.")]
    fn vault_info(&self) -> Result<String, String> {
        let note_count = self.list_note_paths(None, Some(usize::MAX))?.len();
        Ok(format!(
            "Vault: {}\nMarkdown notes: {note_count}",
            self.vault_path().display()
        ))
    }

    #[tool(description = "List Markdown notes in the vault or in a relative vault directory.")]
    fn list_notes(
        &self,
        Parameters(ListNotesRequest { directory, limit }): Parameters<ListNotesRequest>,
    ) -> Result<String, String> {
        let notes = self.list_note_paths(directory.as_deref(), limit)?;
        if notes.is_empty() {
            Ok("No Markdown notes found".to_string())
        } else {
            Ok(notes.join("\n"))
        }
    }

    #[tool(description = "Read a Markdown note by relative vault path.")]
    fn read_note(
        &self,
        Parameters(ReadNoteRequest { path }): Parameters<ReadNoteRequest>,
    ) -> Result<String, String> {
        self.read_note_content(&path)
    }

    #[tool(description = "Create or overwrite a Markdown note by relative vault path.")]
    fn write_note(
        &self,
        Parameters(WriteNoteRequest {
            path,
            content,
            overwrite,
        }): Parameters<WriteNoteRequest>,
    ) -> Result<String, String> {
        self.write_note_content(&path, &content, overwrite.unwrap_or(false))
    }

    #[tool(description = "Append text to a Markdown note by relative vault path.")]
    fn append_note(
        &self,
        Parameters(AppendNoteRequest { path, content }): Parameters<AppendNoteRequest>,
    ) -> Result<String, String> {
        self.append_note_content(&path, &content)
    }

    #[tool(description = "Search Markdown notes for a case-insensitive text query.")]
    fn search_notes(
        &self,
        Parameters(SearchNotesRequest {
            query,
            directory,
            limit,
        }): Parameters<SearchNotesRequest>,
    ) -> Result<String, String> {
        let matches = self.search_note_contents(&query, directory.as_deref(), limit)?;
        if matches.is_empty() {
            Ok("No matches found".to_string())
        } else {
            Ok(matches.join("\n"))
        }
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
            .with_instructions("Use these tools to read, create, append, list, and search Markdown notes inside the configured Obsidian vault. Paths must be relative to the vault.")
    }
}

fn collect_markdown_files(
    vault: &Path,
    directory: &Path,
    limit: usize,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    if notes.len() >= limit {
        return Ok(());
    }

    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Cannot read directory '{}': {error}", directory.display()))?;
    let mut entries = entries.collect::<Result<Vec<_>, _>>().map_err(|error| {
        format!(
            "Cannot read directory entry in '{}': {error}",
            directory.display()
        )
    })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        if notes.len() >= limit {
            break;
        }

        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Cannot inspect '{}': {error}", path.display()))?;

        if file_type.is_dir() {
            collect_markdown_files(vault, &path, limit, notes)?;
        } else if file_type.is_file() && has_markdown_extension(&path) {
            let relative = path
                .strip_prefix(vault)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            notes.push(relative);
        }
    }

    Ok(())
}

fn has_markdown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn clamp_limit(limit: Option<usize>, default: usize, maximum: usize) -> usize {
    limit.unwrap_or(default).min(maximum)
}

fn truncate_line(line: &str) -> String {
    const MAX_CHARS: usize = 160;
    let mut chars = line.chars();
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

    #[test]
    fn rejects_paths_that_escape_vault() {
        let vault = TestVault::new();
        let server = ObsidianMcp::new(vault.path()).unwrap();

        assert!(server.read_note_content("../secret.md").is_err());
        assert!(
            server
                .write_note_content("/tmp/secret.md", "", true)
                .is_err()
        );
    }

    #[test]
    fn writes_reads_lists_and_searches_notes() {
        let vault = TestVault::new();
        let server = ObsidianMcp::new(vault.path()).unwrap();

        server
            .write_note_content("Projects/Rust.md", "Rust MCP\nSecond line", false)
            .unwrap();
        server
            .append_note_content("Projects/Rust.md", "\nObsidian vault")
            .unwrap();

        let content = server.read_note_content("Projects/Rust.md").unwrap();
        assert!(content.contains("Rust MCP"));
        assert!(content.contains("Obsidian vault"));

        let notes = server.list_note_paths(Some("Projects"), Some(10)).unwrap();
        assert_eq!(notes, vec!["Projects/Rust.md"]);

        let matches = server
            .search_note_contents("obsidian", Some("Projects"), Some(10))
            .unwrap();
        assert_eq!(matches, vec!["Projects/Rust.md:3: Obsidian vault"]);
    }

    #[test]
    fn refuses_non_markdown_writes() {
        let vault = TestVault::new();
        let server = ObsidianMcp::new(vault.path()).unwrap();

        let result = server.write_note_content("image.png", "", false);
        assert!(result.is_err());
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
