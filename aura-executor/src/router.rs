//! Executor router for dispatching actions.

use crate::{ExecuteContext, Executor};
use aura_core::{Action, Effect, EffectKind, EffectStatus};
use std::sync::Arc;
use tracing::{debug, error, instrument};

/// Router that dispatches actions to the appropriate executor.
pub struct ExecutorRouter {
    executors: Vec<Arc<dyn Executor>>,
}

impl ExecutorRouter {
    /// Create a new empty router.
    #[must_use]
    pub fn new() -> Self {
        Self {
            executors: Vec::new(),
        }
    }

    /// Add an executor to the router.
    pub fn add_executor(&mut self, executor: Arc<dyn Executor>) {
        self.executors.push(executor);
    }

    /// Create a router with the given executors.
    #[must_use]
    pub fn with_executors(executors: Vec<Arc<dyn Executor>>) -> Self {
        Self { executors }
    }

    /// Execute an action by finding and invoking the appropriate executor.
    #[instrument(skip(self, ctx, action), fields(action_id = %action.action_id, kind = ?action.kind))]
    pub async fn execute(&self, ctx: &ExecuteContext, action: &Action) -> Effect {
        // Find an executor that can handle this action
        for executor in &self.executors {
            if executor.can_handle(action) {
                debug!(executor = executor.name(), "Dispatching action to executor");

                match executor.execute(ctx, action).await {
                    Ok(effect) => {
                        debug!(?effect.status, "Action executed successfully");
                        return effect;
                    }
                    Err(e) => {
                        error!(error = %e, "Executor failed");
                        return Effect::new(
                            action.action_id,
                            EffectKind::Agreement,
                            EffectStatus::Failed,
                            format!("Executor error: {e}"),
                        );
                    }
                }
            }
        }

        // No executor found
        debug!("No executor found for action");
        Effect::new(
            action.action_id,
            EffectKind::Agreement,
            EffectStatus::Failed,
            "No executor available for action",
        )
    }
}

impl Default for ExecutorRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::{ActionId, ActionKind, AgentId};
    use bytes::Bytes;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_no_executor() {
        let router = ExecutorRouter::new();
        let ctx = ExecuteContext::new(
            AgentId::generate(),
            ActionId::generate(),
            PathBuf::from("/tmp"),
        );
        let action = Action::new(ActionId::generate(), ActionKind::Delegate, Bytes::new());

        let effect = router.execute(&ctx, &action).await;
        assert_eq!(effect.status, EffectStatus::Failed);
    }

    #[tokio::test]
    async fn test_noop_executor() {
        use crate::NoOpExecutor;

        let mut router = ExecutorRouter::new();
        router.add_executor(Arc::new(NoOpExecutor));

        let ctx = ExecuteContext::new(
            AgentId::generate(),
            ActionId::generate(),
            PathBuf::from("/tmp"),
        );
        let action = Action::new(ActionId::generate(), ActionKind::Delegate, Bytes::new());

        let effect = router.execute(&ctx, &action).await;
        assert_eq!(effect.status, EffectStatus::Committed);
    }
}
