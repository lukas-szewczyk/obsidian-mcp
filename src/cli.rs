use std::{
    env,
    ffi::{OsStr, OsString},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{AppResult, ObsidianMcpError};

pub(crate) type CliFuture<'a> = Pin<Box<dyn Future<Output = AppResult<String>> + Send + 'a>>;

pub(crate) trait ObsidianCliRunner: std::fmt::Debug + Send + Sync {
    fn run<'a>(&'a self, vault: &'a Path, args: Vec<OsString>) -> CliFuture<'a>;
}

pub(crate) struct ObsidianCommand {
    args: Vec<OsString>,
}

impl ObsidianCommand {
    pub(crate) fn new(command: impl Into<OsString>) -> Self {
        Self {
            args: vec![command.into()],
        }
    }

    pub(crate) fn parameter(mut self, key: &str, value: impl AsRef<str>) -> Self {
        self.args.push(format!("{key}={}", value.as_ref()).into());
        self
    }

    pub(crate) fn flag(mut self, flag: impl Into<OsString>) -> Self {
        self.args.push(flag.into());
        self
    }

    pub(crate) fn into_args(self, vault_name: Option<&str>) -> Vec<OsString> {
        match vault_name {
            Some(vault_name) => std::iter::once(OsString::from(format!("vault={vault_name}")))
                .chain(self.args)
                .collect(),
            None => self.args,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RealObsidianCli {
    program: OsString,
    timeout: Duration,
}

impl RealObsidianCli {
    pub(crate) fn from_env() -> Self {
        let program = env::var_os("OBSIDIAN_CLI").unwrap_or_else(|| OsString::from("obsidian"));
        Self {
            program,
            timeout: Duration::from_secs(15),
        }
    }

    fn run_blocking(
        program: OsString,
        vault: PathBuf,
        args: Vec<OsString>,
        timeout: Duration,
    ) -> AppResult<String> {
        let command_text = format_command(&program, &args);
        let mut child = Command::new(&program)
            .current_dir(vault)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    ObsidianMcpError::CliUnavailable(format!(
                        "Cannot run Obsidian CLI '{}': command not found. Install or enable the Obsidian CLI, or set OBSIDIAN_CLI to the CLI path.",
                        program.to_string_lossy()
                    ))
                } else {
                    ObsidianMcpError::CliFailed(format!(
                        "Cannot run Obsidian CLI command '{command_text}': {error}"
                    ))
                }
            })?;

        let started_at = Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|error| {
                    ObsidianMcpError::CliFailed(format!(
                        "Cannot wait for Obsidian CLI command '{command_text}': {error}"
                    ))
                })?
                .is_some()
            {
                let output = child.wait_with_output().map_err(|error| {
                    ObsidianMcpError::CliFailed(format!(
                        "Cannot collect Obsidian CLI output for '{command_text}': {error}"
                    ))
                })?;

                let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
                if output.status.success() {
                    return Ok(stdout);
                }

                let stderr = String::from_utf8_lossy(&output.stderr);
                let details = first_non_empty([stderr.as_ref(), stdout.as_str()])
                    .map(truncate_error)
                    .unwrap_or_else(|| format!("exit status {}", output.status));
                return Err(ObsidianMcpError::CliFailed(format!(
                    "Obsidian CLI command failed: {command_text}\n{details}"
                )));
            }

            if started_at.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ObsidianMcpError::CliFailed(format!(
                    "Obsidian CLI command timed out after {}s: {command_text}",
                    timeout.as_secs()
                )));
            }

            thread::sleep(Duration::from_millis(25));
        }
    }
}

impl ObsidianCliRunner for RealObsidianCli {
    fn run<'a>(&'a self, vault: &'a Path, args: Vec<OsString>) -> CliFuture<'a> {
        let program = self.program.clone();
        let timeout = self.timeout;
        let vault = vault.to_path_buf();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || Self::run_blocking(program, vault, args, timeout))
                .await
                .map_err(|error| {
                    ObsidianMcpError::CliFailed(format!("Obsidian CLI worker failed: {error}"))
                })?
        })
    }
}

pub(crate) fn encode_cli_text(content: &str) -> String {
    content
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn format_command(program: &OsStr, args: &[OsString]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(OsString::as_os_str))
        .map(display_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_arg(arg: &OsStr) -> String {
    let value = arg.to_string_lossy();
    if value.contains(char::is_whitespace) {
        format!("{value:?}")
    } else {
        value.into_owned()
    }
}

fn first_non_empty<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

pub(crate) fn truncate_error(message: &str) -> String {
    const MAX_CHARS: usize = 1_000;
    let mut chars = message.trim().chars();
    let truncated: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}
