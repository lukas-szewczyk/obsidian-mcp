use rmcp::model::{
    GetPromptRequestParams, GetPromptResult, Prompt, PromptArgument, PromptMessage,
    PromptMessageRole,
};

use super::resources::ObsidianResourceUri;
use super::*;

impl ObsidianMcp {
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
                "draft_change_set",
                Some("Draft and preview a safe multi-note change set."),
                Some(vec![required_prompt_argument(
                    "intent",
                    "What the user wants to create, replace, or append across Markdown notes.",
                )]),
            )
            .with_title("Draft note change set"),
            Prompt::new(
                "daily_review",
                Some("Review today's daily note and prepare a grounded plan."),
                None,
            )
            .with_title("Daily review"),
            Prompt::new(
                "plan_day",
                Some("Plan one day from its daily note, overdue tasks, and active projects."),
                Some(vec![required_prompt_argument(
                    "date",
                    "Date to plan in YYYY-MM-DD format.",
                )]),
            )
            .with_title("Plan day"),
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
            Prompt::new(
                "weekly_review",
                Some("Review daily notes and open tasks for a date range."),
                Some(vec![
                    required_prompt_argument("from", "Start date in YYYY-MM-DD format."),
                    required_prompt_argument("to", "End date in YYYY-MM-DD format."),
                ]),
            )
            .with_title("Weekly review"),
            Prompt::new(
                "project_review",
                Some("Review one project note together with backlinks and open tasks."),
                Some(vec![required_prompt_argument(
                    "path",
                    "Vault-relative Markdown path for the project note.",
                )]),
            )
            .with_title("Project review"),
            Prompt::new(
                "inbox_triage",
                Some("Triage open tasks and inbox-like notes into next actions."),
                Some(vec![optional_prompt_argument(
                    "directory",
                    "Optional vault-relative inbox directory to inspect.",
                )]),
            )
            .with_title("Inbox triage"),
            Prompt::new(
                "vault_audit",
                Some("Review vault graph quality and recommend prioritized link improvements."),
                None,
            )
            .with_title("Audit vault graph"),
            Prompt::new(
                "base_review",
                Some("Review the dynamic results of one Obsidian Base view."),
                Some(vec![
                    required_prompt_argument(
                        "path",
                        "Vault-relative Base path, for example Projects.base.",
                    ),
                    optional_prompt_argument("view", "Optional named Base view to query."),
                ]),
            )
            .with_title("Review Obsidian Base"),
        ]
    }

    pub fn get_prompt_result(&self, request: GetPromptRequestParams) -> AppResult<GetPromptResult> {
        match request.name.as_str() {
            "summarize_note" => {
                let path = required_prompt_string(&request, "path")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let uri = ObsidianResourceUri::Note(normalized_path);
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
                        "Use the `search_notes` tool to search for `{query}` in the Obsidian vault.{directory_instruction} Read the most relevant `workos://note/{{path}}` resources before answering. Cite note paths and keep the synthesis grounded in note contents."
                    ),
                )])
                .with_description("Search the vault and synthesize matching notes."))
            }
            "draft_note_update" => {
                let path = required_prompt_string(&request, "path")?;
                let intent = required_prompt_string(&request, "intent")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let uri = ObsidianResourceUri::Note(normalized_path.clone());
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Prepare a Markdown update for `{}` based on this intent: {intent}\n\nFirst read `{uri}` if it exists, then use `preview_note_change` to show the exact proposed result. Do not call `append_note`, `create_note`, or `replace_note` until the user approves the preview.",
                        normalized_path.as_cli_arg()
                    ),
                )])
                .with_description("Draft a safe note update."))
            }
            "draft_change_set" => {
                let intent = required_prompt_string(&request, "intent")?;
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Prepare a small, coherent set of Markdown note changes for this intent: {intent}\n\nRead the affected notes as needed, then call `preview_change_set` with all proposed create, replace, and append operations. Show the exact preview and its token. Do not call `apply_change_set` unless the user separately and explicitly accepts that exact preview token."
                    ),
                )])
                .with_description("Draft and preview a safe multi-note change set."))
            }
            "daily_review" => Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "Read `workos://workspace/today`, summarize today's note, extract commitments and open loops, then propose a short prioritized plan. Do not modify the vault.",
            )])
            .with_description("Review today's daily note.")),
            "plan_day" => {
                let date = DailyDate::parse(&required_prompt_string(&request, "date")?)?;
                let daily_uri = ObsidianResourceUri::Daily(date.clone());
                let overdue_uri = ObsidianResourceUri::TasksDueBefore(date.clone());
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Read `{daily_uri}` and `{overdue_uri}`. Use `list_tasks` with status `{{\"type\":\"todo\"}}`, then inspect relevant projects with `get_project_status`. Produce a realistic plan for `{date}` with three priorities, time-sensitive tasks, project next actions, and items to defer. Do not modify the vault."
                    ),
                )])
                .with_description("Plan one day from the work system."))
            }
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
                        "Use `list_tags` and `search_notes` to investigate `{normalized_tag}`. Read relevant `workos://note/{{path}}` resources, then summarize the theme, key notes, stale items, and suggested cleanup. Do not modify the vault."
                    ),
                )])
                .with_description("Summarize tag usage across the vault."))
            }
            "backlink_review" => {
                let path = required_prompt_string(&request, "path")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let note_uri = ObsidianResourceUri::Note(normalized_path.clone());
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Use `list_backlinks` for `{}` and read the target note `{note_uri}`. Summarize incoming context, important relationships, and follow-up notes worth reading. Do not modify the vault.",
                        normalized_path.as_cli_arg()
                    ),
                )])
                .with_description("Review backlinks for one note."))
            }
            "weekly_review" => {
                let from = DailyDate::parse(&required_prompt_string(&request, "from")?)?;
                let to = DailyDate::parse(&required_prompt_string(&request, "to")?)?;
                if from > to {
                    return Err(ObsidianMcpError::InvalidInput(
                        "from date must be before or equal to to date".to_string(),
                    ));
                }
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Use `read_daily_notes` from `{from}` to `{to}`, read `{}`, and use `list_tasks` with status `{{\"type\":\"todo\"}}`. Review commitments, overdue and unfinished tasks, recurring themes, stale items, active project risks, and a short next-week plan. Use `get_project_status` for relevant project notes. Do not modify the vault.",
                        ObsidianResourceUri::TasksDueBefore(to.clone())
                    ),
                )])
                .with_description("Review daily notes and open tasks for a date range."))
            }
            "project_review" => {
                let path = required_prompt_string(&request, "path")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let note_uri = ObsidianResourceUri::Note(normalized_path.clone());
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Use `get_project_status` for `{}`. Read project note `{note_uri}` and use `list_backlinks` only when more relationship detail is needed. Summarize current state, properties, risks, decisions, open tasks, and the next concrete actions. Do not modify the vault.",
                        normalized_path.as_cli_arg()
                    ),
                )])
                .with_description("Review one project note."))
            }
            "inbox_triage" => {
                let directory = optional_prompt_string(&request, "directory")
                    .filter(|directory| !directory.trim().is_empty());
                let directory_instruction = directory
                    .as_deref()
                    .map(|directory| {
                        format!(
                            " Also call `list_notes` with directory `{directory}` and read likely inbox notes."
                        )
                    })
                    .unwrap_or_else(|| {
                        " Also inspect `workos://tasks/open` for task inbox candidates."
                            .to_string()
                    });
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Use `list_tasks` with status `{{\"type\":\"todo\"}}` to triage open work.{directory_instruction} Group items into next actions, waiting, projects, someday, and unclear. Draft suggested note/task updates, but do not call create, replace, append, or status-changing tools until the user approves exact changes."
                    ),
                )])
                .with_description("Triage open tasks and inbox-like notes."))
            }
            "vault_audit" => Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                PromptMessageRole::User,
                "Read `workos://vault/audit`. Group unresolved links, orphan notes, and dead ends by impact. Use `get_note_context` only for the highest-impact notes when more relationship detail is needed. Recommend concrete link or organization improvements, cite note paths, and do not modify the vault.",
            )])
            .with_description("Audit the vault knowledge graph and recommend improvements.")),
            "base_review" => {
                let path = VaultRelativePath::base(&required_prompt_string(&request, "path")?)?;
                let view = optional_prompt_string(&request, "view")
                    .filter(|view| !view.trim().is_empty());
                let view_instruction = view
                    .as_deref()
                    .map(|view| format!(" with named view `{view}`"))
                    .unwrap_or_else(|| " using its default view".to_string());
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Use `query_base` for `{}`{view_instruction}. Analyze the returned dynamic records, summarize important groupings, risks, stale items, and concrete next actions. Cite note paths or file names from the results. Do not call `create_base_item` unless the user separately and explicitly requests creating an item.",
                        path.as_cli_arg()
                    ),
                )])
                .with_description("Review one Obsidian Base view."))
            }
            _ => Err(ObsidianMcpError::ResourceNotFound(format!(
                "Unknown Obsidian prompt: {}",
                request.name
            ))),
        }
    }
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
