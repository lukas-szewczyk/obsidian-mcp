use std::collections::BTreeMap;

use super::*;

impl ObsidianMcp {
    pub async fn get_note_context_data(
        &self,
        path: &str,
        limit: Option<usize>,
    ) -> AppResult<NoteContextResponse> {
        let path = VaultRelativePath::markdown(path)?;
        let path_text = path.as_cli_arg();
        let limit = clamp_limit(limit, 100, 1_000);

        let aliases = self
            .run_cli(ObsidianCommand::new("aliases").parameter("path", &path_text))
            .await?;
        let outline = self
            .run_cli(
                ObsidianCommand::new("outline")
                    .parameter("path", &path_text)
                    .parameter("format", "md"),
            )
            .await?;
        let outgoing_links = self
            .run_cli(ObsidianCommand::new("links").parameter("path", &path_text))
            .await?;
        let backlinks = self
            .run_cli(
                ObsidianCommand::new("backlinks")
                    .parameter("path", &path_text)
                    .flag("counts"),
            )
            .await?;

        let aliases = parse_graph_lines(&aliases, "No aliases found.", limit);
        let outline = parse_graph_lines(&outline, "No headings found.", limit);
        let outgoing_links = parse_graph_lines(&outgoing_links, "No links found.", limit);
        let backlinks = parse_graph_lines(&backlinks, "No backlinks found.", limit);

        Ok(NoteContextResponse {
            path: path_text,
            alias_count: aliases.len(),
            outline_count: outline.len(),
            outgoing_link_count: outgoing_links.len(),
            backlink_count: backlinks.len(),
            aliases,
            outline,
            outgoing_links,
            backlinks,
        })
    }

    pub async fn audit_vault_data(&self, limit: Option<usize>) -> AppResult<VaultAuditResponse> {
        let limit = clamp_limit(limit, 100, 1_000);
        let unresolved = self
            .run_cli(
                ObsidianCommand::new("unresolved")
                    .flag("counts")
                    .flag("verbose")
                    .parameter("format", "tsv"),
            )
            .await?;
        let orphans = self.run_cli(ObsidianCommand::new("orphans")).await?;
        let dead_ends = self.run_cli(ObsidianCommand::new("deadends")).await?;

        let mut unresolved_links = parse_unresolved_links_tsv(&unresolved)?;
        unresolved_links.truncate(limit);
        let orphan_notes = parse_markdown_paths(&orphans, limit);
        let dead_ends = parse_markdown_paths(&dead_ends, limit);

        Ok(VaultAuditResponse {
            unresolved_link_count: unresolved_links.len(),
            orphan_note_count: orphan_notes.len(),
            dead_end_count: dead_ends.len(),
            unresolved_links,
            orphan_notes,
            dead_ends,
        })
    }
}

fn parse_graph_lines(output: &str, empty_message: &str, limit: usize) -> Vec<String> {
    if output.trim() == empty_message {
        return Vec::new();
    }

    let mut lines = parse_output_lines(output);
    lines.truncate(limit);
    lines
}

fn parse_markdown_paths(output: &str, limit: usize) -> Vec<String> {
    let mut paths = parse_output_lines(output)
        .into_iter()
        .filter(|path| has_markdown_extension(path))
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths.truncate(limit);
    paths
}

fn parse_unresolved_links_tsv(output: &str) -> AppResult<Vec<UnresolvedLinkItem>> {
    if output.trim() == "No unresolved links found." || output.trim().is_empty() {
        return Ok(Vec::new());
    }

    let mut links = BTreeMap::<String, (usize, Vec<String>)>::new();
    for line in output.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split('\t');
        let link = fields.next().unwrap_or_default().trim();
        let count = fields.next().unwrap_or_default().trim();
        let source = fields.next().unwrap_or_default().trim();
        if link.is_empty() || count.is_empty() || source.is_empty() || fields.next().is_some() {
            return Err(ObsidianMcpError::Parse(format!(
                "Cannot parse unresolved link row from Obsidian CLI output: {}",
                truncate_error(line)
            )));
        }
        let count = count.parse::<usize>().map_err(|_| {
            ObsidianMcpError::Parse(format!(
                "Cannot parse unresolved link count from Obsidian CLI output: {}",
                truncate_error(line)
            ))
        })?;
        let source = VaultRelativePath::markdown(source)?.as_cli_arg();
        let entry = links.entry(link.to_string()).or_default();
        entry.0 = entry.0.max(count);
        entry.1.push(source);
    }

    let mut links = links
        .into_iter()
        .map(|(link, (count, mut sources))| {
            sources.sort();
            sources.dedup();
            UnresolvedLinkItem {
                link,
                count,
                sources,
            }
        })
        .collect::<Vec<_>>();
    links.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.link.cmp(&right.link))
    });
    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_graph_empty_messages_and_markdown_paths() {
        assert!(parse_graph_lines("No links found.\n", "No links found.", 10).is_empty());
        assert_eq!(
            parse_markdown_paths("B.md\nimage.png\nA.md\nA.md\n", 10),
            vec!["A.md", "B.md"]
        );
    }

    #[test]
    fn parses_and_aggregates_unresolved_link_rows() {
        let links = parse_unresolved_links_tsv(
            "Missing note\t3\tProjects/Rust.md\nMissing note\t3\tStart.md\nOther\t1\tTodo.md\n",
        )
        .unwrap();

        assert_eq!(
            links,
            vec![
                UnresolvedLinkItem {
                    link: "Missing note".to_string(),
                    count: 3,
                    sources: vec!["Projects/Rust.md".to_string(), "Start.md".to_string()],
                },
                UnresolvedLinkItem {
                    link: "Other".to_string(),
                    count: 1,
                    sources: vec!["Todo.md".to_string()],
                },
            ]
        );
        assert!(parse_unresolved_links_tsv("broken row").is_err());
        assert!(parse_unresolved_links_tsv("Missing\tnope\tStart.md").is_err());
    }
}
