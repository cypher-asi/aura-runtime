//! Error types for the Aura system.
//!
//! Uses `thiserror` for library errors with context preservation.

use crate::ids::{ActionId, AgentId, TxId};
use thiserror::Error;

/// Result type alias using `AuraError`.
pub type Result<T> = std::result::Result<T, AuraError>;

/// Core error type for the Aura system.
#[derive(Error, Debug)]
pub enum AuraError {
    // === Storage Errors ===
    #[error("storage error: {message}")]
    Storage {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("agent not found: {agent_id}")]
    AgentNotFound { agent_id: AgentId },

    #[error("record entry not found: agent={agent_id}, seq={seq}")]
    RecordEntryNotFound { agent_id: AgentId, seq: u64 },

    #[error("transaction not found: {tx_id}")]
    TransactionNotFound { tx_id: TxId },

    #[error("inbox empty for agent: {agent_id}")]
    InboxEmpty { agent_id: AgentId },

    #[error("sequence mismatch: expected {expected}, got {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },

    // === Serialization Errors ===
    #[error("serialization error: {message}")]
    Serialization {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    #[error("deserialization error: {message}")]
    Deserialization {
        message: String,
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    // === Kernel Errors ===
    #[error("kernel error: {message}")]
    Kernel { message: String },

    #[error("policy violation: {reason}")]
    PolicyViolation { reason: String },

    #[error("action not allowed: {action_kind}")]
    ActionNotAllowed { action_kind: String },

    #[error("tool not allowed: {tool}")]
    ToolNotAllowed { tool: String },

    // === Executor Errors ===
    #[error("executor error: {message}")]
    Executor { message: String },

    #[error("tool execution failed: {tool}, reason: {reason}")]
    ToolExecutionFailed { tool: String, reason: String },

    #[error("tool timeout: {tool}, timeout_ms: {timeout_ms}")]
    ToolTimeout { tool: String, timeout_ms: u64 },

    #[error("sandbox violation: {path}")]
    SandboxViolation { path: String },

    // === Reasoner Errors ===
    #[error("reasoner error: {message}")]
    Reasoner { message: String },

    #[error("reasoner timeout after {timeout_ms}ms")]
    ReasonerTimeout { timeout_ms: u64 },

    #[error("reasoner unavailable: {reason}")]
    ReasonerUnavailable { reason: String },

    // === Validation Errors ===
    #[error("validation error: {message}")]
    Validation { message: String },

    #[error("invalid transaction: {reason}")]
    InvalidTransaction { reason: String },

    #[error("invalid action: {action_id}, reason: {reason}")]
    InvalidAction { action_id: ActionId, reason: String },

    // === Configuration Errors ===
    #[error("configuration error: {message}")]
    Configuration { message: String },

    // === Internal Errors ===
    #[error("internal error: {message}")]
    Internal { message: String },
}

impl AuraError {
    /// Create a storage error with a message.
    pub fn storage(message: impl Into<String>) -> Self {
        Self::Storage {
            message: message.into(),
            source: None,
        }
    }

    /// Create a storage error with a source.
    pub fn storage_with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Storage {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a serialization error.
    pub fn serialization(message: impl Into<String>) -> Self {
        Self::Serialization {
            message: message.into(),
            source: None,
        }
    }

    /// Create a serialization error with a source.
    pub fn serialization_with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Serialization {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a deserialization error.
    pub fn deserialization(message: impl Into<String>) -> Self {
        Self::Deserialization {
            message: message.into(),
            source: None,
        }
    }

    /// Create a deserialization error with a source.
    pub fn deserialization_with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::Deserialization {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a kernel error.
    pub fn kernel(message: impl Into<String>) -> Self {
        Self::Kernel {
            message: message.into(),
        }
    }

    /// Create a policy violation error.
    pub fn policy_violation(reason: impl Into<String>) -> Self {
        Self::PolicyViolation {
            reason: reason.into(),
        }
    }

    /// Create an executor error.
    pub fn executor(message: impl Into<String>) -> Self {
        Self::Executor {
            message: message.into(),
        }
    }

    /// Create a reasoner error.
    pub fn reasoner(message: impl Into<String>) -> Self {
        Self::Reasoner {
            message: message.into(),
        }
    }

    /// Create a validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::Validation {
            message: message.into(),
        }
    }

    /// Create a configuration error.
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::Configuration {
            message: message.into(),
        }
    }

    /// Create an internal error.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

// Conversion from serde_json errors
impl From<serde_json::Error> for AuraError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization {
            message: err.to_string(),
            source: Some(Box::new(err)),
        }
    }
}
