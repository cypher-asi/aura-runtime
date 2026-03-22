//! Main agent loop orchestrator.
//!
//! `AgentLoop` drives the multi-step agentic conversation by calling
//! the step processor in a loop with intelligence:
//! blocking detection, compaction, sanitization, budget management, etc.

use std::collections::HashSet;
use std::time::Duration;

use aura_reasoner::{Message, ToolDefinition};
use tracing::{debug, info};

use crate::blocking::detection::{detect_all_blocked, BlockingContext};
use crate::blocking::stall::StallDetector;
use crate::budget::ExplorationState;
use crate::build;
use crate::compaction;
use crate::constants::{
    AUTO_BUILD_COOLDOWN, CHARS_PER_TOKEN, DEFAULT_EXPLORATION_ALLOWANCE, MAX_ITERATIONS,
    THINKING_MIN_BUDGET, THINKING_TAPER_AFTER, THINKING_TAPER_FACTOR,
};
use crate::helpers;
use crate::read_guard::ReadGuardState;
use crate::sanitize;
use crate::types::{
    AgentLoopResult, AgentToolExecutor, BuildBaseline, ToolCallInfo, ToolCallResult,
};

/// Configuration for the agent loop.
#[derive(Debug, Clone)]
pub struct AgentLoopConfig {
    /// Maximum iterations (model calls).
    pub max_iterations: usize,
    /// Maximum tokens per response.
    pub max_tokens: u32,
    /// Streaming timeout per iteration.
    pub stream_timeout: Duration,
    /// Credit attribution label.
    pub billing_reason: String,
    /// Loop-level model override.
    pub model_override: Option<String>,
    /// Maximum context tokens for compaction.
    pub max_context_tokens: Option<u64>,
    /// Credit budget (total tokens allowed).
    pub credit_budget: Option<u64>,
    /// Exploration allowance (read-only calls before warning).
    pub exploration_allowance: usize,
    /// Auto-build cooldown in iterations.
    pub auto_build_cooldown: usize,
    /// Thinking budget taper starts after this iteration.
    pub thinking_taper_after: usize,
    /// Factor to reduce thinking budget.
    pub thinking_taper_factor: f64,
    /// Minimum thinking budget after tapering.
    pub thinking_min_budget: u32,
    /// Additional tool definitions beyond core tools.
    pub extra_tools: Vec<ToolDefinition>,
    /// System prompt to use.
    pub system_prompt: String,
    /// Model name.
    pub model: String,
}

impl Default for AgentLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: MAX_ITERATIONS,
            max_tokens: 16_384,
            stream_timeout: Duration::from_secs(60),
            billing_reason: "agent_loop".to_string(),
            model_override: None,
            max_context_tokens: Some(200_000),
            credit_budget: None,
            exploration_allowance: DEFAULT_EXPLORATION_ALLOWANCE,
            auto_build_cooldown: AUTO_BUILD_COOLDOWN,
            thinking_taper_after: THINKING_TAPER_AFTER,
            thinking_taper_factor: THINKING_TAPER_FACTOR,
            thinking_min_budget: THINKING_MIN_BUDGET,
            extra_tools: Vec::new(),
            system_prompt: String::new(),
            model: "claude-opus-4-5-20251101".to_string(),
        }
    }
}

/// The main multi-step agent loop orchestrator.
pub struct AgentLoop {
    config: AgentLoopConfig,
}

impl AgentLoop {
    /// Create a new agent loop with the given configuration.
    #[must_use]
    pub const fn new(config: AgentLoopConfig) -> Self {
        Self { config }
    }

    /// Run the agent loop with the given executor and initial messages.
    ///
    /// This is the main entry point. It drives the multi-step conversation
    /// by processing tool calls, managing context, and applying intelligence.
    ///
    /// # Errors
    ///
    /// Returns error if a model call or tool execution fails fatally.
    #[allow(clippy::cast_precision_loss, clippy::unused_async)]
    pub async fn run(
        &self,
        _executor: &dyn AgentToolExecutor,
        mut messages: Vec<Message>,
        _tools: Vec<ToolDefinition>,
    ) -> anyhow::Result<AgentLoopResult> {
        let mut result = AgentLoopResult::default();

        info!(
            max_iterations = self.config.max_iterations,
            exploration_allowance = self.config.exploration_allowance,
            "Starting agent loop"
        );

        // Phase 1: setup and validation pass only.
        // Full model-call + tool-execution loop is wired in Phase 4.
        sanitize::validate_and_repair(&mut messages);

        if let Some(max_tokens) = self.config.max_context_tokens {
            let char_count = compaction::estimate_message_chars(&messages);
            let estimated_tokens = char_count / CHARS_PER_TOKEN;
            let utilization = estimated_tokens as f64 / max_tokens as f64;

            if let Some(tier) = compaction::select_tier(utilization) {
                debug!(utilization, "Compacting context");
                compaction::compact_older_messages(&mut messages, &tier);
                sanitize::validate_and_repair(&mut messages);
            }
        }

        result.iterations = 1;
        result.messages = messages;
        Ok(result)
    }

