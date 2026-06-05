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
                        "Prepare a Markdown update for `{}` based on this intent: {intent}\n\nFirst read `{uri}` if it exists. Draft the exact text to append, create, or replace. Do not call `append_note`, `create_note`, or `replace_note` until the user approves the final text.",
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
                        "Use `read_daily_notes` from `{from}` to `{to}` and `list_tasks` with status `{{\"type\":\"todo\"}}`. Review commitments, unfinished tasks, recurring themes, stale items, and a short next-week plan. Read relevant `obsidian://note/{{path}}` resources when task context is unclear. Do not modify the vault."
                    ),
                )])
                .with_description("Review daily notes and open tasks for a date range."))
            }
            "project_review" => {
                let path = required_prompt_string(&request, "path")?;
                let normalized_path = VaultRelativePath::markdown(&path)?;
                let note_uri = ObsidianResourceUri::note(&normalized_path);
                let backlinks_uri = ObsidianResourceUri::backlinks(&normalized_path);
                Ok(GetPromptResult::new(vec![PromptMessage::new_text(
                    PromptMessageRole::User,
                    format!(
                        "Read project note `{note_uri}`, backlinks `{backlinks_uri}`, and use `list_tasks` with target `{{\"type\":\"note\",\"path\":\"{}\"}}`. Summarize current state, risks, decisions, open tasks, and the next concrete actions. Do not modify the vault.",
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
                        " Also inspect `obsidian://tasks/open` for task inbox candidates."
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
