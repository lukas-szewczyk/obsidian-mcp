use std::{
    fmt,
    path::{Component, Path, PathBuf},
};

use crate::{AppResult, ObsidianMcpError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VaultRelativePath(PathBuf);

impl VaultRelativePath {
    pub(crate) fn parse(raw_path: &str) -> AppResult<Self> {
        let normalized = raw_path.trim().replace('\\', "/");
        if normalized.is_empty() {
            return Err(ObsidianMcpError::InvalidPath(
                "path cannot be empty".to_string(),
            ));
        }

        let path = Path::new(&normalized);
        if path.is_absolute() {
            return Err(ObsidianMcpError::InvalidPath(
                "path must be relative to the vault".to_string(),
            ));
        }

        let mut safe_path = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(segment) => safe_path.push(segment),
                Component::CurDir => {}
                _ => {
                    return Err(ObsidianMcpError::InvalidPath(
                        "path cannot escape the vault".to_string(),
                    ));
                }
            }
        }

        if safe_path.as_os_str().is_empty() {
            return Err(ObsidianMcpError::InvalidPath(
                "path cannot be empty".to_string(),
            ));
        }

        Ok(Self(safe_path))
    }

    pub(crate) fn markdown(raw_path: &str) -> AppResult<Self> {
        Self::with_extension(
            raw_path,
            "md",
            "Only Markdown notes with the .md extension are supported",
        )
    }

    pub(crate) fn base(raw_path: &str) -> AppResult<Self> {
        Self::with_extension(
            raw_path,
            "base",
            "Only Obsidian Bases with the .base extension are supported",
        )
    }

    fn with_extension(raw_path: &str, expected: &str, error_message: &str) -> AppResult<Self> {
        let path = Self::parse(raw_path)?;
        let extension = path
            .0
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();

        if !extension.eq_ignore_ascii_case(expected) {
            return Err(ObsidianMcpError::InvalidPath(error_message.to_string()));
        }

        Ok(path)
    }

    pub(crate) fn as_cli_arg(&self) -> String {
        path_to_cli_arg(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct DailyDate {
    year: u16,
    month: u8,
    day: u8,
}

impl DailyDate {
    pub(crate) fn parse(raw_date: &str) -> AppResult<Self> {
        let date = raw_date.trim();
        if date.len() != 10 {
            return Err(ObsidianMcpError::InvalidInput(
                "date must use YYYY-MM-DD format".to_string(),
            ));
        }

        let bytes = date.as_bytes();
        if bytes[4] != b'-'
            || bytes[7] != b'-'
            || !bytes[..4].iter().all(u8::is_ascii_digit)
            || !bytes[5..7].iter().all(u8::is_ascii_digit)
            || !bytes[8..].iter().all(u8::is_ascii_digit)
        {
            return Err(ObsidianMcpError::InvalidInput(
                "date must use YYYY-MM-DD format".to_string(),
            ));
        }

        let year = date[..4]
            .parse::<u16>()
            .map_err(|_| ObsidianMcpError::InvalidInput("date year is not valid".to_string()))?;
        let month = date[5..7]
            .parse::<u8>()
            .map_err(|_| ObsidianMcpError::InvalidInput("date month is not valid".to_string()))?;
        let day = date[8..]
            .parse::<u8>()
            .map_err(|_| ObsidianMcpError::InvalidInput("date day is not valid".to_string()))?;

        if month == 0 || month > 12 {
            return Err(ObsidianMcpError::InvalidInput(
                "date month is not valid".to_string(),
            ));
        }

        let max_day = days_in_month(year, month);
        if day == 0 || day > max_day {
            return Err(ObsidianMcpError::InvalidInput(
                "date day is not valid".to_string(),
            ));
        }

        Ok(Self { year, month, day })
    }

    pub(crate) fn next(&self) -> Self {
        let max_day = days_in_month(self.year, self.month);
        if self.day < max_day {
            return Self {
                year: self.year,
                month: self.month,
                day: self.day + 1,
            };
        }

        if self.month < 12 {
            Self {
                year: self.year,
                month: self.month + 1,
                day: 1,
            }
        } else {
            Self {
                year: self.year + 1,
                month: 1,
                day: 1,
            }
        }
    }

    pub(crate) fn note_path(&self) -> AppResult<VaultRelativePath> {
        VaultRelativePath::markdown(&format!("{self}.md"))
    }
}

impl fmt::Display for DailyDate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:04}-{:02}-{:02}",
            self.year, self.month, self.day
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TaskLine(usize);

impl TaskLine {
    pub(crate) fn parse(line: usize) -> AppResult<Self> {
        if line == 0 {
            return Err(ObsidianMcpError::InvalidInput(
                "task line must be greater than zero".to_string(),
            ));
        }

        Ok(Self(line))
    }

    pub(crate) fn as_usize(self) -> usize {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PropertyName(String);

impl PropertyName {
    pub(crate) fn parse(raw_name: &str) -> AppResult<Self> {
        let name = raw_name.trim();
        if name.is_empty() {
            return Err(ObsidianMcpError::InvalidInput(
                "property name cannot be empty".to_string(),
            ));
        }
        if name.contains('\n') || name.contains('\r') {
            return Err(ObsidianMcpError::InvalidInput(
                "property name must be a single line".to_string(),
            ));
        }

        Ok(Self(name.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

fn days_in_month(year: u16, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: u16) -> bool {
    year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

fn path_to_cli_arg(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

pub(crate) fn has_markdown_extension(path: &str) -> bool {
    has_extension(path, "md")
}

pub(crate) fn has_base_extension(path: &str) -> bool {
    has_extension(path, "base")
}

fn has_extension(path: &str, expected: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_relative_path_normalizes_and_validates_paths() {
        assert_eq!(
            VaultRelativePath::markdown(r"Projects\Rust.md")
                .unwrap()
                .as_cli_arg(),
            "Projects/Rust.md"
        );
        assert_eq!(
            VaultRelativePath::parse("./Projects/../Rust.md")
                .unwrap_err()
                .to_string(),
            "path cannot escape the vault"
        );
        assert_eq!(
            VaultRelativePath::markdown("Projects/Rust.txt")
                .unwrap_err()
                .to_string(),
            "Only Markdown notes with the .md extension are supported"
        );
        assert_eq!(
            VaultRelativePath::base(r"Bases\Projects.base")
                .unwrap()
                .as_cli_arg(),
            "Bases/Projects.base"
        );
        assert_eq!(
            VaultRelativePath::base("Projects.md")
                .unwrap_err()
                .to_string(),
            "Only Obsidian Bases with the .base extension are supported"
        );
        assert_eq!(
            VaultRelativePath::parse("/tmp/Rust.md")
                .unwrap_err()
                .to_string(),
            "path must be relative to the vault"
        );
    }

    #[test]
    fn daily_dates_and_task_lines_validate_inputs() {
        let leap_day = DailyDate::parse("2024-02-29").unwrap();
        assert_eq!(leap_day.to_string(), "2024-02-29");
        assert_eq!(leap_day.next().to_string(), "2024-03-01");
        assert_eq!(
            DailyDate::parse("2026-02-29").unwrap_err().to_string(),
            "date day is not valid"
        );
        assert_eq!(
            TaskLine::parse(0).unwrap_err().to_string(),
            "task line must be greater than zero"
        );
        assert_eq!(
            PropertyName::parse("  status  ").unwrap().as_str(),
            "status"
        );
        assert_eq!(
            PropertyName::parse(" ").unwrap_err().to_string(),
            "property name cannot be empty"
        );
    }
}
