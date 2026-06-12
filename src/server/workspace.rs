use super::*;

const WORKOS_CONTRACT: &str = "workos.v1";
const DAILY_PATH_FORMAT: &str = "%Y-%m-%d.md";
const TASK_DATE_SYNTAX: [&str; 2] = ["tasks-emoji", "dataview"];

impl ObsidianMcp {
    pub async fn workspace_profile_data(&self) -> AppResult<WorkspaceProfileResponse> {
        let mut warnings = Vec::new();

        let vault = parse_vault_overview(&self.run_cli(ObsidianCommand::new("vault")).await?)?;

        let sync_status = match self.run_cli(ObsidianCommand::new("sync:status")).await {
            Ok(output) => parse_sync_status(&output),
            Err(error) => {
                warnings.push(format!("sync status unavailable: {error}"));
                None
            }
        };

        let obsidian_version = match self.run_cli(ObsidianCommand::new("version")).await {
            Ok(output) => first_non_empty([output.as_str()]).map(str::to_string),
            Err(error) => {
                warnings.push(format!("Obsidian version unavailable: {error}"));
                None
            }
        };

        let bases = match self.list_bases_data(Some(100)).await {
            Ok(bases) => bases,
            Err(error) => {
                warnings.push(format!("bases unavailable: {error}"));
                Vec::new()
            }
        };

        let daily = match self.run_cli(ObsidianCommand::new("daily:path")).await {
            Ok(_) => true,
            Err(error) => {
                warnings.push(format!("daily notes unavailable: {error}"));
                false
            }
        };

        let projects_dir = project_directory_from_env();
        let projects = self.folder_exists(&projects_dir).await;

        Ok(WorkspaceProfileResponse {
            contract: WORKOS_CONTRACT.to_string(),
            server: ProfileServer {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            vault: ProfileVault {
                name: vault.name,
                path: vault.path,
                files: vault.files,
                folders: vault.folders,
            },
            sync: ProfileSync {
                status: sync_status,
            },
            conventions: ProfileConventions {
                projects_dir,
                daily_path_format: DAILY_PATH_FORMAT.to_string(),
                task_date_syntax: TASK_DATE_SYNTAX.map(str::to_string).to_vec(),
            },
            capabilities: ProfileCapabilities {
                projects,
                daily,
                bases: !bases.is_empty(),
            },
            bases,
            system: ProfileSystem {
                obsidian_version,
                warnings,
            },
        })
    }

    async fn folder_exists(&self, directory: &str) -> bool {
        let Ok(directory) = VaultRelativePath::parse(directory) else {
            return false;
        };

        self.run_cli(
            ObsidianCommand::new("folder")
                .parameter("path", directory.as_cli_arg())
                .parameter("info", "files"),
        )
        .await
        .is_ok()
    }
}

struct VaultOverview {
    name: String,
    path: String,
    files: usize,
    folders: usize,
}

fn parse_vault_overview(output: &str) -> AppResult<VaultOverview> {
    let mut name = None;
    let mut path = None;
    let mut files = None;
    let mut folders = None;

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
            "files" => files = value.parse::<usize>().ok(),
            "folders" => folders = value.parse::<usize>().ok(),
            _ => {}
        }
    }

    match (name, path, files, folders) {
        (Some(name), Some(path), Some(files), Some(folders)) => Ok(VaultOverview {
            name,
            path,
            files,
            folders,
        }),
        _ => Err(ObsidianMcpError::Parse(format!(
            "Cannot parse vault overview from Obsidian CLI output: {}",
            truncate_error(output)
        ))),
    }
}

fn parse_sync_status(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("status:"))
        .map(|status| status.trim().to_string())
        .filter(|status| !status.is_empty())
        .or_else(|| first_non_empty([output]).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vault_overview_and_sync_status() {
        let overview = parse_vault_overview(
            "name\ttest-vault\npath\t/Users/me/test-vault\nfiles\t2\nfolders\t0\nsize\t209\n",
        )
        .unwrap();
        assert_eq!(overview.name, "test-vault");
        assert_eq!(overview.path, "/Users/me/test-vault");
        assert_eq!(overview.files, 2);
        assert_eq!(overview.folders, 0);

        assert!(matches!(
            parse_vault_overview("name only-name"),
            Err(ObsidianMcpError::Parse(_))
        ));

        assert_eq!(
            parse_sync_status("status: disconnected\nSync is not set up for this vault."),
            Some("disconnected".to_string())
        );
        assert_eq!(
            parse_sync_status("Everything is up to date."),
            Some("Everything is up to date.".to_string())
        );
        assert_eq!(parse_sync_status("  \n"), None);
    }
}
