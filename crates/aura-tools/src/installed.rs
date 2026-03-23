//! Installed tool support via HTTP POST.
//!
//! Installed tools implement the [`Tool`] trait and dispatch execution
//! to a remote service via HTTP POST to an endpoint URL.

use crate::error::ToolError;
use crate::tool::{Tool, ToolContext};
use async_trait::async_trait;
use aura_core::{InstalledToolDefinition, ToolAuth, ToolCallContext, ToolResult};
use aura_reasoner::ToolDefinition;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Request body posted to the installed tool's endpoint.
#[derive(Debug, Serialize)]
struct InstalledToolRequest {
    tool_name: String,
    input: serde_json::Value,
    context: ToolCallContext,
}

/// Expected response body from the installed tool endpoint.
#[derive(Debug, Deserialize)]
struct InstalledToolResponse {
    #[serde(default)]
    output: String,
    #[serde(default = "default_true")]
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

const fn default_true() -> bool {
    true
}

const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// An installed tool that dispatches execution via HTTP POST.
pub struct InstalledTool {
    def: InstalledToolDefinition,
    client: reqwest::Client,
}

impl InstalledTool {
    /// Create a new installed tool from a definition.
    ///
    /// # Errors
    /// Returns `ToolError::ExternalToolError` if the HTTP client cannot be built.
    pub(crate) fn new(def: InstalledToolDefinition) -> Result<Self, ToolError> {
        let timeout = def
            .timeout_ms
            .map_or(DEFAULT_TIMEOUT, |ms| std::time::Duration::from_millis(ms));

        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| {
                ToolError::ExternalToolError(format!("Failed to build HTTP client: {e}"))
            })?;
        Ok(Self { def, client })
    }

    /// Create a new installed tool with a custom HTTP client.
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn with_client(def: InstalledToolDefinition, client: reqwest::Client) -> Self {
        Self { def, client }
    }

    /// Apply authentication from the tool definition to a request builder.
    fn apply_auth(
        &self,
        builder: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        match &self.def.auth {
            ToolAuth::None => builder,
            ToolAuth::Bearer { token } => {
                builder.header("Authorization", format!("Bearer {token}"))
            }
            ToolAuth::ApiKey { header, key } => builder.header(header.as_str(), key.as_str()),
            ToolAuth::Headers { headers } => {
                let mut b = builder;
                for (k, v) in headers {
                    b = b.header(k.as_str(), v.as_str());
                }
                b
            }
        }
    }
}

