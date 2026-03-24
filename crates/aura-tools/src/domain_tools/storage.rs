//! Storage domain tool handlers (logs, project stats).

use serde_json::{json, Value};
use tracing::debug;

use super::api::DomainApi;
use super::helpers::str_field;

pub async fn create_log(api: &dyn DomainApi, project_id: &str, input: &Value) -> String {
    debug!(project_id, "domain_tools: create_log");
    let message = input["message"].as_str().unwrap_or_default();
    let level = input["level"].as_str().unwrap_or("info");
    let metadata = input.get("metadata");
    let agent_id = str_field(input, "project_agent_id");

    match api.create_log(project_id, message, level, agent_id.as_deref(), metadata).await {
        Ok(result) => json!({ "ok": true, "result": result }).to_string(),
        Err(e) => json!({ "ok": false, "error": e.to_string() }).to_string(),
    }
}

pub async fn list_logs(api: &dyn DomainApi, project_id: &str, input: &Value) -> String {
    debug!(project_id, "domain_tools: list_logs");
    let level = str_field(input, "level");
    let limit = input["limit"].as_u64();

    match api.list_logs(project_id, level.as_deref(), limit).await {
        Ok(result) => json!({ "ok": true, "result": result }).to_string(),
        Err(e) => json!({ "ok": false, "error": e.to_string() }).to_string(),
    }
}

pub async fn get_project_stats(api: &dyn DomainApi, project_id: &str, _input: &Value) -> String {
    debug!(project_id, "domain_tools: get_project_stats");

    match api.get_project_stats(project_id).await {
        Ok(result) => json!({ "ok": true, "result": result }).to_string(),
        Err(e) => json!({ "ok": false, "error": e.to_string() }).to_string(),
    }
}
