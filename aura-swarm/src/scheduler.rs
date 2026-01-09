//! Scheduler for dispatching agent workers.

use crate::worker::process_agent;
use aura_core::AgentId;
use aura_kernel::Kernel;
use aura_reasoner::Reasoner;
use aura_store::{AgentStatus, Store};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument};

/// Per-agent lock for single-writer guarantee.
type AgentLock = Arc<Mutex<()>>;

/// Scheduler for managing agent workers.
pub struct Scheduler<S, R>
where
    S: Store + 'static,
    R: Reasoner + 'static,
{
    store: Arc<S>,
    kernel: Arc<Kernel<S, R>>,
    /// Per-agent locks to ensure single-writer
    agent_locks: DashMap<AgentId, AgentLock>,
}

impl<S, R> Scheduler<S, R>
where
    S: Store + 'static,
    R: Reasoner + 'static,
{
    /// Create a new scheduler.
    #[must_use]
    pub fn new(store: Arc<S>, kernel: Arc<Kernel<S, R>>) -> Self {
        Self {
            store,
            kernel,
            agent_locks: DashMap::new(),
        }
    }

    /// Get or create lock for an agent.
    fn get_lock(&self, agent_id: AgentId) -> AgentLock {
        self.agent_locks
            .entry(agent_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Schedule processing for an agent.
    ///
    /// This will acquire the agent lock and process all pending transactions.
    #[instrument(skip(self), fields(agent_id = %agent_id))]
    pub async fn schedule_agent(&self, agent_id: AgentId) -> anyhow::Result<u64> {
        // Check agent status
        let status = self.store.get_agent_status(agent_id)?;
        if status != AgentStatus::Active {
            debug!(?status, "Agent not active, skipping");
            return Ok(0);
        }

        // Check if there's work to do
        if !self.store.has_pending_tx(agent_id)? {
            debug!("No pending transactions");
            return Ok(0);
        }

        // Acquire lock
        let lock = self.get_lock(agent_id);
        let _guard = lock.lock().await;

        debug!("Lock acquired, processing");

        // Process all pending transactions
        match process_agent(agent_id, self.store.clone(), self.kernel.clone()).await {
            Ok(count) => {
                info!(processed = count, "Agent processing complete");
                Ok(count)
            }
            Err(e) => {
                error!(error = %e, "Agent processing failed");
                Err(e)
            }
        }
    }

    /// Check if an agent is currently being processed.
    ///
    /// Returns `true` if the agent's lock is held (processing in progress).
    /// This is part of the public API for external consumers to check agent status.
    #[must_use]
    #[allow(dead_code)] // Public API for external consumers
    pub fn is_agent_busy(&self, agent_id: AgentId) -> bool {
        self.agent_locks
            .get(&agent_id)
            .is_some_and(|lock| lock.try_lock().is_err())
    }
}
