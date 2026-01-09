//! Request types for the reasoner.

use aura_core::{ActionKind, AgentId, Transaction};
use serde::{Deserialize, Serialize};

/// A summary of a record entry for context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSummary {
    /// Sequence number
    pub seq: u64,
    /// Transaction kind
    pub tx_kind: String,
    /// Action kinds that were taken
    pub action_kinds: Vec<ActionKind>,
    /// Truncated payload for context
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_summary: Option<String>,
}

/// Request to the reasoner for proposals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeRequest {
    /// Agent making the request
    pub agent_id: AgentId,
    /// Current transaction to process
    pub tx: Transaction,
    /// Recent record entries for context
    pub record_window: Vec<RecordSummary>,
    /// Limits for the response
    pub limits: ProposeLimits,
}

/// Limits for proposal generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposeLimits {
    /// Maximum number of proposals to return
    pub max_proposals: u32,
}

impl Default for ProposeLimits {
    fn default() -> Self {
        Self { max_proposals: 8 }
    }
}

impl ProposeRequest {
    /// Create a new propose request.
    #[must_use]
    pub fn new(agent_id: AgentId, tx: Transaction) -> Self {
        Self {
            agent_id,
            tx,
            record_window: Vec::new(),
            limits: ProposeLimits::default(),
        }
    }

    /// Add record window context.
    #[must_use]
    pub fn with_record_window(mut self, window: Vec<RecordSummary>) -> Self {
        self.record_window = window;
        self
    }

    /// Set limits.
    #[must_use]
    pub const fn with_limits(mut self, limits: ProposeLimits) -> Self {
        self.limits = limits;
        self
    }
}
