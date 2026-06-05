use rmcp::serde_json::{self, Value};

use super::*;

impl ObsidianMcp {
    pub async fn list_properties_data(&self, path: &str) -> AppResult<Vec<NoteProperty>> {
        let path = VaultRelativePath::markdown(path)?;
        let output = self
            .run_cli(
                ObsidianCommand::new("properties")
                    .parameter("path", path.as_cli_arg())
                    .parameter("format", "json"),
            )
            .await?;

        parse_properties_json(&output)
    }

    pub async fn set_property_data(
        &self,
        path: &str,
        name: &str,
        value: &str,
        property_type: Option<&PropertyType>,
        preview: bool,
    ) -> AppResult<SetPropertyResponse> {
        let path = VaultRelativePath::markdown(path)?;
        let name = PropertyName::parse(name)?;
        if !self.note_exists_at(&path).await {
            return Err(ObsidianMcpError::InvalidInput(
                "Note does not exist; create it before setting properties".to_string(),
            ));
        }

        let properties = self.list_properties_data(&path.as_cli_arg()).await?;
        let previous_value = properties
            .iter()
            .find(|property| property.name == name.as_str())
            .map(|property| property.value.clone());

        if !preview {
            let mut command = ObsidianCommand::new("property:set")
                .parameter("path", path.as_cli_arg())
                .parameter("name", name.as_str())
                .parameter("value", value);
            if let Some(property_type) = property_type {
                command = command.parameter("type", property_type_cli_arg(property_type));
            }
            self.run_cli(command).await?;
        }

        Ok(SetPropertyResponse {
            path: path.as_cli_arg(),
            name: name.as_str().to_string(),
            value: value.to_string(),
            property_type: property_type.cloned(),
            previous_value,
            applied: !preview,
            message: if preview {
                "Previewed property change".to_string()
            } else {
                "Set property".to_string()
            },
        })
    }

    pub async fn list_overdue_tasks_data(
        &self,
        as_of: &str,
        target: &TaskReadTarget,
        limit: Option<usize>,
    ) -> AppResult<Vec<OverdueTaskItem>> {
        let as_of = DailyDate::parse(as_of)?;
        let mut overdue = self
            .list_tasks_data(target, Some(&TaskStatus::Todo), Some(1_000))
            .await?
            .into_iter()
            .filter_map(|task| {
                let due_date = task_due_date(&task.text)?;
                (due_date < as_of).then(|| OverdueTaskItem {
                    due_date: due_date.to_string(),
                    status: task.status,
                    text: task.text,
                    path: task.path,
                    line: task.line,
                })
            })
            .collect::<Vec<_>>();

        overdue.sort_by(|left, right| {
            left.due_date
                .cmp(&right.due_date)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.line.cmp(&right.line))
        });
        overdue.truncate(clamp_limit(limit, 100, 1_000));
        Ok(overdue)
    }

    pub async fn list_tasks_by_project_data(
        &self,
        path: &str,
        status: Option<&TaskStatus>,
        limit: Option<usize>,
    ) -> AppResult<Vec<TaskItem>> {
        let path = VaultRelativePath::markdown(path)?;
        self.list_tasks_data(
            &TaskReadTarget::Note {
                path: path.as_cli_arg(),
            },
            status,
            limit,
        )
        .await
    }

    pub async fn get_project_status_data(
        &self,
        path: &str,
        limit: Option<usize>,
    ) -> AppResult<ProjectStatusResponse> {
        let path = VaultRelativePath::markdown(path)?;
        let path_text = path.as_cli_arg();
        let limit = Some(clamp_limit(limit, 100, 500));
        let content = self.read_note_content_at(&path).await?;
        let properties = self.list_properties_data(&path_text).await?;
        let open_tasks = self
            .list_tasks_by_project_data(&path_text, Some(&TaskStatus::Todo), limit)
            .await?;
        let completed_tasks = self
            .list_tasks_by_project_data(&path_text, Some(&TaskStatus::Done), limit)
            .await?;
        let backlinks = self.list_backlinks_data(&path_text, true, limit).await?;

        Ok(ProjectStatusResponse {
            path: path_text,
            content,
            properties,
            open_task_count: open_tasks.len(),
            completed_task_count: completed_tasks.len(),
            backlink_count: backlinks.len(),
            open_tasks,
            completed_tasks,
            backlinks,
        })
    }

    pub async fn preview_note_change_data(
        &self,
        path: &str,
        mode: &NoteChangeMode,
        content: &str,
    ) -> AppResult<PreviewNoteChangeResponse> {
        let path = VaultRelativePath::markdown(path)?;
        let exists = self.note_exists_at(&path).await;
        match mode {
            NoteChangeMode::Create if exists => {
                return Err(ObsidianMcpError::InvalidInput(
                    "Note already exists; preview replace or append instead".to_string(),
                ));
            }
            NoteChangeMode::Replace if !exists => {
                return Err(ObsidianMcpError::InvalidInput(
                    "Note does not exist; preview create instead".to_string(),
                ));
            }
            _ => {}
        }

        let current_content = if exists {
            Some(self.read_note_content_at(&path).await?)
        } else {
            None
        };
        let proposed_content = match mode {
            NoteChangeMode::Create | NoteChangeMode::Replace => content.to_string(),
            NoteChangeMode::Append => {
                format!(
                    "{}{content}",
                    current_content.as_deref().unwrap_or_default()
                )
            }
        };

        Ok(PreviewNoteChangeResponse {
            path: path.as_cli_arg(),
            mode: mode.clone(),
            exists,
            current_content,
            proposed_content,
        })
    }
}

