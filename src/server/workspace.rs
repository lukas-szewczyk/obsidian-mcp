use super::{
    work_system::{task_due_date, task_scheduled_date},
    *,
};

const WORKOS_CONTRACT: &str = "workos.v1";
const DAILY_PATH_FORMAT: &str = "%Y-%m-%d.md";
const TASK_DATE_SYNTAX: [&str; 2] = ["tasks-emoji", "dataview"];
const TASKS_LIMIT: usize = 1_000;
const INDEX_LIMIT: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DueDateFilter {
    On,
    Before,
}

impl DueDateFilter {
    fn as_str(self) -> &'static str {
        match self {
            Self::On => "due_on",
            Self::Before => "due_before",
        }
    }
}

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

    pub async fn workspace_today_data(&self) -> AppResult<WorkspaceTodayResponse> {
        let daily_path = self
            .run_cli(ObsidianCommand::new("daily:path"))
            .await?
            .trim()
            .to_string();
        let today = daily_date_from_path(&daily_path)?;

        let (exists, content) = match self.run_cli(ObsidianCommand::new("daily:read")).await {
            Ok(content) => (true, Some(content)),
            Err(ObsidianMcpError::NoteNotFound(_)) => (false, None),
            Err(error) => return Err(error),
        };

        let open_tasks = self
            .list_tasks_data(&TaskReadTarget::Vault, Some(&TaskStatus::Todo), Some(1_000))
            .await?;
        let open_total = open_tasks.len();

        let mut due_today = Vec::new();
        let mut overdue = Vec::new();
        for task in &open_tasks {
            let Some(due) = task_due_date(&task.text) else {
                continue;
            };
            if due == today {
                due_today.push(normalize_task(task));
            } else if due < today {
                overdue.push(normalize_task(task));
            }
        }
        sort_tasks(&mut due_today);
        sort_tasks(&mut overdue);

        let in_daily_note = if exists {
            match self
                .list_tasks_data(&TaskReadTarget::Daily, Some(&TaskStatus::Todo), Some(1_000))
                .await
            {
                Ok(tasks) => tasks.iter().map(normalize_task).collect(),
                Err(ObsidianMcpError::NoteNotFound(_)) => Vec::new(),
                Err(error) => return Err(error),
            }
        } else {
            Vec::new()
        };

        Ok(WorkspaceTodayResponse {
            contract: WORKOS_CONTRACT.to_string(),
            date: today.to_string(),
            daily_note: DailyNote {
                path: daily_path,
                exists,
                content,
            },
            counts: TodayCounts {
                due_today: due_today.len(),
                overdue: overdue.len(),
                open_total,
            },
            tasks: TodayTasks {
                due_today,
                overdue,
                in_daily_note,
            },
        })
    }
    pub async fn open_tasks_resource_data(&self) -> AppResult<TasksResource> {
        let open_tasks = self
            .list_tasks_data(
                &TaskReadTarget::Vault,
                Some(&TaskStatus::Todo),
                Some(TASKS_LIMIT),
            )
            .await?;
        let truncated = open_tasks.len() >= TASKS_LIMIT;
        let tasks: Vec<Task> = open_tasks.iter().map(normalize_task).collect();

        Ok(TasksResource {
            contract: WORKOS_CONTRACT.to_string(),
            count: tasks.len(),
            truncated,
            tasks,
        })
    }

    pub(super) async fn dated_tasks_resource_data(
        &self,
        filter: DueDateFilter,
        date: &DailyDate,
    ) -> AppResult<DatedTasksResource> {
        let open_tasks = self
            .list_tasks_data(
                &TaskReadTarget::Vault,
                Some(&TaskStatus::Todo),
                Some(TASKS_LIMIT),
            )
            .await?;
        let truncated = open_tasks.len() >= TASKS_LIMIT;
        let mut tasks: Vec<Task> = open_tasks
            .iter()
            .filter(|task| {
                task_due_date(&task.text).is_some_and(|due| match filter {
                    DueDateFilter::On => due == *date,
                    DueDateFilter::Before => due < *date,
                })
            })
            .map(normalize_task)
            .collect();
        sort_tasks(&mut tasks);

        Ok(DatedTasksResource {
            contract: WORKOS_CONTRACT.to_string(),
            date: date.to_string(),
            op: filter.as_str().to_string(),
            count: tasks.len(),
            truncated,
            tasks,
        })
    }

    pub async fn projects_index_resource_data(&self) -> AppResult<ProjectsIndexResource> {
        let (_, paths) = self
            .list_project_note_paths(None, Some(INDEX_LIMIT))
            .await?;
        let truncated = paths.len() >= INDEX_LIMIT;
        let projects: Vec<ProjectIndexItem> = paths
            .into_iter()
            .map(|path| ProjectIndexItem {
                title: note_title(&path),
                path,
            })
            .collect();

        Ok(ProjectsIndexResource {
            contract: WORKOS_CONTRACT.to_string(),
            count: projects.len(),
            truncated,
            projects,
        })
    }

    pub(super) async fn note_context_resource_data(
        &self,
        path: &VaultRelativePath,
    ) -> AppResult<NoteContextResource> {
        let cli_path = path.as_cli_arg();
        let content = self.read_note_content_at(path).await?;
        let properties = self.list_properties_data(&cli_path).await?;
        let tasks = self
            .list_tasks_data(
                &TaskReadTarget::Note {
                    path: cli_path.clone(),
                },
                None,
                Some(TASKS_LIMIT),
            )
            .await?;
        let links = parse_output_lines(
            &self
                .run_cli(ObsidianCommand::new("links").parameter("path", &cli_path))
                .await?,
        );
        let backlinks = self
            .list_backlinks_data(&cli_path, true, Some(INDEX_LIMIT))
            .await?;
        let tags = self
            .list_tags_data(Some(&cli_path), false, false, None)
            .await?;

        Ok(NoteContextResource {
            contract: WORKOS_CONTRACT.to_string(),
            path: cli_path,
            content,
            properties: properties_object(&properties),
            tags,
            tasks: tasks.iter().map(normalize_task).collect(),
            links,
            backlinks: backlinks
                .iter()
                .map(|line| parse_backlink_line(line))
                .collect(),
        })
    }

    pub(super) async fn project_status_resource_data(
        &self,
        path: &VaultRelativePath,
    ) -> AppResult<ProjectStatusResource> {
        let cli_path = path.as_cli_arg();
        let properties = self.list_properties_data(&cli_path).await?;
        let open_tasks = self
            .list_tasks_data(
                &TaskReadTarget::Note {
                    path: cli_path.clone(),
                },
                Some(&TaskStatus::Todo),
                Some(TASKS_LIMIT),
            )
            .await?;
        let backlink_count = self.backlink_count(&cli_path).await?;

        let title = properties
            .iter()
            .find(|property| property.name == "title")
            .and_then(|property| property.value.as_str().map(str::to_string))
            .unwrap_or_else(|| note_title(&cli_path));
        let open_tasks: Vec<Task> = open_tasks.iter().map(normalize_task).collect();

        Ok(ProjectStatusResource {
            contract: WORKOS_CONTRACT.to_string(),
            path: cli_path,
            title,
            properties: properties_object(&properties),
            open_task_count: open_tasks.len(),
            open_tasks,
            backlink_count,
        })
    }

    pub async fn vault_audit_resource_data(&self) -> AppResult<VaultAuditResource> {
        let audit = self.audit_vault_data(Some(INDEX_LIMIT)).await?;
        let truncated = audit.unresolved_link_count >= INDEX_LIMIT
            || audit.orphan_note_count >= INDEX_LIMIT
            || audit.dead_end_count >= INDEX_LIMIT;

        Ok(VaultAuditResource {
            contract: WORKOS_CONTRACT.to_string(),
            unresolved: audit.unresolved_links,
            orphans: audit.orphan_notes,
            deadends: audit.dead_ends,
            truncated,
        })
    }

    pub(super) async fn base_query_resource_data(
        &self,
        path: &VaultRelativePath,
    ) -> AppResult<BaseQueryResource> {
        let result = self
            .query_base_data(&path.as_cli_arg(), None, Some(INDEX_LIMIT))
            .await?;
        let truncated = result.count >= INDEX_LIMIT;

        Ok(BaseQueryResource {
            contract: WORKOS_CONTRACT.to_string(),
            path: result.path,
            view: result.view,
            count: result.count,
            truncated,
            results: result.results,
        })
    }

    async fn backlink_count(&self, cli_path: &str) -> AppResult<usize> {
        let output = self
            .run_cli(
                ObsidianCommand::new("backlinks")
                    .parameter("path", cli_path)
                    .flag("total"),
            )
            .await?;
        let trimmed = output.trim();
        if trimmed.is_empty() {
            Ok(0)
        } else {
            parse_count(trimmed)
        }
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

fn note_title(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(path)
        .to_string()
}

fn properties_object(properties: &[NoteProperty]) -> rmcp::serde_json::Value {
    let mut object = rmcp::serde_json::Map::new();
    for property in properties {
        object.insert(property.name.clone(), property.value.clone());
    }
    rmcp::serde_json::Value::Object(object)
}

fn parse_backlink_line(line: &str) -> BacklinkItem {
    if let Some((path, count)) = line.rsplit_once('\t')
        && let Ok(count) = count.trim().parse::<usize>()
    {
        return BacklinkItem {
            path: path.trim().to_string(),
            count,
        };
    }

    BacklinkItem {
        path: line.trim().to_string(),
        count: 1,
    }
}

fn daily_date_from_path(path: &str) -> AppResult<DailyDate> {
    let stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();

    DailyDate::parse(stem).map_err(|_| {
        ObsidianMcpError::Parse(format!(
            "Cannot parse today's date from daily note path '{path}'; expected a YYYY-MM-DD file name"
        ))
    })
}

fn normalize_task(task: &TaskItem) -> Task {
    let text = task_text_without_checkbox(&task.text);
    let raw = if task.text.trim_start().starts_with("- [") {
        task.text.trim().to_string()
    } else {
        format!("- [{}] {}", task.status, task.text.trim())
    };

    Task {
        path: task.path.clone(),
        line: task.line,
        text: strip_date_markers(text),
        status: task.status.clone(),
        due: task_due_date(text).map(|date| date.to_string()),
        scheduled: task_scheduled_date(text).map(|date| date.to_string()),
        raw,
    }
}

fn task_text_without_checkbox(text: &str) -> &str {
    let trimmed = text.trim();
    let Some(rest) = trimmed.strip_prefix("- [") else {
        return trimmed;
    };
    let mut status = rest.chars();
    if status.next().is_none() {
        return trimmed;
    }
    match status.as_str().strip_prefix(']') {
        Some(rest) => rest.trim_start(),
        None => trimmed,
    }
}

fn strip_date_markers(text: &str) -> String {
    const MARKERS: [&str; 4] = ["📅", "due::", "⏳", "scheduled::"];
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::new();
    let mut index = 0;

    while index < tokens.len() {
        let is_marker = MARKERS.contains(&tokens[index].trim_start_matches('['));
        let next_is_date = tokens
            .get(index + 1)
            .is_some_and(|next| DailyDate::parse(next.trim_end_matches(']')).is_ok());
        if is_marker && next_is_date {
            index += 2;
        } else {
            kept.push(tokens[index]);
            index += 1;
        }
    }

    kept.join(" ")
}

fn sort_tasks(tasks: &mut [Task]) {
    tasks.sort_by(|left, right| {
        left.due
            .cmp(&right.due)
            .then_with(|| left.path.cmp(&right.path))
            .then(left.line.cmp(&right.line))
    });
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
