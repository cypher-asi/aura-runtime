//! Core types for the agent orchestration layer.

use async_trait::async_trait;

/// Information about a tool call to be executed.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    /// Tool use ID from the model.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// Tool arguments as JSON.
    pub input: serde_json::Value,
}

/// Result of executing a single tool call.
#[derive(Debug, Clone)]
pub struct ToolCallResult {
    /// Tool use ID.
    pub tool_use_id: String,
    /// Result content (text or error message).
    pub content: String,
    /// Whether the tool execution failed.
    pub is_error: bool,
    /// When true, the loop terminates after processing all results in this batch.
    /// Used by engine tools like `task_done` to signal task completion.
    pub stop_loop: bool,
}

impl ToolCallResult {
    /// Create a successful result.
    #[must_use]
    pub fn success(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: false,
            stop_loop: false,
        }
    }

    /// Create an error result.
    #[must_use]
    pub fn error(tool_use_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_use_id: tool_use_id.into(),
            content: content.into(),
            is_error: true,
            stop_loop: false,
        }
    }
}

/// Result of an automatic build check.
#[derive(Debug, Clone, Default)]
pub struct AutoBuildResult {
    /// Whether the build succeeded.
    pub success: bool,
    /// Build output (stdout + stderr).
    pub output: String,
    /// Number of errors detected.
    pub error_count: usize,
}

/// Captured build error baseline for distinguishing pre-existing from new errors.
#[derive(Debug, Clone, Default)]
pub struct BuildBaseline {
    /// Error signatures from the baseline build.
    pub error_signatures: Vec<String>,
}

/// Result of the full agent loop execution.
#[derive(Debug, Default)]
pub struct AgentLoopResult {
    /// Whether the loop timed out.
    pub timed_out: bool,
    /// Whether the loop stopped due to insufficient credits.
    pub insufficient_credits: bool,
    /// LLM error that terminated the loop, if any.
    pub llm_error: Option<String>,
    /// Accumulated assistant text across all iterations.
    pub total_text: String,
    /// Accumulated thinking text across all iterations.
    pub total_thinking: String,
    /// Total input tokens used.
    pub total_input_tokens: u64,
    /// Total output tokens used.
    pub total_output_tokens: u64,
    /// Number of iterations completed.
    pub iterations: usize,
    /// Final message history.
    pub messages: Vec<aura_reasoner::Message>,
}

/// Implementors execute tool calls and optionally provide build integration.
///
/// `aura-runtime` provides a default implementation wrapping `ExecutorRouter`.
/// `aura-app` can implement this with project-aware paths, domain tools
/// (spec/task CRUD, dev loop, engine phase gating), and event forwarding.
#[async_trait]
pub trait AgentToolExecutor: Send + Sync {
    /// Execute a batch of tool calls.
    ///
    /// Implementations may:
    /// - Gate certain tools (e.g., writes before `submit_plan`)
    /// - Dispatch domain tools to external services
    /// - Track file operations for stub detection
    /// - Signal loop termination via `stop_loop`
    async fn execute(&self, tool_calls: &[ToolCallInfo]) -> Vec<ToolCallResult>;

    /// Run a lightweight build check (e.g., `cargo check --lib`).
    ///
    /// Returns `None` when build checking is not configured.
    async fn auto_build_check(&self) -> Option<AutoBuildResult> {
        None
    }

    /// Capture current build error state as a baseline for distinguishing
    /// pre-existing errors from newly introduced ones.
    async fn capture_build_baseline(&self) -> Option<BuildBaseline> {
        None
    }
}
