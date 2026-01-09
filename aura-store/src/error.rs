//! Storage error types.

use aura_core::{AgentId, TxId};
use thiserror::Error;

/// Storage-specific error type.
#[derive(Error, Debug)]
pub enum StoreError {
    #[error("RocksDB error: {0}")]
    RocksDb(#[from] rocksdb::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("deserialization error: {0}")]
    Deserialization(String),

    #[error("agent not found: {0}")]
    AgentNotFound(AgentId),

    #[error("record entry not found: agent={0}, seq={1}")]
    RecordEntryNotFound(AgentId, u64),

    #[error("transaction not found: {0}")]
    TransactionNotFound(TxId),

    #[error("inbox empty for agent: {0}")]
    InboxEmpty(AgentId),

    #[error("sequence mismatch: expected {expected}, got {actual}")]
    SequenceMismatch { expected: u64, actual: u64 },

    #[error("column family not found: {0}")]
    ColumnFamilyNotFound(String),

    #[error("invalid key format: {0}")]
    InvalidKey(String),
}

impl From<serde_json::Error> for StoreError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}