fn parse_properties_json(output: &str) -> AppResult<Vec<NoteProperty>> {
    let output = output.trim();
    if output.is_empty() || output == "No frontmatter found." {
        return Ok(Vec::new());
    }

    let value = serde_json::from_str::<Value>(output).map_err(|error| {
        ObsidianMcpError::Parse(format!(
            "Cannot parse properties from Obsidian CLI output: {error}"
        ))
    })?;
    let mut properties = match value {
        Value::Object(properties) => properties
            .into_iter()
            .map(|(name, value)| NoteProperty { name, value })
            .collect::<Vec<_>>(),
        Value::Array(properties) => properties
            .into_iter()
            .map(parse_property_entry)
            .collect::<AppResult<Vec<_>>>()?,
        _ => {
            return Err(ObsidianMcpError::Parse(
                "Cannot parse properties from Obsidian CLI output: expected a JSON object or array"
                    .to_string(),
            ));
        }
    };
    properties.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(properties)
}

fn parse_property_entry(value: Value) -> AppResult<NoteProperty> {
    let Value::Object(mut property) = value else {
        return Err(ObsidianMcpError::Parse(
            "Cannot parse property from Obsidian CLI output: expected a JSON object".to_string(),
        ));
    };
    let name = property
        .remove("name")
        .and_then(|name| name.as_str().map(str::to_string))
        .ok_or_else(|| {
            ObsidianMcpError::Parse(
                "Cannot parse property from Obsidian CLI output: missing string name".to_string(),
            )
        })?;
    let value = property.remove("value").ok_or_else(|| {
        ObsidianMcpError::Parse(
            "Cannot parse property from Obsidian CLI output: missing value".to_string(),
        )
    })?;
    Ok(NoteProperty { name, value })
}

fn property_type_cli_arg(property_type: &PropertyType) -> &'static str {
    match property_type {
        PropertyType::Text => "text",
        PropertyType::List => "list",
        PropertyType::Number => "number",
        PropertyType::Checkbox => "checkbox",
        PropertyType::Date => "date",
        PropertyType::Datetime => "datetime",
    }
}

fn task_due_date(text: &str) -> Option<DailyDate> {
    ["📅", "due::"].into_iter().find_map(|marker| {
        let tail = text.split_once(marker)?.1.trim_start();
        let date = tail.get(..10)?;
        DailyDate::parse(date).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_properties_and_due_dates() {
        let properties =
            parse_properties_json(r#"{"status":"active","priority":2,"tags":["project"]}"#)
                .unwrap();
        assert_eq!(
            properties
                .iter()
                .map(|property| property.name.as_str())
                .collect::<Vec<_>>(),
            vec!["priority", "status", "tags"]
        );
        assert!(
            parse_properties_json("No frontmatter found.")
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            parse_properties_json(r#"[{"name":"status","value":"active"}]"#).unwrap()[0].name,
            "status"
        );
        assert_eq!(
            task_due_date("- [ ] Ship release 📅 2026-06-04")
                .unwrap()
                .to_string(),
            "2026-06-04"
        );
        assert_eq!(
            task_due_date("- [ ] Ship release due:: 2026-06-03")
                .unwrap()
                .to_string(),
            "2026-06-03"
        );
        assert!(task_due_date("- [ ] No due date").is_none());
    }
}
