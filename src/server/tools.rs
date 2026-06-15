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

    async fn list_notes(
        &self,
        Parameters(ListNotesRequest { directory, limit }): Parameters<ListNotesRequest>,
    ) -> Result<Json<ListNotesResponse>, McpError> {
        let notes = self
            .list_note_paths(directory.as_deref(), limit)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ReadNoteResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let content = self
            .read_note_content_at(&normalized_path)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<CreateNoteResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let message = self
            .create_note_content(&normalized_path.as_cli_arg(), &content)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ReplaceNoteResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let message = self
            .replace_note_content(&normalized_path.as_cli_arg(), &content)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<AppendNoteResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let message = self
            .append_note_content(&normalized_path.as_cli_arg(), &content)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<SearchNotesResponse>, McpError> {
        let matches = self
            .search_note_contents(&query, directory.as_deref(), limit)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ListTagsResponse>, McpError> {
        let tags = self
            .list_tags_data(
                path.as_deref(),
                counts.unwrap_or(false),
                sort_by_count.unwrap_or(false),
                limit,
            )
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ListBacklinksResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let backlinks = self
            .list_backlinks_data(
                &normalized_path.as_cli_arg(),
                counts.unwrap_or(false),
                limit,
            )
            .await
            .map_err(tool_mcp_error)?;
        Ok(Json(ListBacklinksResponse {
            path: normalized_path.as_cli_arg(),
            count: backlinks.len(),
            backlinks,
        }))
    }

    #[tool(
        description = "Read one note's aliases, outline, direct outgoing links, and backlinks without reading neighboring note contents.",
        annotations(
            title = "Get note context",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn get_note_context(
        &self,
        Parameters(GetNoteContextRequest { path, limit }): Parameters<GetNoteContextRequest>,
    ) -> Result<Json<NoteContextResponse>, McpError> {
        self.get_note_context_data(&path, limit)
            .await
            .map(Json)
            .map_err(tool_mcp_error)
    }

    #[tool(
        description = "Audit the Markdown knowledge graph for unresolved links, orphan notes, and dead ends.",
        annotations(
            title = "Audit vault graph",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn audit_vault(
        &self,
        Parameters(AuditVaultRequest { limit }): Parameters<AuditVaultRequest>,
    ) -> Result<Json<VaultAuditResponse>, McpError> {
        self.audit_vault_data(limit)
            .await
            .map(Json)
            .map_err(tool_mcp_error)
    }

    #[tool(
        description = "List Obsidian Base files in the vault.",
        annotations(
            title = "List Obsidian Bases",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn list_bases(
        &self,
        Parameters(ListBasesRequest { limit }): Parameters<ListBasesRequest>,
    ) -> Result<Json<ListBasesResponse>, McpError> {
        let bases = self.list_bases_data(limit).await.map_err(tool_mcp_error)?;
        Ok(Json(ListBasesResponse {
            count: bases.len(),
            bases,
        }))
    }

    #[tool(
        description = "Query an Obsidian Base's default or named view and return its dynamic JSON results.",
        annotations(
            title = "Query Obsidian Base",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn query_base(
        &self,
        Parameters(QueryBaseRequest { path, view, limit }): Parameters<QueryBaseRequest>,
    ) -> Result<Json<QueryBaseResponse>, McpError> {
        self.query_base_data(&path, view.as_deref(), limit)
            .await
            .map(Json)
            .map_err(tool_mcp_error)
    }

    #[tool(
        description = "Create a new note through an explicit Obsidian Base and named view.",
        annotations(
            title = "Create Base item",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn create_base_item(
        &self,
        Parameters(CreateBaseItemRequest {
            path,
            view,
            name,
            content,
        }): Parameters<CreateBaseItemRequest>,
    ) -> Result<Json<CreateBaseItemResponse>, McpError> {
        self.create_base_item_data(&path, &view, &name, content.as_deref())
            .await
            .map(Json)
            .map_err(tool_mcp_error)
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
    async fn read_daily_note(&self) -> Result<Json<ReadDailyNoteResponse>, McpError> {
        let content = self
            .read_daily_note_content()
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<AppendDailyNoteResponse>, McpError> {
        let message = self
            .append_daily_note_content(&content, inline.unwrap_or(false))
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ReadDailyNotesResponse>, McpError> {
        let notes = self
            .read_daily_notes_data(&from, &to, limit)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ListTasksResponse>, McpError> {
        let target = target.unwrap_or_default();
        let tasks = self
            .list_tasks_data(&target, status.as_ref(), limit)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<CreateTaskResponse>, McpError> {
        let (target, task) = self
            .create_task_data(&target, &text)
            .await
            .map_err(tool_mcp_error)?;
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
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn set_task_status(
        &self,
        Parameters(SetTaskStatusRequest { path, line, status }): Parameters<SetTaskStatusRequest>,
    ) -> Result<Json<SetTaskStatusResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let status = self
            .set_task_status_data(&normalized_path.as_cli_arg(), line, &status)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ListProjectsResponse>, McpError> {
        let (directory, projects) = self
            .list_project_note_paths(directory.as_deref(), limit)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ListPropertiesResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let properties = self
            .list_properties_data(&normalized_path.as_cli_arg())
            .await
            .map_err(tool_mcp_error)?;
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
            destructive_hint = true,
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
    ) -> Result<Json<SetPropertyResponse>, McpError> {
        self.set_property_data(
            &path,
            &name,
            &value,
            property_type.as_ref(),
            preview.unwrap_or(true),
        )
        .await
        .map(Json)
        .map_err(tool_mcp_error)
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
    ) -> Result<Json<ListOverdueTasksResponse>, McpError> {
        let as_of = DailyDate::parse(&as_of)
            .map_err(tool_mcp_error)?
            .to_string();
        let target = target.unwrap_or_default();
        let tasks = self
            .list_overdue_tasks_data(&as_of, &target, limit)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ListTasksByProjectResponse>, McpError> {
        let normalized_path = VaultRelativePath::markdown(&path).map_err(tool_mcp_error)?;
        let tasks = self
            .list_tasks_by_project_data(&normalized_path.as_cli_arg(), status.as_ref(), limit)
            .await
            .map_err(tool_mcp_error)?;
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
    ) -> Result<Json<ProjectStatusResponse>, McpError> {
        self.get_project_status_data(&path, limit)
            .await
            .map(Json)
            .map_err(tool_mcp_error)
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
    ) -> Result<Json<PreviewNoteChangeResponse>, McpError> {
        self.preview_note_change_data(&path, &mode, &content)
            .await
            .map(Json)
            .map_err(tool_mcp_error)
    }

    #[tool(
        description = "Preflight one to fifty note create, replace, or append operations for best-effort optimistic concurrency and return exact proposed contents plus a deterministic approval token without modifying the vault.",
        annotations(
            title = "Preview note change set",
            read_only_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn preview_change_set(
        &self,
        Parameters(PreviewChangeSetRequest { changes }): Parameters<PreviewChangeSetRequest>,
    ) -> Result<Json<PreviewChangeSetResponse>, McpError> {
        self.preview_change_set_data(changes)
            .await
            .map(Json)
            .map_err(tool_mcp_error)
    }

    #[tool(
        description = "Apply an explicitly approved note change set sequentially and non-atomically only if a fresh full preflight produces the same preview token. This is best-effort optimistic concurrency, not compare-and-swap.",
        annotations(
            title = "Apply note change set",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn apply_change_set(
        &self,
        Parameters(ApplyChangeSetRequest {
            changes,
            preview_token,
        }): Parameters<ApplyChangeSetRequest>,
    ) -> Result<Json<ApplyChangeSetResponse>, McpError> {
        self.apply_change_set_data(changes, &preview_token)
            .await
            .map(Json)
            .map_err(tool_mcp_error)
    }
}