#[async_trait]
impl Tool for InstalledTool {
    fn name(&self) -> &str {
        &self.def.name
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.def.name.clone(),
            description: self.def.description.clone(),
            input_schema: self.def.input_schema.clone(),
            cache_control: None,
        }
    }

    #[instrument(skip(self, ctx, args), fields(tool = %self.def.name, url = %self.def.endpoint))]
    async fn execute(
        &self,
        ctx: &ToolContext,
        args: serde_json::Value,
    ) -> Result<ToolResult, ToolError> {
        debug!("Dispatching to installed tool endpoint");

        let context = ToolCallContext {
            workspace: ctx.sandbox.root().to_string_lossy().to_string(),
            agent_id: String::new(),
        };

        let request_body = InstalledToolRequest {
            tool_name: self.def.name.clone(),
            input: args,
            context,
        };

        let builder = self.client.post(&self.def.endpoint).json(&request_body);
        let builder = self.apply_auth(builder);

        let response = builder.send().await.map_err(|e| {
            ToolError::ExternalToolError(format!(
                "HTTP request to {} failed: {e}",
                self.def.endpoint
            ))
        })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let truncated: String = body.chars().take(200).collect();
            return Err(ToolError::ExternalToolError(format!(
                "Installed tool returned HTTP {status}: {truncated}"
            )));
        }

        let tool_response: InstalledToolResponse = response.json().await.map_err(|e| {
            ToolError::ExternalToolError(format!("Failed to parse endpoint response: {e}"))
        })?;

        if tool_response.ok {
            Ok(ToolResult::success(&self.def.name, tool_response.output))
        } else {
            let error_msg = tool_response
                .error
                .unwrap_or_else(|| "installed tool failed".to_string());
            Ok(ToolResult::failure(&self.def.name, error_msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::ToolAuth;

    fn sample_def() -> InstalledToolDefinition {
        InstalledToolDefinition {
            name: "my_tool".into(),
            description: "Does things".into(),
            input_schema: serde_json::json!({"type": "object"}),
            endpoint: "http://localhost:8080/tool".into(),
            auth: ToolAuth::None,
            timeout_ms: None,
            namespace: None,
            metadata: Default::default(),
        }
    }

    #[test]
    fn test_installed_tool_definition_roundtrip() {
        let def = sample_def();
        let json = serde_json::to_string(&def).unwrap();
        let parsed: InstalledToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "my_tool");
        assert_eq!(parsed.endpoint, "http://localhost:8080/tool");
    }

    #[test]
    fn test_installed_tool_produces_correct_definition() {
        let mut def = sample_def();
        def.name = "ext_search".into();
        def.description = "Search external index".into();
        def.input_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        });
        def.endpoint = "http://example.com/search".into();

        let tool = InstalledTool::new(def).unwrap();
        assert_eq!(tool.name(), "ext_search");

        let tool_def = tool.definition();
        assert_eq!(tool_def.name, "ext_search");
        assert_eq!(tool_def.description, "Search external index");
        assert!(tool_def.input_schema["properties"]["query"].is_object());
    }

    #[test]
    fn test_installed_tool_with_client_constructor() {
        let def = sample_def();
        let client = reqwest::Client::new();
        let tool = InstalledTool::with_client(def, client);
        assert_eq!(tool.name(), "my_tool");
    }

    #[test]
    fn test_installed_tool_definition_cache_control_is_none() {
        let def = sample_def();
        let tool = InstalledTool::new(def).unwrap();
        assert!(tool.definition().cache_control.is_none());
    }

    #[test]
    fn test_installed_tool_response_deserialization_defaults() {
        let json = r#"{"output": "hello"}"#;
        let resp: InstalledToolResponse = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert!(resp.error.is_none());
        assert_eq!(resp.output, "hello");
    }

    #[test]
    fn test_installed_tool_response_deserialization_failure() {
        let json = r#"{"ok": false, "error": "bad input"}"#;
        let resp: InstalledToolResponse = serde_json::from_str(json).unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.error.as_deref(), Some("bad input"));
        assert!(resp.output.is_empty());
    }

    #[test]
    fn test_installed_tool_response_deserialization_empty() {
        let json = r#"{}"#;
        let resp: InstalledToolResponse = serde_json::from_str(json).unwrap();
        assert!(resp.ok);
        assert!(resp.output.is_empty());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_installed_tool_request_serialization() {
        let req = InstalledToolRequest {
            tool_name: "my_tool".into(),
            input: serde_json::json!({"key": "value"}),
            context: ToolCallContext {
                workspace: "/tmp/ws".into(),
                agent_id: "agent-1".into(),
            },
        };
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["tool_name"], "my_tool");
        assert_eq!(json["input"]["key"], "value");
        assert_eq!(json["context"]["workspace"], "/tmp/ws");
    }

    #[test]
    fn test_installed_tool_with_timeout() {
        let mut def = sample_def();
        def.timeout_ms = Some(60_000);
        let tool = InstalledTool::new(def).unwrap();
        assert_eq!(tool.name(), "my_tool");
    }

    #[test]
    fn test_installed_tool_with_bearer_auth() {
        let mut def = sample_def();
        def.auth = ToolAuth::Bearer {
            token: "secret".into(),
        };
        let tool = InstalledTool::new(def).unwrap();
        assert_eq!(tool.name(), "my_tool");
    }
}
