use std::{
    env,
    ffi::{OsStr, OsString},
    future::Future,
    io::{self, Read},
    path::{Path, PathBuf},
    pin::Pin,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::{AppResult, ObsidianMcpError};

const MAX_CAPTURED_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

pub(crate) type CliFuture<'a> = Pin<Box<dyn Future<Output = AppResult<CliOutput>> + Send + 'a>>;

pub(crate) trait ObsidianCliRunner: std::fmt::Debug + Send + Sync {
    fn run<'a>(&'a self, vault: &'a Path, args: Vec<OsString>) -> CliFuture<'a>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CliOutput {
    pub(crate) success: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

impl CliOutput {
    #[cfg(test)]
    pub(crate) fn success(stdout: impl Into<String>) -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        }
    }
}

pub(crate) struct ObsidianCommand {
    command: String,
    args: Vec<OsString>,
}

impl ObsidianCommand {
    pub(crate) fn new(command: impl Into<OsString>) -> Self {
        let command = command.into();
        Self {
            command: command.to_string_lossy().into_owned(),
            args: vec![command],
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

    pub(crate) fn command_name(&self) -> &str {
        &self.command
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
    ) -> AppResult<CliOutput> {
        let command_text = format_command(&program, &args);
        let mut child = Command::new(&program)
            .current_dir(vault)
            .args(&args)
            .stdin(Stdio::null())
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
                    ObsidianMcpError::CliInfrastructure(format!(
                        "Cannot run Obsidian CLI command '{command_text}': {error}"
                    ))
                }
            })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            ObsidianMcpError::CliInfrastructure(format!(
                "Cannot capture Obsidian CLI stdout for '{command_text}'"
            ))
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ObsidianMcpError::CliInfrastructure(format!(
                "Cannot capture Obsidian CLI stderr for '{command_text}'"
            ))
        })?;
        let stdout_reader = thread::spawn(move || capture_pipe(stdout));
        let stderr_reader = thread::spawn(move || capture_pipe(stderr));

        let started_at = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = join_capture(stdout_reader, "stdout", &command_text);
                    let _ = join_capture(stderr_reader, "stderr", &command_text);
                    return Err(ObsidianMcpError::CliInfrastructure(format!(
                        "Cannot wait for Obsidian CLI command '{command_text}': {error}"
                    )));
                }
            }

            if started_at.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                join_capture(stdout_reader, "stdout", &command_text)?;
                join_capture(stderr_reader, "stderr", &command_text)?;
                return Err(ObsidianMcpError::CliTimeout(format!(
                    "Obsidian CLI command timed out after {}s and was terminated; if this was a write command, completion is indeterminate: {command_text}",
                    timeout.as_secs()
                )));
            }

            thread::sleep(Duration::from_millis(25));
        };

        let stdout = join_capture(stdout_reader, "stdout", &command_text)?;
        let stderr = join_capture(stderr_reader, "stderr", &command_text)?;
        Ok(CliOutput {
            success: status.success(),
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
        })
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
                    ObsidianMcpError::CliInfrastructure(format!(
                        "Obsidian CLI worker failed: {error}"
                    ))
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
    if value.starts_with("content=") {
        return "content=<redacted>".to_string();
    }
    if value.starts_with("value=") {
        return "value=<redacted>".to_string();
    }
    if value.contains(char::is_whitespace) {
        format!("{value:?}")
    } else {
        value.into_owned()
    }
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

pub(crate) fn redact_sensitive_text(message: &str) -> String {
    message
        .lines()
        .map(|line| {
            ["content=", "value="]
                .into_iter()
                .filter_map(|key| line.find(key).map(|index| (index, key)))
                .min_by_key(|(index, _)| *index)
                .map(|(index, key)| format!("{}{}<redacted>", &line[..index], key))
                .unwrap_or_else(|| line.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Debug)]
struct CapturedPipe {
    bytes: Vec<u8>,
    truncated: bool,
}

fn capture_pipe(mut pipe: impl Read) -> io::Result<CapturedPipe> {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;
    loop {
        let count = pipe.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = MAX_CAPTURED_OUTPUT_BYTES.saturating_sub(bytes.len());
        if remaining > 0 {
            bytes.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        if count > remaining {
            truncated = true;
        }
    }
    Ok(CapturedPipe { bytes, truncated })
}

fn join_capture(
    handle: thread::JoinHandle<io::Result<CapturedPipe>>,
    stream: &str,
    command_text: &str,
) -> AppResult<CapturedPipe> {
    handle
        .join()
        .map_err(|_| {
            ObsidianMcpError::CliInfrastructure(format!(
                "Obsidian CLI {stream} reader panicked for '{command_text}'"
            ))
        })?
        .map_err(|error| {
            ObsidianMcpError::CliInfrastructure(format!(
                "Cannot read Obsidian CLI {stream} for '{command_text}': {error}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn command_diagnostics_redact_sensitive_values() {
        let text = format_command(
            OsStr::new("obsidian"),
            &[
                OsString::from("create"),
                OsString::from("content=secret body"),
                OsString::from("value=secret property"),
                OsString::from("path=Projects/Rust.md"),
            ],
        );

        assert_eq!(
            text,
            "obsidian create content=<redacted> value=<redacted> path=Projects/Rust.md"
        );
        assert_eq!(
            redact_sensitive_text("Error: bad content=secret body"),
            "Error: bad content=<redacted>"
        );
    }

    #[test]
    fn drains_large_stdout_and_stderr_without_deadlock() {
        let script = TestScript::new(
            "#!/bin/sh\nhead -c 1048576 /dev/zero\nhead -c 1048576 /dev/zero >&2\n",
        );

        let output = RealObsidianCli::run_blocking(
            script.path.clone().into_os_string(),
            env::temp_dir(),
            Vec::new(),
            Duration::from_secs(5),
        )
        .unwrap();

        assert!(output.success);
        assert_eq!(output.stdout.len(), 1_048_576);
        assert_eq!(output.stderr.len(), 1_048_576);
        assert!(!output.stdout_truncated);
        assert!(!output.stderr_truncated);
    }

    #[test]
    fn marks_output_over_capture_limit_and_continues_draining() {
        let script = TestScript::new("#!/bin/sh\nhead -c 9000000 /dev/zero\n");

        let output = RealObsidianCli::run_blocking(
            script.path.clone().into_os_string(),
            env::temp_dir(),
            Vec::new(),
            Duration::from_secs(5),
        )
        .unwrap();

        assert!(output.success);
        assert_eq!(output.stdout.len(), MAX_CAPTURED_OUTPUT_BYTES);
        assert!(output.stdout_truncated);
    }

    #[test]
    fn timeout_terminates_and_reaps_child() {
        let script = TestScript::new("#!/bin/sh\nwhile :; do :; done\n");
        let started = Instant::now();

        let error = RealObsidianCli::run_blocking(
            script.path.clone().into_os_string(),
            env::temp_dir(),
            Vec::new(),
            Duration::from_millis(100),
        )
        .unwrap_err();

        assert!(matches!(error, ObsidianMcpError::CliTimeout(_)));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    struct TestScript {
        path: PathBuf,
    }

    impl TestScript {
        fn new(content: &str) -> Self {
            let path = env::temp_dir().join(format!(
                "obsidian_mcp_cli_test_{}_{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::write(&path, content).unwrap();
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&path, permissions).unwrap();
            Self { path }
        }
    }

    impl Drop for TestScript {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }
}
