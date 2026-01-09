//! Execution context for executors.

use aura_core::{ActionId, AgentId};
use std::path::PathBuf;
use std::time::Duration;

/// Context provided to executors when executing an action.
#[derive(Debug, Clone)]
pub struct ExecuteContext {
    /// The agent executing the action
    pub agent_id: AgentId,
    /// The action being executed
    pub action_id: ActionId,
    /// Workspace root for this agent (sandbox root for tools)
    pub workspace_root: PathBuf,
    /// Configuration limits
    pub limits: ExecuteLimits,
}

/// Execution limits enforced by the executor.
#[derive(Debug, Clone)]
pub struct ExecuteLimits {
    /// Maximum bytes to read from files
    pub read_bytes: usize,
    /// Maximum bytes to write to files
    pub write_bytes: usize,
    /// Maximum command execution time
    pub command_timeout: Duration,
    /// Maximum stdout bytes from commands
    pub stdout_bytes: usize,
    /// Maximum stderr bytes from commands
    pub stderr_bytes: usize,
}

impl Default for ExecuteLimits {
    fn default() -> Self {
        Self {
            read_bytes: 5 * 1024 * 1024, // 5MB
            write_bytes: 1024 * 1024,    // 1MB
            command_timeout: Duration::from_secs(10),
            stdout_bytes: 256 * 1024, // 256KB
            stderr_bytes: 256 * 1024, // 256KB
        }
    }
}

impl ExecuteContext {
    /// Create a new execution context.
    #[must_use]
    pub fn new(agent_id: AgentId, action_id: ActionId, workspace_root: PathBuf) -> Self {
        Self {
            agent_id,
            action_id,
            workspace_root,
            limits: ExecuteLimits::default(),
        }
    }

    /// Set custom limits.
    #[must_use]
    pub const fn with_limits(mut self, limits: ExecuteLimits) -> Self {
        self.limits = limits;
        self
    }
}
