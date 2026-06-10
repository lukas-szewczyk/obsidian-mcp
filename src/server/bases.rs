use rmcp::serde_json::{self, Value};

use super::*;

impl ObsidianMcp {
    pub async fn list_bases_data(&self, limit: Option<usize>) -> AppResult<Vec<String>> {
        let output = self.run_cli(ObsidianCommand::new("bases")).await?;
        if output.trim() == "No base files found in vault" || output.trim().is_empty() {
            return Ok(Vec::new());
        }

        let mut bases = parse_output_lines(&output)
            .into_iter()
            .filter(|path| has_base_extension(path))
            .collect::<Vec<_>>();
        bases.sort();
        bases.dedup();
        bases.truncate(clamp_limit(limit, 100, 1_000));
        Ok(bases)
    }

    pub async fn query_base_data(
        &self,
        path: &str,
        view: Option<&str>,
        limit: Option<usize>,
    ) -> AppResult<QueryBaseResponse> {
        let path = VaultRelativePath::base(path)?;
        let view = view.map(validate_base_text).transpose()?;
        let mut command = ObsidianCommand::new("base:query")
            .parameter("path", path.as_cli_arg())
            .parameter("format", "json");
        if let Some(view) = &view {
            command = command.parameter("view", view);
        }

        let output = self.run_cli(command).await?;
        let mut results = parse_base_query_json(&output)?;
        results.truncate(clamp_limit(limit, 100, 1_000));

        Ok(QueryBaseResponse {
            path: path.as_cli_arg(),
            view,
            count: results.len(),
            results,
        })
    }

    pub async fn create_base_item_data(
        &self,
        path: &str,
        view: &str,
        name: &str,
        content: Option<&str>,
    ) -> AppResult<CreateBaseItemResponse> {
        let path = VaultRelativePath::base(path)?;
        let view = validate_base_text(view)?;
        let name = validate_base_item_name(name)?;
        self.query_base_data(&path.as_cli_arg(), Some(&view), Some(1))
            .await?;
        let mut command = ObsidianCommand::new("base:create")
            .parameter("path", path.as_cli_arg())
            .parameter("view", &view)
            .parameter("name", &name);
        if let Some(content) = content {
            command = command.parameter("content", encode_cli_text(content));
        }

        let message = self.run_cli(command).await?.trim().to_string();
        Ok(CreateBaseItemResponse {
            path: path.as_cli_arg(),
            view,
            name,
            message,
        })
    }
}

fn validate_base_text(value: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ObsidianMcpError::InvalidInput(
            "base view cannot be empty".to_string(),
        ));
    }
    if value.contains('\n') || value.contains('\r') {
        return Err(ObsidianMcpError::InvalidInput(
            "base view must be a single line".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn validate_base_item_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ObsidianMcpError::InvalidInput(
            "base item name cannot be empty".to_string(),
        ));
    }
    if name.contains('\n') || name.contains('\r') {
        return Err(ObsidianMcpError::InvalidInput(
            "base item name must be a single line".to_string(),
        ));
    }
    if name.contains('/') || name.contains('\\') {
        return Err(ObsidianMcpError::InvalidInput(
            "base item name cannot contain path separators".to_string(),
        ));
    }
    Ok(name.to_string())
}

fn parse_base_query_json(output: &str) -> AppResult<Vec<Value>> {
    let value = serde_json::from_str::<Value>(output.trim()).map_err(|error| {
        ObsidianMcpError::Parse(format!(
            "Cannot parse Base query from Obsidian CLI output: {error}"
        ))
    })?;
    let Value::Array(results) = value else {
        return Err(ObsidianMcpError::Parse(
            "Cannot parse Base query from Obsidian CLI output: expected a JSON array".to_string(),
        ));
    };
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_base_inputs_and_parses_dynamic_results() {
        assert_eq!(validate_base_text(" Active ").unwrap(), "Active");
        assert!(validate_base_text("\n").is_err());
        assert_eq!(
            validate_base_item_name("Project Alpha").unwrap(),
            "Project Alpha"
        );
        assert!(validate_base_item_name("Projects/Alpha").is_err());

        let results =
            parse_base_query_json(r#"[{"file.name":"Rust","status":"active"},["dynamic"]]"#)
                .unwrap();
        assert_eq!(results.len(), 2);
        assert!(parse_base_query_json("{}").is_err());
        assert!(parse_base_query_json("not json").is_err());
    }
}