    /// Process tool call results from one iteration.
    ///
    /// Applies blocking detection, tracks writes/reads/commands,
    /// manages exploration budget, and handles build checks.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn process_tool_results(
        &self,
        tool_calls: &[ToolCallInfo],
        executor: &dyn AgentToolExecutor,
        blocking_ctx: &mut BlockingContext,
        read_guard: &mut ReadGuardState,
        exploration_state: &mut ExplorationState,
        stall_detector: &mut StallDetector,
        messages: &mut Vec<Message>,
        had_any_write: &mut bool,
        build_cooldown: &mut usize,
        build_baseline: Option<&BuildBaseline>,
    ) -> Vec<ToolCallResult> {
        let mut blocked_results = Vec::new();
        let mut to_execute = Vec::new();

        for tool in tool_calls {
            let check = detect_all_blocked(tool, blocking_ctx, read_guard);
            if check.blocked {
                let msg = check
                    .recovery_message
                    .unwrap_or_else(|| "Blocked".to_string());
                helpers::push_or_replace_warning(messages, &msg);
                blocked_results.push(ToolCallResult {
                    tool_use_id: tool.id.clone(),
                    content: msg,
                    is_error: true,
                    stop_loop: false,
                });
            } else {
                to_execute.push(tool.clone());
            }
        }

        let executed = if to_execute.is_empty() {
            Vec::new()
        } else {
            executor.execute(&to_execute).await
        };

        let mut write_targets = HashSet::new();
        let mut any_write_success = false;

        for exec_result in &executed {
            let tool = to_execute.iter().find(|t| t.id == exec_result.tool_use_id);
            if let Some(tool) = tool {
                if helpers::is_exploration_tool(&tool.name) {
                    exploration_state.count += 1;
                    let path = tool.input.get("path").and_then(|v| v.as_str());
                    if let Some(path) = path {
                        let is_range = tool.input.get("start_line").is_some();
                        if is_range {
                            read_guard.record_range_read(path);
                        } else {
                            read_guard.record_full_read(path);
                        }
                    }
                }

                if helpers::is_write_tool(&tool.name) {
                    if let Some(path) = tool.input.get("path").and_then(|v| v.as_str()) {
                        write_targets.insert(path.to_string());
                        if exec_result.is_error {
                            blocking_ctx.on_write_failure(path);
                        } else {
                            blocking_ctx.on_write_success(path, read_guard);
                            any_write_success = true;
                            *had_any_write = true;
                        }
                    }
                }

                if crate::constants::COMMAND_TOOLS.contains(&tool.name.as_str()) {
                    blocking_ctx.on_command_result(!exec_result.is_error);
                }
            }
        }

        let _stalled = stall_detector.update(&write_targets, any_write_success);

        if any_write_success && *build_cooldown == 0 {
            if let Some(build_result) = executor.auto_build_check().await {
                *build_cooldown = self.config.auto_build_cooldown;
                if !build_result.success {
                    let annotated = if let Some(baseline) = build_baseline {
                        build::annotate_build_output(&build_result.output, baseline)
                    } else {
                        build_result.output.clone()
                    };
                    messages.push(Message::user(format!(
                        "Build check failed with {} error(s):\n\n{annotated}",
                        build_result.error_count
                    )));
                }
            }
        }

        if any_write_success {
            blocking_ctx.exploration_allowance += 2;
        }

        let mut all_results = blocked_results;
        all_results.extend(executed);
        all_results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockExecutor {
        results: Vec<ToolCallResult>,
    }

    #[async_trait::async_trait]
    impl AgentToolExecutor for MockExecutor {
        async fn execute(&self, _tool_calls: &[ToolCallInfo]) -> Vec<ToolCallResult> {
            self.results.clone()
        }
    }

    #[test]
    fn test_agent_loop_config_defaults() {
        let config = AgentLoopConfig::default();
        assert_eq!(config.max_iterations, 25);
        assert_eq!(config.exploration_allowance, 12);
        assert_eq!(config.auto_build_cooldown, 2);
        assert_eq!(config.thinking_taper_after, 2);
        assert!((config.thinking_taper_factor - 0.6).abs() < f64::EPSILON);
        assert_eq!(config.thinking_min_budget, 1024);
    }

    #[tokio::test]
    async fn test_agent_loop_simple_run() {
        let config = AgentLoopConfig::default();
        let agent = AgentLoop::new(config);
        let executor = MockExecutor { results: vec![] };
        let messages = vec![Message::user("hello")];
        let tools = vec![];

        let result = agent.run(&executor, messages, tools).await.unwrap();
        assert!(result.iterations > 0);
    }

    #[test]
    fn test_tool_call_result_defaults() {
        let result = ToolCallResult::success("id", "content");
        assert!(!result.is_error);
        assert!(!result.stop_loop);

        let err = ToolCallResult::error("id", "error");
        assert!(err.is_error);
        assert!(!err.stop_loop);
    }
}
