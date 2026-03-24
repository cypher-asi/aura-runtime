//! Domain tool handlers and the `DomainApi` callback trait.
//!
//! This module provides the harness-side tool handlers for domain operations
//! (specs, tasks, projects, orbit, network, storage). The actual data access
//! is delegated through the [`DomainApi`] trait so that the harness never
//! depends on app-level crates.

pub mod api;
pub mod helpers;
pub mod network;
pub mod orbit;
pub mod project;
pub mod specs;
pub mod storage;
pub mod tasks;

pub use api::*;

use serde_json::{json, Value};
use std::sync::Arc;
use tracing::warn;

const DOMAIN_TOOL_NAMES: &[&str] = &[
    "list_specs", "get_spec", "create_spec", "update_spec", "delete_spec",
    "list_tasks", "get_task", "create_task", "update_task", "delete_task", "transition_task",
    "get_project", "update_project",
    "create_log", "list_logs", "get_project_stats",
    "orbit_push", "orbit_create_repo", "orbit_list_repos", "orbit_list_branches",
    "orbit_create_branch", "orbit_list_commits", "orbit_get_diff",
    "orbit_create_pr", "orbit_list_prs", "orbit_merge_pr",
    "post_to_feed", "list_projects", "check_budget", "record_usage",
];

/// Dispatches domain tool calls to the appropriate handler via `DomainApi`.
pub struct DomainToolExecutor {
    api: Arc<dyn DomainApi>,
    /// Per-session JWT for orbit/network calls that need user auth.
    session_jwt: Option<String>,
}

impl DomainToolExecutor {
    pub fn new(api: Arc<dyn DomainApi>) -> Self {
        Self {
            api,
            session_jwt: None,
        }
    }

    /// Create an executor with a session-scoped JWT for orbit/network auth.
    pub fn with_session_jwt(api: Arc<dyn DomainApi>, jwt: Option<String>) -> Self {
        Self {
            api,
            session_jwt: jwt,
        }
    }

    /// Inject the session JWT into input args so orbit/network handlers can use it.
    fn inject_jwt(&self, input: &Value) -> Value {
        let mut patched = input.clone();
        if let (Some(jwt), Some(obj)) = (&self.session_jwt, patched.as_object_mut()) {
            if !obj.contains_key("jwt") {
                obj.insert("jwt".to_string(), Value::String(jwt.clone()));
            }
        }
        patched
    }

    /// Execute a domain tool by name.
    ///
    /// `project_id` is threaded through from the session context.
    /// Returns a JSON string result (always contains an `ok` field).
    pub async fn execute(&self, tool_name: &str, project_id: &str, input: &Value) -> String {
        match tool_name {
            // Specs
            "list_specs" => specs::list_specs(self.api.as_ref(), project_id, input).await,
            "get_spec" => specs::get_spec(self.api.as_ref(), project_id, input).await,
            "create_spec" => specs::create_spec(self.api.as_ref(), project_id, input).await,
            "update_spec" => specs::update_spec(self.api.as_ref(), project_id, input).await,
            "delete_spec" => specs::delete_spec(self.api.as_ref(), project_id, input).await,

            // Tasks
            "list_tasks" => tasks::list_tasks(self.api.as_ref(), project_id, input).await,
            "get_task" => tasks::get_task(self.api.as_ref(), project_id, input).await,
            "create_task" => tasks::create_task(self.api.as_ref(), project_id, input).await,
            "update_task" => tasks::update_task(self.api.as_ref(), project_id, input).await,
            "delete_task" => tasks::delete_task(self.api.as_ref(), project_id, input).await,
            "transition_task" => tasks::transition_task(self.api.as_ref(), project_id, input).await,

            // Project
            "get_project" => project::get_project(self.api.as_ref(), project_id, input).await,
            "update_project" => project::update_project(self.api.as_ref(), project_id, input).await,

            // Storage (logs, stats)
            "create_log" => storage::create_log(self.api.as_ref(), project_id, input).await,
            "list_logs" => storage::list_logs(self.api.as_ref(), project_id, input).await,
            "get_project_stats" => {
                storage::get_project_stats(self.api.as_ref(), project_id, input).await
            }

            // Orbit (git operations — JWT injected from session)
            "orbit_push" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_push(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_create_repo" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_create_repo(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_list_repos" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_list_repos(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_list_branches" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_list_branches(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_create_branch" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_create_branch(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_list_commits" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_list_commits(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_get_diff" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_get_diff(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_create_pr" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_create_pr(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_list_prs" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_list_prs(self.api.as_ref(), project_id, &inp).await
            }
            "orbit_merge_pr" => {
                let inp = self.inject_jwt(input);
                orbit::orbit_merge_pr(self.api.as_ref(), project_id, &inp).await
            }

            // Network (social, billing — JWT injected for user-scoped calls)
            "post_to_feed" => {
                let inp = self.inject_jwt(input);
                network::post_to_feed(self.api.as_ref(), project_id, &inp).await
            }
            "list_projects" => {
                let inp = self.inject_jwt(input);
                network::network_list_projects(self.api.as_ref(), project_id, &inp).await
            }
            "check_budget" => {
                let inp = self.inject_jwt(input);
                network::check_budget(self.api.as_ref(), project_id, &inp).await
            }
            "record_usage" => {
                let inp = self.inject_jwt(input);
                network::record_usage(self.api.as_ref(), project_id, &inp).await
            }

            other => {
                warn!(tool = other, "unknown domain tool");
                json!({
                    "ok": false,
                    "error": format!("unknown domain tool: {other}")
                })
                .to_string()
            }
        }
    }

    /// Returns true if `tool_name` is a domain tool handled by this executor.
    pub fn handles(&self, tool_name: &str) -> bool {
        DOMAIN_TOOL_NAMES.contains(&tool_name)
    }

    /// List all domain tool names handled by this executor.
    pub fn tool_names(&self) -> &[&'static str] {
        DOMAIN_TOOL_NAMES
    }
}
