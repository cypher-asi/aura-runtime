//! Tool error types.

use thiserror::Error;

/// Tool-specific error type.
#[derive(Error, Debug)]
pub enum ToolError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),

    #[error("tool disabled: {0}")]
    ToolDisabled(String),

    #[error("sandbox violation: path {path} escapes workspace root")]
    SandboxViolation { path: String },

    #[error("path not found: {0}")]
    PathNotFound(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid arguments: {0}")]
    InvalidArguments(String),

    #[error("command not allowed: {0}")]
    CommandNotAllowed(String),

    #[error("command timeout after {timeout_ms}ms")]
    CommandTimeout { timeout_ms: u64 },

    #[error("size limit exceeded: {actual} > {limit}")]
    SizeLimitExceeded { actual: usize, limit: usize },

    #[error("serialization error: {0}")]
    Serialization(String),
}
