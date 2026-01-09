//! Normalized provider-agnostic conversation types.
//!
//! These are AURA canonical types for model interactions.
//! Every provider adapter maps to/from these types.

use serde::{Deserialize, Serialize};

// ============================================================================
// Role and Content Types
// ============================================================================

/// Role in conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// User message
    User,
    /// Assistant (model) message
    Assistant,
}

/// Content block in a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text content
    Text { text: String },

    /// Model requesting tool use (assistant only)
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },

    /// Result of tool execution (user only, in response to `tool_use`)
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
        is_error: bool,
    },
}

impl ContentBlock {
    /// Create a text content block.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text { text: text.into() }
    }

    /// Create a tool use content block.
    #[must_use]
    pub fn tool_use(
        id: impl Into<String>,
        name: impl Into<String>,
        input: serde_json::Value,
    ) -> Self {
        Self::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
        }
    }

    /// Create a tool result content block.
    #[must_use]
    pub fn tool_result(
        tool_use_id: impl Into<String>,
        content: ToolResultContent,
        is_error: bool,
    ) -> Self {
        Self::ToolResult {
            tool_use_id: tool_use_id.into(),
            content,
            is_error,
        }
    }

    /// Get the text content if this is a text block.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text { text } => Some(text),
            _ => None,
        }
    }

    /// Check if this is a tool use block.
    #[must_use]
    pub const fn is_tool_use(&self) -> bool {
        matches!(self, Self::ToolUse { .. })
    }
}

// ============================================================================
// Tool Result Content
// ============================================================================

/// Content of a tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    /// Plain text result
    Text(String),
    /// Structured JSON result
    Json(serde_json::Value),
}

impl ToolResultContent {
    /// Create text content.
    #[must_use]
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    /// Create JSON content.
    #[must_use]
    pub const fn json(value: serde_json::Value) -> Self {
        Self::Json(value)
    }
}

impl From<String> for ToolResultContent {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<&str> for ToolResultContent {
    fn from(s: &str) -> Self {
        Self::Text(s.to_string())
    }
}

impl From<serde_json::Value> for ToolResultContent {
    fn from(v: serde_json::Value) -> Self {
        Self::Json(v)
    }
}

// ============================================================================
// Message
// ============================================================================

/// A message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: Role,
    /// Content blocks in the message
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Create a new message.
    #[must_use]
    pub const fn new(role: Role, content: Vec<ContentBlock>) -> Self {
        Self { role, content }
    }

    /// Create a user message with text content.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// Create an assistant message with text content.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// Create a user message with tool results.
    #[must_use]
    pub fn tool_results(results: Vec<(String, ToolResultContent, bool)>) -> Self {
        Self {
            role: Role::User,
            content: results
                .into_iter()
                .map(|(id, content, is_error)| ContentBlock::ToolResult {
                    tool_use_id: id,
                    content,
                    is_error,
                })
                .collect(),
        }
    }

    /// Get all text content concatenated.
    #[must_use]
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(ContentBlock::as_text)
            .collect::<Vec<_>>()
            .join("")
    }

    /// Get all tool use blocks.
    #[must_use]
    pub fn tool_uses(&self) -> Vec<&ContentBlock> {
        self.content.iter().filter(|b| b.is_tool_use()).collect()
    }

    /// Check if this message contains tool use.
    #[must_use]
    pub fn has_tool_use(&self) -> bool {
        self.content.iter().any(ContentBlock::is_tool_use)
    }
}

// ============================================================================
// Tool Definition
// ============================================================================

/// Tool definition for the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name (e.g., "fs.read", "search.code")
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

// ============================================================================
// Tool Choice
// ============================================================================

/// How the model should choose tools.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolChoice {
    /// Model decides whether to use tools
    #[default]
    Auto,
    /// Model should not use any tools
    None,
    /// Model must use a tool
    Required,
    /// Model must use the specified tool
    Tool { name: String },
}

impl ToolChoice {
    /// Create a tool choice for a specific tool.
    #[must_use]
    pub fn tool(name: impl Into<String>) -> Self {
        Self::Tool { name: name.into() }
    }
}

// ============================================================================
// Model Request
// ============================================================================

/// Request to the model.
#[derive(Debug, Clone)]
pub struct ModelRequest {
    /// Model identifier (e.g., "claude-opus-4-5-20251101")
    pub model: String,
    /// System prompt
    pub system: String,
    /// Conversation messages
    pub messages: Vec<Message>,
    /// Available tools
    pub tools: Vec<ToolDefinition>,
    /// Tool choice mode
    pub tool_choice: ToolChoice,
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Sampling temperature
    pub temperature: Option<f32>,
}

impl ModelRequest {
    /// Create a new model request builder.
    #[must_use]
    pub fn builder(model: impl Into<String>, system: impl Into<String>) -> ModelRequestBuilder {
        ModelRequestBuilder::new(model, system)
    }
}

/// Builder for `ModelRequest`.
pub struct ModelRequestBuilder {
    model: String,
    system: String,
    messages: Vec<Message>,
    tools: Vec<ToolDefinition>,
    tool_choice: ToolChoice,
    max_tokens: u32,
    temperature: Option<f32>,
}

