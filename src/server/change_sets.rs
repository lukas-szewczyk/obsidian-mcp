use std::collections::HashSet;

use sha2::{Digest, Sha256};

use super::*;

const CHANGE_SET_CONTRACT_VERSION: &str = "obsidian-mcp-change-set-v1";
const MAX_CHANGE_SET_OPERATIONS: usize = 50;
const SHA256_TOKEN_PREFIX: &str = "sha256:";

#[derive(Debug)]
struct ChangeSetPreflight {
    changes: Vec<PreflightChange>,
    preview_token: String,
    precondition_error: Option<String>,
}

#[derive(Debug)]
struct PreflightChange {
    preview: PreviewChangeSetItem,
    content: String,
}

impl ObsidianMcp {
    pub async fn preview_change_set_data(
        &self,
        changes: Vec<ChangeSetOperation>,
    ) -> AppResult<PreviewChangeSetResponse> {
        let preflight = self.preflight_change_set(changes).await?;
        if let Some(error) = preflight.precondition_error {
            return Err(ObsidianMcpError::InvalidInput(error));
        }

        Ok(PreviewChangeSetResponse {
            count: preflight.changes.len(),
            changes: preflight
                .changes
                .into_iter()
                .map(|change| change.preview)
                .collect(),
            preview_token: preflight.preview_token,
        })
    }

    pub async fn apply_change_set_data(
        &self,
        changes: Vec<ChangeSetOperation>,
        expected_preview_token: &str,
    ) -> AppResult<ApplyChangeSetResponse> {
        validate_preview_token(expected_preview_token)?;
        let preflight = self.preflight_change_set(changes).await?;
        let observed_preview_token = preflight.preview_token;

        if observed_preview_token != expected_preview_token
            || preflight.precondition_error.is_some()
        {
            return Ok(ApplyChangeSetResponse {
                outcome: ChangeSetApplyOutcome::Conflict,
                expected_preview_token: expected_preview_token.to_string(),
                observed_preview_token,
                applied: Vec::new(),
                failed: None,
                skipped: preflight
                    .changes
                    .iter()
                    .map(|change| change.preview.index)
                    .collect(),
            });
        }

        let mut applied = Vec::with_capacity(preflight.changes.len());
        for change in &preflight.changes {
            let preview = &change.preview;
            let result = match preview.mode {
                NoteChangeMode::Create => {
                    self.create_note_content(&preview.path, &change.content)
                        .await
                }
                NoteChangeMode::Replace => {
                    self.replace_note_content(&preview.path, &change.content)
                        .await
                }
                NoteChangeMode::Append => {
                    self.append_note_content(&preview.path, &change.content)
                        .await
                }
            };

            match result {
                Ok(message) => applied.push(AppliedChangeSetItem {
                    index: preview.index,
                    path: preview.path.clone(),
                    mode: preview.mode.clone(),
                    message,
                }),
                Err(error) => {
                    return Ok(ApplyChangeSetResponse {
                        outcome: ChangeSetApplyOutcome::PartialFailure,
                        expected_preview_token: expected_preview_token.to_string(),
                        observed_preview_token,
                        applied,
                        failed: Some(FailedChangeSetItem {
                            index: preview.index,
                            path: preview.path.clone(),
                            mode: preview.mode.clone(),
                            error: error_message(error),
                        }),
                        skipped: preflight
                            .changes
                            .iter()
                            .skip(preview.index + 1)
                            .map(|change| change.preview.index)
                            .collect(),
                    });
                }
            }
        }

        Ok(ApplyChangeSetResponse {
            outcome: ChangeSetApplyOutcome::Applied,
            expected_preview_token: expected_preview_token.to_string(),
            observed_preview_token,
            applied,
            failed: None,
            skipped: Vec::new(),
        })
    }

