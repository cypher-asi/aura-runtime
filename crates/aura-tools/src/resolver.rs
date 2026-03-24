//! Tool resolver — unified dispatch layer for tool execution.
//!
//! The resolver owns the internal `Tool` implementations (built-in handlers)
//! and falls back to HTTP-installed tools from the [`ToolCatalog`] when an
//! internal handler is not found.  This implements the `internal_first`
//! precedence policy described in the architecture plan.

use crate::catalog::ToolCatalog;
use crate::catalog::ToolProfile;
use crate::domain_tools::DomainToolExecutor;
use crate::error::ToolError;
use crate::installed::InstalledTool;
use crate::sandbox::Sandbox;
use crate::tool::{builtin_tools, Tool, ToolContext};
use crate::ToolConfig;
use async_trait::async_trait;
use aura_core::{
    Action, ActionKind, Effect, EffectKind, EffectStatus, InstalledToolDefinition, ToolCall,
    ToolResult,
};
use aura_executor::{ExecuteContext, Executor};
use aura_reasoner::ToolDefinition;
use bytes::Bytes;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, instrument};

/// Unified tool resolver providing both visibility and execution dispatch.
///
/// Implements [`Executor`] so it can be plugged into the kernel layer
/// (scheduler, `ExecutorRouter`) as a drop-in replacement for `ToolExecutor`.
pub struct ToolResolver {
    catalog: Arc<ToolCatalog>,
    tools: HashMap<String, Box<dyn Tool>>,
    domain_executor: Option<Arc<DomainToolExecutor>>,
    config: ToolConfig,
}

impl ToolResolver {
    /// Create a resolver pre-loaded with all built-in tool handlers.
    #[must_use]
    pub fn new(catalog: Arc<ToolCatalog>, config: ToolConfig) -> Self {
        let mut tools: HashMap<String, Box<dyn Tool>> = HashMap::new();
        for tool in builtin_tools() {
            tools.insert(tool.name().to_string(), tool);
        }
        Self {
            catalog,
            tools,
            domain_executor: None,
            config,
        }
    }

    /// Attach a domain tool executor for specs/tasks/project dispatch.
    #[must_use]
    pub fn with_domain_executor(mut self, exec: Arc<DomainToolExecutor>) -> Self {
        self.domain_executor = Some(exec);
        self
    }

    /// Visible tools for a profile (delegates to the catalog + config).
    #[must_use]
    pub fn visible_tools(&self, profile: ToolProfile) -> Vec<ToolDefinition> {
        self.catalog.visible_tools(profile, &self.config)
    }

    /// Register an additional internal tool at runtime.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Register an installed tool for direct execution (e.g. session-scoped
    /// tools that need an auth-token override).
    ///
    /// # Errors
    /// Returns `ToolError` if the HTTP client cannot be built.
    pub fn register_installed(&mut self, def: InstalledToolDefinition) -> Result<(), ToolError> {
        let tool = InstalledTool::new(def)?;
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        Ok(())
    }

    /// Execute a tool call with `internal_first` precedence:
    /// 1. Check permission gates.
    /// 2. Try the internal handler map.
    /// 3. Fall back to HTTP-installed tool from the catalog.
    #[instrument(skip(self, ctx), fields(tool = %tool_call.tool))]
    async fn execute_tool(
        &self,
        ctx: &ExecuteContext,
        tool_call: &ToolCall,
    ) -> Result<ToolResult, ToolError> {
        let tool_name = &tool_call.tool;

        const FS_TOOLS: &[&str] = &[
            "read_file",
            "write_file",
            "edit_file",
            "delete_file",
            "list_files",
            "find_files",
            "stat_file",
            "search_code",
        ];
        const CMD_TOOLS: &[&str] = &["run_command"];

        if FS_TOOLS.contains(&tool_name.as_str()) && !self.config.enable_fs {
            return Err(ToolError::ToolDisabled(tool_name.clone()));
        }
        if CMD_TOOLS.contains(&tool_name.as_str()) && !self.config.enable_commands {
            return Err(ToolError::ToolDisabled(tool_name.clone()));
        }

        let sandbox = Sandbox::new(&ctx.workspace_root)?;
        let tool_ctx = ToolContext {
            sandbox,
            config: self.config.clone(),
        };

        // 1. Internal handler (highest precedence)
        if let Some(tool) = self.tools.get(tool_name.as_str()) {
            return tool.execute(&tool_ctx, tool_call.args.clone()).await;
        }

        // 2. Domain tools (specs, tasks, project)
        if let Some(ref domain) = self.domain_executor {
            if domain.handles(tool_name) {
                let project_id = tool_call.args["project_id"]
                    .as_str()
                    .unwrap_or_default();
                let result_json = domain
                    .execute(tool_name, project_id, &tool_call.args)
                    .await;
                return Ok(ToolResult::success(tool_name, result_json));
            }
        }

        // 3. HTTP fallback from catalog
        if let Some(def) = self.catalog.get_installed(tool_name) {
            debug!(tool = %tool_name, "Falling back to HTTP-installed handler");
            let installed = InstalledTool::new(def)?;
            return installed.execute(&tool_ctx, tool_call.args.clone()).await;
        }

        Err(ToolError::UnknownTool(tool_name.clone()))
    }
}

// ---------------------------------------------------------------------------
// Executor trait impl  — allows the resolver to be used in ExecutorRouter
// ---------------------------------------------------------------------------

