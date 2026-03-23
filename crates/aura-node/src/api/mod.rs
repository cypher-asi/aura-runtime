//! HTTP API routes for tool management and service proxies.

pub mod network;
pub mod orbit;
pub mod storage;
mod tools;

pub use tools::{delete_tool_handler, get_tools_handler, install_tool_handler};

use std::sync::Arc;

/// Shared HTTP clients and service URLs for proxy endpoints.
#[derive(Clone)]
pub struct ServiceClients {
    pub http: reqwest::Client,
    pub orbit_url: String,
    pub aura_storage_url: String,
    pub aura_network_url: String,
    pub internal_token: Option<String>,
}

/// Per-request context for proxy handlers (state = ServiceClients, JWT from headers).
#[derive(Clone)]
pub struct ProxyContext {
    pub clients: Arc<ServiceClients>,
    pub session_jwt: Option<String>,
}

/// Extract JWT from an `Authorization: Bearer <token>` header.
pub fn extract_jwt(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(String::from)
}

/// Standard input body from installed tool HTTP POST.
#[derive(Debug, serde::Deserialize)]
pub struct ToolProxyRequest {
    #[serde(default)]
    pub tool_name: String,
    pub input: serde_json::Value,
    #[serde(default)]
    pub context: Option<ToolProxyContext>,
}

#[derive(Debug, serde::Deserialize)]
pub struct ToolProxyContext {
    #[serde(default)]
    pub workspace: String,
    #[serde(default)]
    pub agent_id: String,
}

/// Standard response from proxy handlers.
#[derive(Debug, serde::Serialize)]
pub struct ToolProxyResponse {
    pub output: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ToolProxyResponse {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            ok: true,
            error: None,
        }
    }

    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            output: String::new(),
            ok: false,
            error: Some(error.into()),
        }
    }
}
