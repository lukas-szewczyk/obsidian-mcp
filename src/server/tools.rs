use rmcp::{
    handler::server::wrapper::{Json, Parameters},
    tool, tool_router,
};

use super::*;

#[tool_router(vis = "pub(crate)")]
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
        description = "Create a Markdown note by relative vault path, refusing to replace an existing note.",
        annotations(
            title = "Create note",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn create_note(
        &self,
        Parameters(CreateNoteRequest { path, content }): Parameters<CreateNoteRequest>,
    ) -> Result<Json<CreateNoteResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let message = self
            .create_note_content(&normalized_path.as_cli_arg(), &content)
            .await
            .map_err(error_message)?;
        Ok(Json(CreateNoteResponse {
            path: normalized_path.as_cli_arg(),
            message,
        }))
    }

    #[tool(
        description = "Replace the contents of an existing Markdown note by relative vault path.",
        annotations(
            title = "Replace note",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn replace_note(
        &self,
        Parameters(ReplaceNoteRequest { path, content }): Parameters<ReplaceNoteRequest>,
    ) -> Result<Json<ReplaceNoteResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let message = self
            .replace_note_content(&normalized_path.as_cli_arg(), &content)
            .await
            .map_err(error_message)?;
        Ok(Json(ReplaceNoteResponse {
            path: normalized_path.as_cli_arg(),
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

    #[tool(
        description = "Read daily notes for an inclusive YYYY-MM-DD date range, returning per-note errors for missing notes.",
        annotations(
            title = "Read daily notes",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn read_daily_notes(
        &self,
        Parameters(ReadDailyNotesRequest { from, to, limit }): Parameters<ReadDailyNotesRequest>,
    ) -> Result<Json<ReadDailyNotesResponse>, String> {
        let notes = self
            .read_daily_notes_data(&from, &to, limit)
            .await
            .map_err(error_message)?;
        Ok(Json(ReadDailyNotesResponse {
            from,
            to,
            count: notes.len(),
            notes,
        }))
    }

    #[tool(
        description = "List Markdown tasks with an optional typed target and typed status filter.",
        annotations(
            title = "List tasks",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_tasks(
        &self,
        Parameters(ListTasksRequest {
            target,
            status,
            limit,
        }): Parameters<ListTasksRequest>,
    ) -> Result<Json<ListTasksResponse>, String> {
        let target = target.unwrap_or_default();
        let tasks = self
            .list_tasks_data(&target, status.as_ref(), limit)
            .await
            .map_err(error_message)?;
        Ok(Json(ListTasksResponse {
            target,
            status,
            count: tasks.len(),
            tasks,
        }))
    }

    #[tool(
        description = "Create a new Markdown todo task in one note or today's daily note.",
        annotations(
            title = "Create task",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn create_task(
        &self,
        Parameters(CreateTaskRequest { target, text }): Parameters<CreateTaskRequest>,
    ) -> Result<Json<CreateTaskResponse>, String> {
        let (target, task) = self
            .create_task_data(&target, &text)
            .await
            .map_err(error_message)?;
        Ok(Json(CreateTaskResponse {
            message: format!("Created task in {target}"),
            target,
            task,
        }))
    }

    #[tool(
        description = "Set a Markdown task status by note path and line number.",
        annotations(
            title = "Set task status",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_task_status(
        &self,
        Parameters(SetTaskStatusRequest { path, line, status }): Parameters<SetTaskStatusRequest>,
    ) -> Result<Json<SetTaskStatusResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let status = self
            .set_task_status_data(&normalized_path.as_cli_arg(), line, &status)
            .await
            .map_err(error_message)?;
        Ok(Json(SetTaskStatusResponse {
            path: normalized_path.as_cli_arg(),
            line,
            status,
            message: "Updated task".to_string(),
        }))
    }

    #[tool(
        description = "List project notes under the configured or provided vault-relative projects directory.",
        annotations(
            title = "List projects",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_projects(
        &self,
        Parameters(ListProjectsRequest { directory, limit }): Parameters<ListProjectsRequest>,
    ) -> Result<Json<ListProjectsResponse>, String> {
        let (directory, projects) = self
            .list_project_note_paths(directory.as_deref(), limit)
            .await
            .map_err(error_message)?;
        Ok(Json(ListProjectsResponse {
            directory,
            count: projects.len(),
            projects,
        }))
    }

    #[tool(
        description = "List structured frontmatter properties for one Markdown note.",
        annotations(
            title = "List note properties",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_properties(
        &self,
        Parameters(ListPropertiesRequest { path }): Parameters<ListPropertiesRequest>,
    ) -> Result<Json<ListPropertiesResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let properties = self
            .list_properties_data(&normalized_path.as_cli_arg())
            .await
            .map_err(error_message)?;
        Ok(Json(ListPropertiesResponse {
            path: normalized_path.as_cli_arg(),
            count: properties.len(),
            properties,
        }))
    }

    #[tool(
        description = "Set a typed frontmatter property on an existing Markdown note, or preview the change without writing.",
        annotations(
            title = "Set note property",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_property(
        &self,
        Parameters(SetPropertyRequest {
            path,
            name,
            value,
            property_type,
            preview,
        }): Parameters<SetPropertyRequest>,
    ) -> Result<Json<SetPropertyResponse>, String> {
        self.set_property_data(
            &path,
            &name,
            &value,
            property_type.as_ref(),
            preview.unwrap_or(true),
        )
        .await
        .map(Json)
        .map_err(error_message)
    }

    #[tool(
        description = "List incomplete tasks with a due date before an explicit YYYY-MM-DD date.",
        annotations(
            title = "List overdue tasks",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_overdue_tasks(
        &self,
        Parameters(ListOverdueTasksRequest {
            as_of,
            target,
            limit,
        }): Parameters<ListOverdueTasksRequest>,
    ) -> Result<Json<ListOverdueTasksResponse>, String> {
        let as_of = DailyDate::parse(&as_of).map_err(error_message)?.to_string();
        let target = target.unwrap_or_default();
        let tasks = self
            .list_overdue_tasks_data(&as_of, &target, limit)
            .await
            .map_err(error_message)?;
        Ok(Json(ListOverdueTasksResponse {
            as_of,
            target,
            count: tasks.len(),
            tasks,
        }))
    }

    #[tool(
        description = "List tasks belonging to one project note.",
        annotations(
            title = "List project tasks",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_tasks_by_project(
        &self,
        Parameters(ListTasksByProjectRequest {
            path,
            status,
            limit,
        }): Parameters<ListTasksByProjectRequest>,
    ) -> Result<Json<ListTasksByProjectResponse>, String> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(error_message)?;
        let tasks = self
            .list_tasks_by_project_data(&normalized_path.as_cli_arg(), status.as_ref(), limit)
            .await
            .map_err(error_message)?;
        Ok(Json(ListTasksByProjectResponse {
            path: normalized_path.as_cli_arg(),
            status,
            count: tasks.len(),
            tasks,
        }))
    }

    #[tool(
        description = "Read one project note with its properties, open and completed tasks, and backlinks.",
        annotations(
            title = "Get project status",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_project_status(
        &self,
        Parameters(GetProjectStatusRequest { path, limit }): Parameters<GetProjectStatusRequest>,
    ) -> Result<Json<ProjectStatusResponse>, String> {
        self.get_project_status_data(&path, limit)
            .await
            .map(Json)
            .map_err(error_message)
    }

    #[tool(
        description = "Preview the exact contents produced by creating, replacing, or appending to a Markdown note without modifying the vault.",
        annotations(
            title = "Preview note change",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn preview_note_change(
        &self,
        Parameters(PreviewNoteChangeRequest {
            path,
            mode,
            content,
        }): Parameters<PreviewNoteChangeRequest>,
    ) -> Result<Json<PreviewNoteChangeResponse>, String> {
        self.preview_note_change_data(&path, &mode, &content)
            .await
            .map(Json)
            .map_err(error_message)
    }
}