    async fn preflight_change_set(
        &self,
        changes: Vec<ChangeSetOperation>,
    ) -> AppResult<ChangeSetPreflight> {
        let changes = normalize_change_set(changes)?;
        let mut previews = Vec::with_capacity(changes.len());
        let mut precondition_error = None;

        for (index, change) in changes.into_iter().enumerate() {
            let path = VaultRelativePath::markdown(&change.path)?;
            let exists = self.note_exists_at(&path).await?;
            let current_content = if exists {
                Some(self.read_note_content_at(&path).await?)
            } else {
                None
            };
            if precondition_error.is_none() {
                precondition_error = change_precondition_error(&change.mode, exists, &change.path);
            }
            let content = change.content;
            let proposed_content = match change.mode {
                NoteChangeMode::Create | NoteChangeMode::Replace => content.clone(),
                NoteChangeMode::Append => {
                    format!(
                        "{}{}",
                        current_content.as_deref().unwrap_or_default(),
                        content
                    )
                }
            };
            previews.push(PreflightChange {
                content,
                preview: PreviewChangeSetItem {
                    index,
                    path: change.path,
                    mode: change.mode,
                    exists,
                    current_content,
                    proposed_content,
                },
            });
        }

        Ok(ChangeSetPreflight {
            preview_token: change_set_token(&previews),
            changes: previews,
            precondition_error,
        })
    }
}

fn normalize_change_set(changes: Vec<ChangeSetOperation>) -> AppResult<Vec<ChangeSetOperation>> {
    if changes.is_empty() {
        return Err(ObsidianMcpError::InvalidInput(
            "change set must contain at least one operation".to_string(),
        ));
    }
    if changes.len() > MAX_CHANGE_SET_OPERATIONS {
        return Err(ObsidianMcpError::InvalidInput(format!(
            "change set cannot contain more than {MAX_CHANGE_SET_OPERATIONS} operations"
        )));
    }

    let mut paths = HashSet::with_capacity(changes.len());
    changes
        .into_iter()
        .map(|change| {
            let path = VaultRelativePath::markdown(&change.path)?.as_cli_arg();
            if !paths.insert(path.clone()) {
                return Err(ObsidianMcpError::InvalidInput(format!(
                    "change set contains duplicate path: {path}"
                )));
            }
            Ok(ChangeSetOperation {
                path,
                mode: change.mode,
                content: change.content,
            })
        })
        .collect()
}

fn change_precondition_error(mode: &NoteChangeMode, exists: bool, path: &str) -> Option<String> {
    match mode {
        NoteChangeMode::Create if exists => Some(format!(
            "Note already exists at {path}; use replace or append instead"
        )),
        NoteChangeMode::Replace if !exists => {
            Some(format!("Note does not exist at {path}; use create instead"))
        }
        _ => None,
    }
}

fn validate_preview_token(token: &str) -> AppResult<()> {
    let hex = token.strip_prefix(SHA256_TOKEN_PREFIX).ok_or_else(|| {
        ObsidianMcpError::InvalidInput(
            "preview_token must use the sha256:<64 lowercase hex characters> format".to_string(),
        )
    })?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ObsidianMcpError::InvalidInput(
            "preview_token must use the sha256:<64 lowercase hex characters> format".to_string(),
        ));
    }
    Ok(())
}

fn change_set_token(changes: &[PreflightChange]) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, CHANGE_SET_CONTRACT_VERSION.as_bytes());
    hash_u64(&mut hasher, changes.len() as u64);
    for change in changes {
        let preview = &change.preview;
        hash_u64(&mut hasher, preview.index as u64);
        hash_field(&mut hasher, preview.path.as_bytes());
        hash_field(&mut hasher, change_mode_name(&preview.mode).as_bytes());
        hash_field(&mut hasher, change.content.as_bytes());
        hash_field(&mut hasher, &[u8::from(preview.exists)]);
        match &preview.current_content {
            Some(content) => {
                hash_field(&mut hasher, &[1]);
                hash_field(&mut hasher, content.as_bytes());
            }
            None => hash_field(&mut hasher, &[0]),
        }
        hash_field(&mut hasher, preview.proposed_content.as_bytes());
    }
    let digest = hasher.finalize();
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("{SHA256_TOKEN_PREFIX}{hex}")
}

fn change_mode_name(mode: &NoteChangeMode) -> &'static str {
    match mode {
        NoteChangeMode::Create => "create",
        NoteChangeMode::Replace => "replace",
        NoteChangeMode::Append => "append",
    }
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hash_u64(hasher, value.len() as u64);
    hasher.update(value);
}

