//! Bridge between `AutomatonController` (defined in `aura-tools`) and the
//! concrete `AutomatonRuntime` + automaton types (from `aura-automaton`).
//!
//! This module lives in `aura-node` because it depends on both crates.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use tracing::info;

use aura_automaton::{AutomatonHandle, AutomatonRuntime, DevLoopAutomaton, TaskRunAutomaton};
use aura_agent::agent_runner::AgentRunnerConfig;
use aura_reasoner::ModelProvider;
use aura_tools::automaton_tools::AutomatonController;
use aura_tools::catalog::ToolCatalog;
use aura_tools::domain_tools::DomainApi;

/// Concrete [`AutomatonController`] wired to the real runtime.
pub struct AutomatonBridge {
    runtime: Arc<AutomatonRuntime>,
    domain: Arc<dyn DomainApi>,
    provider: Arc<dyn ModelProvider + Send + Sync>,
    catalog: Arc<ToolCatalog>,
    /// project_id -> (automaton_id, handle)
    project_handles: Arc<DashMap<String, (String, AutomatonHandle)>>,
}

impl AutomatonBridge {
    pub fn new(
        runtime: Arc<AutomatonRuntime>,
        domain: Arc<dyn DomainApi>,
        provider: Arc<dyn ModelProvider + Send + Sync>,
        catalog: Arc<ToolCatalog>,
    ) -> Self {
        Self {
            runtime,
            domain,
            provider,
            catalog,
            project_handles: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl AutomatonController for AutomatonBridge {
    async fn start_dev_loop(
        &self,
        project_id: &str,
        workspace_root: Option<PathBuf>,
    ) -> Result<String, String> {
        // Prevent duplicate loops for the same project.
        if let Some(entry) = self.project_handles.get(project_id) {
            let (ref id, ref handle) = *entry;
            if !handle.is_finished() {
                return Err(format!(
                    "A dev loop is already running for project {project_id} (automaton_id: {id})"
                ));
            }
            // Previous run finished; clean up stale entry.
            drop(entry);
            self.project_handles.remove(project_id);
        }

        let automaton = DevLoopAutomaton::new(
            self.domain.clone(),
            self.provider.clone(),
            AgentRunnerConfig::default(),
            self.catalog.clone(),
        );

        let config = serde_json::json!({
            "project_id": project_id,
        });

        let (handle, _event_rx) = self
            .runtime
            .install(Box::new(automaton), config, workspace_root)
            .await
            .map_err(|e| format!("failed to install dev-loop automaton: {e}"))?;

        let automaton_id = handle.id().as_str().to_string();
        info!(project_id, automaton_id = %automaton_id, "Dev loop started");
        self.project_handles
            .insert(project_id.to_string(), (automaton_id.clone(), handle));
        Ok(automaton_id)
    }

    async fn pause_dev_loop(&self, project_id: &str) -> Result<(), String> {
        let entry = self
            .project_handles
            .get(project_id)
            .ok_or_else(|| format!("No running dev loop for project {project_id}"))?;
        let (_, ref handle) = *entry;
        if handle.is_finished() {
            return Err("Dev loop has already finished".into());
        }
        handle.pause();
        info!(project_id, "Dev loop paused");
        Ok(())
    }

    async fn stop_dev_loop(&self, project_id: &str) -> Result<(), String> {
        let entry = self
            .project_handles
            .get(project_id)
            .ok_or_else(|| format!("No running dev loop for project {project_id}"))?;
        let (ref id, ref handle) = *entry;
        if handle.is_finished() {
            drop(entry);
            self.project_handles.remove(project_id);
            return Err("Dev loop has already finished".into());
        }
        let automaton_id = id.clone();
        handle.stop();
        drop(entry);
        self.project_handles.remove(project_id);
        info!(project_id, automaton_id = %automaton_id, "Dev loop stopped");
        Ok(())
    }

    async fn run_task(
        &self,
        project_id: &str,
        task_id: &str,
        workspace_root: Option<PathBuf>,
    ) -> Result<String, String> {
        let automaton = TaskRunAutomaton::new(
            self.domain.clone(),
            self.provider.clone(),
            AgentRunnerConfig::default(),
            self.catalog.clone(),
        );

        let config = serde_json::json!({
            "project_id": project_id,
            "task_id": task_id,
        });

        let (mut handle, _event_rx) = self
            .runtime
            .install(Box::new(automaton), config, workspace_root)
            .await
            .map_err(|e| format!("failed to install task-run automaton: {e}"))?;

        let automaton_id = handle.id().as_str().to_string();
        info!(project_id, task_id, automaton_id = %automaton_id, "Single task execution started");

        handle.wait().await;

        let status = handle.status();
        match status {
            aura_automaton::AutomatonStatus::Completed => {
                Ok(format!("Task {task_id} executed successfully"))
            }
            aura_automaton::AutomatonStatus::Failed => {
                Err(format!("Task {task_id} execution failed"))
            }
            other => Err(format!("Task {task_id} ended with status {other:?}")),
        }
    }
}
