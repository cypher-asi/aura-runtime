//! Anthropic provider implementation.
//!
//! Uses `reqwest` directly to communicate with the Anthropic API.
//! This approach is more reliable than SDK wrappers which may have
//! incompatible or private internal types.

use crate::{
    ContentBlock, Message, ModelProvider, ModelRequest, ModelResponse, ProviderTrace, Role,
    StopReason, ToolChoice, ToolDefinition, ToolResultContent, Usage,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tracing::{debug, error, instrument};

// ============================================================================
// Configuration
// ============================================================================

/// Anthropic provider configuration.
#[derive(Debug, Clone)]
pub struct AnthropicConfig {
    /// API key
    pub api_key: String,
    /// Default model to use
    pub default_model: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retries
    pub max_retries: u32,
    /// API base URL
    pub base_url: String,
}

impl AnthropicConfig {
    /// Create a new config from environment variables.
    ///
    /// Reads:
    /// - `AURA_ANTHROPIC_API_KEY` or `ANTHROPIC_API_KEY`
    /// - `AURA_ANTHROPIC_MODEL` (defaults to "claude-sonnet-4-20250514")
    ///
    /// # Errors
    ///
    /// Returns error if API key is not set.
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("AURA_ANTHROPIC_API_KEY")
            .or_else(|_| std::env::var("ANTHROPIC_API_KEY"))
            .map_err(|_| anyhow::anyhow!("AURA_ANTHROPIC_API_KEY or ANTHROPIC_API_KEY not set"))?;

        let default_model = std::env::var("AURA_ANTHROPIC_MODEL")
            .unwrap_or_else(|_| "claude-opus-4-5-20251101".to_string());

        let timeout_ms = std::env::var("AURA_MODEL_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60_000);

        let base_url = std::env::var("AURA_ANTHROPIC_BASE_URL")
            .unwrap_or_else(|_| "https://api.anthropic.com".to_string());

        Ok(Self {
            api_key,
            default_model,
            timeout_ms,
            max_retries: 2,
            base_url,
        })
    }

    /// Create a config with explicit values.
    #[must_use]
    pub fn new(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            default_model: model.into(),
            timeout_ms: 60_000,
            max_retries: 2,
            base_url: "https://api.anthropic.com".to_string(),
        }
    }
}

// ============================================================================
// Provider Implementation
// ============================================================================

/// Anthropic model provider.
///
/// Implements `ModelProvider` for the Anthropic API using direct HTTP calls.
pub struct AnthropicProvider {
    client: reqwest::Client,
    config: AnthropicConfig,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// # Errors
    ///
    /// Returns error if client creation fails.
    pub fn new(config: AnthropicConfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()?;
        Ok(Self { client, config })
    }

    /// Create from environment variables.
    ///
    /// # Errors
    ///
    /// Returns error if configuration or client creation fails.
    pub fn from_env() -> anyhow::Result<Self> {
        let config = AnthropicConfig::from_env()?;
        Self::new(config)
    }
}

#[async_trait]
impl ModelProvider for AnthropicProvider {
    fn name(&self) -> &'static str {
        "anthropic"
    }

    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: ModelRequest) -> anyhow::Result<ModelResponse> {
        let start = Instant::now();

        // Build the API request
        let api_request = ApiRequest {
            model: request.model.clone(),
            system: request.system.clone(),
            messages: convert_messages_to_api(&request.messages),
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(convert_tools_to_api(&request.tools))
            },
            tool_choice: convert_tool_choice(&request.tool_choice),
            max_tokens: request.max_tokens,
            temperature: request.temperature,
        };

        debug!(
            messages = api_request.messages.len(),
            tools = api_request.tools.as_ref().map_or(0, Vec::len),
            "Sending request to Anthropic"
        );

        // Make the API call
        let response = self
            .client
            .post(format!("{}/v1/messages", self.config.base_url))
            .header("x-api-key", &self.config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&api_request)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, "Anthropic API request failed");
                anyhow::anyhow!("Anthropic API request failed: {e}")
            })?;

        let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

        // Check for HTTP errors
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!(status = %status, body = %body, "Anthropic API error");
            anyhow::bail!("Anthropic API error: {status} - {body}");
        }

        // Parse response
        let api_response: ApiResponse = response.json().await.map_err(|e| {
            error!(error = %e, "Failed to parse Anthropic response");
            anyhow::anyhow!("Failed to parse Anthropic response: {e}")
        })?;

        // Convert response to AURA types
        let message = convert_response_to_aura(&api_response.content);
        let stop_reason = match api_response.stop_reason.as_deref() {
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            Some("stop_sequence") => StopReason::StopSequence,
            _ => StopReason::EndTurn,
        };

        debug!(
            stop_reason = ?stop_reason,
            latency_ms = latency_ms,
            input_tokens = api_response.usage.input_tokens,
            output_tokens = api_response.usage.output_tokens,
            "Received response from Anthropic"
        );

        Ok(ModelResponse {
            stop_reason,
            message,
            usage: Usage {
                input_tokens: api_response.usage.input_tokens,
                output_tokens: api_response.usage.output_tokens,
            },
            trace: ProviderTrace {
                request_id: Some(api_response.id),
                latency_ms,
                model: api_response.model,
            },
        })
    }

    async fn health_check(&self) -> bool {
        // Simple health check - could be improved with a lightweight API call
        true
    }
}