fn hash_u64(hasher: &mut Sha256, value: u64) {
    hasher.update(value.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preflight_change(
        index: usize,
        path: &str,
        mode: NoteChangeMode,
        content: &str,
        current_content: Option<&str>,
    ) -> PreflightChange {
        let proposed_content = match mode {
            NoteChangeMode::Create | NoteChangeMode::Replace => content.to_string(),
            NoteChangeMode::Append => {
                format!("{}{content}", current_content.unwrap_or_default())
            }
        };
        PreflightChange {
            preview: PreviewChangeSetItem {
                index,
                path: path.to_string(),
                mode,
                exists: current_content.is_some(),
                current_content: current_content.map(str::to_string),
                proposed_content,
            },
            content: content.to_string(),
        }
    }

    #[test]
    fn token_is_stable_and_sensitive_to_operation_state_and_order() {
        let first = preflight_change(
            0,
            "Projects/Rust.md",
            NoteChangeMode::Append,
            "\nNext",
            Some("# Rust"),
        );
        let second = preflight_change(1, "Ideas/New.md", NoteChangeMode::Create, "# New", None);
        let token = change_set_token(&[first, second]);

        assert_eq!(
            token,
            "sha256:ea4dfaa0cd8f1582e8137bc6084659eac0874beaa85a1d17b9aee4fb3f2fcbfd"
        );
        assert_eq!(
            token,
            change_set_token(&[
                preflight_change(
                    0,
                    "Projects/Rust.md",
                    NoteChangeMode::Append,
                    "\nNext",
                    Some("# Rust"),
                ),
                preflight_change(1, "Ideas/New.md", NoteChangeMode::Create, "# New", None,),
            ])
        );
        assert_ne!(
            token,
            change_set_token(&[
                preflight_change(
                    0,
                    "Projects/Rust.md",
                    NoteChangeMode::Append,
                    "\nChanged",
                    Some("# Rust"),
                ),
                preflight_change(1, "Ideas/New.md", NoteChangeMode::Create, "# New", None,),
            ])
        );
        assert_ne!(
            token,
            change_set_token(&[
                preflight_change(0, "Ideas/New.md", NoteChangeMode::Create, "# New", None,),
                preflight_change(
                    1,
                    "Projects/Rust.md",
                    NoteChangeMode::Append,
                    "\nNext",
                    Some("# Rust"),
                ),
            ])
        );
        assert_ne!(
            token,
            change_set_token(&[
                preflight_change(
                    0,
                    "Projects/Rust.md",
                    NoteChangeMode::Append,
                    "\nNext",
                    Some("# Changed"),
                ),
                preflight_change(1, "Ideas/New.md", NoteChangeMode::Create, "# New", None,),
            ])
        );
        assert_ne!(
            token,
            change_set_token(&[
                preflight_change(
                    0,
                    "Projects/Rust.md",
                    NoteChangeMode::Replace,
                    "\nNext",
                    Some("# Rust"),
                ),
                preflight_change(1, "Ideas/New.md", NoteChangeMode::Create, "# New", None,),
            ])
        );
    }

    #[test]
    fn validates_change_set_shape_paths_and_tokens() {
        assert!(normalize_change_set(Vec::new()).is_err());
        assert_eq!(
            normalize_change_set(
                (0..MAX_CHANGE_SET_OPERATIONS)
                    .map(|index| ChangeSetOperation {
                        path: format!("{index}.md"),
                        mode: NoteChangeMode::Create,
                        content: String::new(),
                    })
                    .collect()
            )
            .unwrap()
            .len(),
            MAX_CHANGE_SET_OPERATIONS
        );
        assert!(
            normalize_change_set(
                (0..=MAX_CHANGE_SET_OPERATIONS)
                    .map(|index| ChangeSetOperation {
                        path: format!("{index}.md"),
                        mode: NoteChangeMode::Create,
                        content: String::new(),
                    })
                    .collect()
            )
            .is_err()
        );
        assert!(
            normalize_change_set(vec![
                ChangeSetOperation {
                    path: "./Same.md".to_string(),
                    mode: NoteChangeMode::Create,
                    content: String::new(),
                },
                ChangeSetOperation {
                    path: "Same.md".to_string(),
                    mode: NoteChangeMode::Append,
                    content: String::new(),
                },
            ])
            .is_err()
        );
        assert!(
            normalize_change_set(vec![ChangeSetOperation {
                path: "../Outside.md".to_string(),
                mode: NoteChangeMode::Create,
                content: String::new(),
            }])
            .is_err()
        );
        assert!(validate_preview_token("not-a-token").is_err());
        assert!(validate_preview_token(&format!("sha256:{}", "a".repeat(64))).is_ok());
    }
}