#[async_trait]
impl Executor for ToolResolver {
    #[instrument(skip(self, ctx, action), fields(action_id = %action.action_id))]
    async fn execute(
        &self,
        ctx: &ExecuteContext,
        action: &Action,
    ) -> Result<Effect, aura_executor::ExecutorError> {
        let tool_call: ToolCall = serde_json::from_slice(&action.payload).map_err(|e| {
            aura_executor::ExecutorError::ExecutionFailed(format!(
                "Failed to parse tool call: {e}"
            ))
        })?;

        debug!(tool = %tool_call.tool, "Executing tool via resolver");

        match self.execute_tool(ctx, &tool_call).await {
            Ok(result) => {
                let payload = serde_json::to_vec(&result).map_err(|e| {
                    aura_executor::ExecutorError::ExecutionFailed(format!(
                        "Failed to serialize tool result: {e}"
                    ))
                })?;
                Ok(Effect::new(
                    action.action_id,
                    EffectKind::Agreement,
                    EffectStatus::Committed,
                    Bytes::from(payload),
                ))
            }
            Err(e) => {
                error!(error = %e, "Tool execution failed");
                let result = ToolResult::failure(&tool_call.tool, e.to_string());
                let payload = serde_json::to_vec(&result).map_err(|e| {
                    aura_executor::ExecutorError::ExecutionFailed(format!(
                        "Failed to serialize error result: {e}"
                    ))
                })?;
                Ok(Effect::new(
                    action.action_id,
                    EffectKind::Agreement,
                    EffectStatus::Failed,
                    Bytes::from(payload),
                ))
            }
        }
    }

    fn can_handle(&self, action: &Action) -> bool {
        if action.kind != ActionKind::Delegate {
            return false;
        }
        serde_json::from_slice::<ToolCall>(&action.payload).is_ok()
    }

    fn name(&self) -> &'static str {
        "tool_resolver"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::{ActionId, AgentId, ToolAuth};
    use tempfile::TempDir;

    fn make_catalog_and_resolver() -> (Arc<ToolCatalog>, ToolResolver) {
        let cat = Arc::new(ToolCatalog::new());
        let resolver = ToolResolver::new(cat.clone(), ToolConfig::default());
        (cat, resolver)
    }

    fn test_context() -> (ExecuteContext, TempDir) {
        let dir = TempDir::new().unwrap();
        let ctx = ExecuteContext::new(
            AgentId::generate(),
            ActionId::generate(),
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }

    #[test]
    fn resolver_has_builtin_tools() {
        let (_cat, resolver) = make_catalog_and_resolver();
        assert!(resolver.tools.contains_key("read_file"));
        assert!(resolver.tools.contains_key("run_command"));
    }

    #[test]
    fn visible_tools_returns_core() {
        let (_cat, resolver) = make_catalog_and_resolver();
        let tools = resolver.visible_tools(ToolProfile::Core);
        let names: std::collections::HashSet<_> =
            tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains("read_file"));
    }

    #[tokio::test]
    async fn execute_builtin_tool() {
        let (_cat, resolver) = make_catalog_and_resolver();
        let (ctx, dir) = test_context();
        std::fs::write(dir.path().join("hello.txt"), "world").unwrap();

        let tc = ToolCall::fs_ls(".");
        let action = Action::delegate_tool(&tc).unwrap();
        let effect = resolver.execute(&ctx, &action).await.unwrap();
        assert_eq!(effect.status, EffectStatus::Committed);
    }

    #[tokio::test]
    async fn unknown_tool_returns_failed_effect() {
        let (_cat, resolver) = make_catalog_and_resolver();
        let (ctx, _dir) = test_context();

        let tc = ToolCall::new("no_such_tool", serde_json::json!({}));
        let action = Action::delegate_tool(&tc).unwrap();
        let effect = resolver.execute(&ctx, &action).await.unwrap();
        assert_eq!(effect.status, EffectStatus::Failed);
    }

    #[tokio::test]
    async fn fs_disabled_returns_failed() {
        let cat = Arc::new(ToolCatalog::new());
        let mut config = ToolConfig::default();
        config.enable_fs = false;
        let resolver = ToolResolver::new(cat, config);
        let (ctx, _dir) = test_context();

        let tc = ToolCall::fs_read("test.txt", None);
        let action = Action::delegate_tool(&tc).unwrap();
        let effect = resolver.execute(&ctx, &action).await.unwrap();
        assert_eq!(effect.status, EffectStatus::Failed);
    }

    #[test]
    fn register_installed_adds_to_map() {
        let (_cat, mut resolver) = make_catalog_and_resolver();
        assert!(!resolver.tools.contains_key("my_http_tool"));

        resolver
            .register_installed(InstalledToolDefinition {
                name: "my_http_tool".into(),
                description: "test".into(),
                input_schema: serde_json::json!({"type": "object"}),
                endpoint: "http://localhost:9999/tool".into(),
                auth: ToolAuth::None,
                timeout_ms: None,
                namespace: None,
                metadata: Default::default(),
            })
            .unwrap();
        assert!(resolver.tools.contains_key("my_http_tool"));
    }

    #[test]
    fn every_exposed_core_tool_has_handler() {
        let (cat, resolver) = make_catalog_and_resolver();
        let core = cat.tools_for_profile(ToolProfile::Core);
        for t in &core {
            let has_handler = resolver.tools.contains_key(t.name.as_str());
            let is_domain = [
                "list_specs",
                "get_spec",
                "create_spec",
                "update_spec",
                "delete_spec",
                "list_tasks",
                "create_task",
                "update_task",
                "delete_task",
                "transition_task",
                "run_task",
                "get_project",
                "update_project",
                "start_dev_loop",
                "pause_dev_loop",
                "stop_dev_loop",
                "task_done",
                "get_task_context",
                "submit_plan",
            ]
            .contains(&t.name.as_str());
            assert!(
                has_handler || is_domain,
                "core tool '{}' has no handler and is not a known domain tool",
                t.name,
            );
        }
    }
}