impl ModelRequestBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new(model: impl Into<String>, system: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system: system.into(),
            messages: Vec::new(),
            tools: Vec::new(),
            tool_choice: ToolChoice::Auto,
            max_tokens: 4096,
            temperature: None,
        }
    }

    /// Set messages.
    #[must_use]
    pub fn messages(mut self, messages: Vec<Message>) -> Self {
        self.messages = messages;
        self
    }

    /// Add a message.
    #[must_use]
    pub fn message(mut self, message: Message) -> Self {
        self.messages.push(message);
        self
    }

    /// Set tools.
    #[must_use]
    pub fn tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    /// Set tool choice.
    #[must_use]
    pub fn tool_choice(mut self, choice: ToolChoice) -> Self {
        self.tool_choice = choice;
        self
    }

    /// Set max tokens.
    #[must_use]
    pub const fn max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = max;
        self
    }

    /// Set temperature.
    #[must_use]
    pub const fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Build the request.
    #[must_use]
    pub fn build(self) -> ModelRequest {
        ModelRequest {
            model: self.model,
            system: self.system,
            messages: self.messages,
            tools: self.tools,
            tool_choice: self.tool_choice,
            max_tokens: self.max_tokens,
            temperature: self.temperature,
        }
    }
}

// ============================================================================
// Stop Reason
// ============================================================================

/// Why the model stopped generating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Model completed its turn naturally
    #[default]
    EndTurn,
    /// Model wants to use tools
    ToolUse,
    /// Hit the `max_tokens` limit
    MaxTokens,
    /// Hit a stop sequence
    StopSequence,
}

// ============================================================================
// Usage
// ============================================================================

/// Token usage information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Usage {
    /// Number of input tokens
    pub input_tokens: u32,
    /// Number of output tokens
    pub output_tokens: u32,
}

impl Usage {
    /// Create new usage information.
    #[must_use]
    pub const fn new(input_tokens: u32, output_tokens: u32) -> Self {
        Self {
            input_tokens,
            output_tokens,
        }
    }

    /// Total tokens used.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.input_tokens + self.output_tokens
    }
}

// ============================================================================
// Provider Trace
// ============================================================================

/// Provider trace for debugging/logging.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderTrace {
    /// Request ID from the provider
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// Latency in milliseconds
    pub latency_ms: u64,
    /// Model that was used
    pub model: String,
}

impl ProviderTrace {
    /// Create a new provider trace.
    #[must_use]
    pub fn new(model: impl Into<String>, latency_ms: u64) -> Self {
        Self {
            request_id: None,
            latency_ms,
            model: model.into(),
        }
    }

    /// Set the request ID.
    #[must_use]
    pub fn with_request_id(mut self, id: impl Into<String>) -> Self {
        self.request_id = Some(id.into());
        self
    }
}

// ============================================================================
// Model Response
// ============================================================================

/// Response from the model.
#[derive(Debug, Clone)]
pub struct ModelResponse {
    /// Why the model stopped
    pub stop_reason: StopReason,
    /// The assistant message
    pub message: Message,
    /// Token usage
    pub usage: Usage,
    /// Provider trace information
    pub trace: ProviderTrace,
}

impl ModelResponse {
    /// Create a new model response.
    #[must_use]
    pub const fn new(
        stop_reason: StopReason,
        message: Message,
        usage: Usage,
        trace: ProviderTrace,
    ) -> Self {
        Self {
            stop_reason,
            message,
            usage,
            trace,
        }
    }

    /// Check if the model wants to use tools.
    #[must_use]
    pub fn wants_tool_use(&self) -> bool {
        self.stop_reason == StopReason::ToolUse
    }

    /// Check if the turn is complete.
    #[must_use]
    pub fn is_end_turn(&self) -> bool {
        self.stop_reason == StopReason::EndTurn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_user() {
        let msg = Message::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.text_content(), "Hello");
    }

    #[test]
    fn test_message_assistant() {
        let msg = Message::assistant("Hi there");
        assert_eq!(msg.role, Role::Assistant);
        assert_eq!(msg.text_content(), "Hi there");
    }

    #[test]
    fn test_message_tool_results() {
        let results = vec![
            ("id1".to_string(), ToolResultContent::text("result1"), false),
            ("id2".to_string(), ToolResultContent::text("error"), true),
        ];
        let msg = Message::tool_results(results);
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content.len(), 2);
    }

    #[test]
    fn test_tool_definition() {
        let tool = ToolDefinition::new(
            "fs.read",
            "Read a file",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        );
        assert_eq!(tool.name, "fs.read");
    }

    #[test]
    fn test_model_request_builder() {
        let request = ModelRequest::builder("claude-sonnet-4-20250514", "You are helpful")
            .message(Message::user("Hi"))
            .max_tokens(1000)
            .temperature(0.7)
            .build();

        assert_eq!(request.model, "claude-sonnet-4-20250514");
        assert_eq!(request.system, "You are helpful");
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.max_tokens, 1000);
        assert_eq!(request.temperature, Some(0.7));
    }

    #[test]
    fn test_content_block_serialization() {
        let text = ContentBlock::text("Hello");
        let json = serde_json::to_string(&text).unwrap();
        assert!(json.contains("\"type\":\"text\""));

        let tool_use =
            ContentBlock::tool_use("123", "fs.read", serde_json::json!({"path": "test"}));
        let json = serde_json::to_string(&tool_use).unwrap();
        assert!(json.contains("\"type\":\"tool_use\""));
    }

    #[test]
    fn test_usage() {
        let usage = Usage::new(100, 50);
        assert_eq!(usage.total(), 150);
    }
}
