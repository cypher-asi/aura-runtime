//! Context building for the kernel.

use aura_core::{hash, RecordEntry, Transaction};
use aura_reasoner::RecordSummary;
use tracing::debug;

/// Context for kernel processing.
#[derive(Debug, Clone)]
pub struct Context {
    /// Hash of the context inputs
    pub context_hash: [u8; 32],
    /// Record window summaries for the reasoner
    pub record_summaries: Vec<RecordSummary>,
}

/// Builder for kernel context.
pub struct ContextBuilder {
    tx_bytes: Vec<u8>,
    record_window: Vec<RecordEntry>,
}

impl ContextBuilder {
    /// Create a new context builder.
    #[must_use]
    pub fn new(tx: &Transaction) -> Self {
        let tx_bytes = serde_json::to_vec(tx).unwrap_or_default();
        Self {
            tx_bytes,
            record_window: Vec::new(),
        }
    }

    /// Add record window entries.
    #[must_use]
    pub fn with_record_window(mut self, entries: Vec<RecordEntry>) -> Self {
        self.record_window = entries;
        self
    }

    /// Build the context.
    #[must_use]
    pub fn build(self) -> Context {
        // Compute context hash
        let mut hasher = hash::Hasher::new();
        hasher.update(&self.tx_bytes);

        // Include minimal deterministic data from record window
        for entry in &self.record_window {
            hasher.update(&entry.seq.to_be_bytes());
            hasher.update(&entry.context_hash);
        }

        let context_hash = hasher.finalize();

        // Build record summaries for reasoner
        let record_summaries: Vec<RecordSummary> = self
            .record_window
            .iter()
            .map(|entry| {
                let action_kinds: Vec<_> = entry.actions.iter().map(|a| a.kind).collect();

                // Truncate payload for summary
                let payload_summary = if entry.tx.payload.len() > 200 {
                    Some(format!(
                        "{}...",
                        String::from_utf8_lossy(&entry.tx.payload[..200])
                    ))
                } else {
                    Some(String::from_utf8_lossy(&entry.tx.payload).to_string())
                };

                RecordSummary {
                    seq: entry.seq,
                    tx_kind: format!("{:?}", entry.tx.kind),
                    action_kinds,
                    payload_summary,
                }
            })
            .collect();

        debug!(
            hash = hex::encode(&context_hash[..8]),
            window_size = record_summaries.len(),
            "Context built"
        );

        Context {
            context_hash,
            record_summaries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::{AgentId, Decision, ProposalSet};

    #[test]
    fn test_context_hash_deterministic() {
        let tx = Transaction::user_prompt(AgentId::generate(), "test");

        let ctx1 = ContextBuilder::new(&tx).build();
        let ctx2 = ContextBuilder::new(&tx).build();

        assert_eq!(ctx1.context_hash, ctx2.context_hash);
    }

    #[test]
    fn test_context_hash_differs_with_window() {
        let agent_id = AgentId::generate();
        let tx = Transaction::user_prompt(agent_id, "test");

        let entry = RecordEntry::builder(1, tx.clone())
            .context_hash([1u8; 32])
            .proposals(ProposalSet::new())
            .decision(Decision::new())
            .build();

        let ctx1 = ContextBuilder::new(&tx).build();
        let ctx2 = ContextBuilder::new(&tx)
            .with_record_window(vec![entry])
            .build();

        assert_ne!(ctx1.context_hash, ctx2.context_hash);
    }
}
