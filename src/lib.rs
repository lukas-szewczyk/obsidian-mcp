mod cli;
mod domain;
mod models;
mod server;

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObsidianMcpError {
    InvalidInput(String),
    InvalidPath(String),
    CliUnavailable(String),
    CliFailed(String),
    Parse(String),
    ResourceNotFound(String),
}

impl fmt::Display for ObsidianMcpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidInput(message)
            | Self::InvalidPath(message)
            | Self::CliUnavailable(message)
            | Self::CliFailed(message)
            | Self::Parse(message)
            | Self::ResourceNotFound(message) => message,
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ObsidianMcpError {}

pub(crate) type AppResult<T> = Result<T, ObsidianMcpError>;

pub(crate) fn error_message(error: ObsidianMcpError) -> String {
    error.to_string()
}

pub use models::*;
pub use server::ObsidianMcp;
