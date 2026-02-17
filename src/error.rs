use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("command timed out")]
    CommandTimeout,

    #[error("command failed: {0}")]
    CommandFailed(String),

    #[error("command I/O failed: {0}")]
    CommandIo(#[from] io::Error),

    #[error("json parse failed: {0}")]
    Json(#[from] serde_json::Error),
}

impl AppError {
    pub fn invalid_config(message: impl Into<String>) -> Self {
        Self::InvalidConfig(message.into())
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput(message.into())
    }
}