// ============================================================================
// API Types (matching Anthropic's JSON schema)
// ============================================================================

#[derive(Debug, Serialize)]
struct ApiRequest {
    model: String,
    system: String,
    messages: Vec<ApiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ApiToolChoice>,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ApiMessage {
    role: String,
    content: Vec<ApiContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiContent {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
}

#[derive(Debug, Serialize)]
struct ApiTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ApiToolChoice {
    Auto,
    Any,
    Tool { name: String },
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    id: String,
    model: String,
    content: Vec<ApiContent>,
    stop_reason: Option<String>,
    usage: ApiUsage,
}

#[derive(Debug, Deserialize)]
struct ApiUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// ============================================================================
// Conversion Functions
// ============================================================================

fn convert_messages_to_api(messages: &[Message]) -> Vec<ApiMessage> {
    messages
        .iter()
        .map(|msg| {
            let role = match msg.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };

            let content: Vec<ApiContent> = msg
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => ApiContent::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => ApiContent::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        let content_text = match content {
                            ToolResultContent::Text(s) => s.clone(),
                            ToolResultContent::Json(v) => {
                                serde_json::to_string(v).unwrap_or_default()
                            }
                        };
                        ApiContent::ToolResult {
                            tool_use_id: tool_use_id.clone(),
                            content: content_text,
                            is_error: Some(*is_error),
                        }
                    }
                })
                .collect();

            ApiMessage {
                role: role.to_string(),
                content,
            }
        })
        .collect()
}

fn convert_tools_to_api(tools: &[ToolDefinition]) -> Vec<ApiTool> {
    tools
        .iter()
        .map(|tool| ApiTool {
            name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
        })
        .collect()
}

fn convert_tool_choice(choice: &ToolChoice) -> Option<ApiToolChoice> {
    match choice {
        ToolChoice::Auto => Some(ApiToolChoice::Auto),
        ToolChoice::None => None,
        ToolChoice::Required => Some(ApiToolChoice::Any),
        ToolChoice::Tool { name } => Some(ApiToolChoice::Tool { name: name.clone() }),
    }
}

fn convert_response_to_aura(content: &[ApiContent]) -> Message {
    let blocks: Vec<ContentBlock> = content
        .iter()
        .map(|c| match c {
            ApiContent::Text { text } => ContentBlock::Text { text: text.clone() },
            ApiContent::ToolUse { id, name, input } => ContentBlock::ToolUse {
                id: id.clone(),
                name: name.clone(),
                input: input.clone(),
            },
            ApiContent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => ContentBlock::ToolResult {
                tool_use_id: tool_use_id.clone(),
                content: ToolResultContent::Text(content.clone()),
                is_error: is_error.unwrap_or(false),
            },
        })
        .collect();

    Message {
        role: Role::Assistant,
        content: blocks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = AnthropicConfig::new("test-key", "claude-3-haiku");
        assert_eq!(config.api_key, "test-key");
        assert_eq!(config.default_model, "claude-3-haiku");
    }

    #[test]
    fn test_convert_messages() {
        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];

        let api_msgs = convert_messages_to_api(&messages);
        assert_eq!(api_msgs.len(), 2);
        assert_eq!(api_msgs[0].role, "user");
        assert_eq!(api_msgs[1].role, "assistant");
    }

    #[test]
    fn test_convert_tools() {
        let tools = vec![ToolDefinition::new(
            "fs.read",
            "Read a file",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                }
            }),
        )];

        let api_tools = convert_tools_to_api(&tools);
        assert_eq!(api_tools.len(), 1);
        assert_eq!(api_tools[0].name, "fs.read");
    }

    #[test]
    fn test_convert_tool_choice() {
        assert!(matches!(
            convert_tool_choice(&ToolChoice::Auto),
            Some(ApiToolChoice::Auto)
        ));
        assert!(matches!(
            convert_tool_choice(&ToolChoice::Required),
            Some(ApiToolChoice::Any)
        ));
        assert!(convert_tool_choice(&ToolChoice::None).is_none());
    }
}
