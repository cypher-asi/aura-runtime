//! WebSocket session state and lifecycle.
//!
//! Each WebSocket connection maps to a `Session` that maintains conversation
//! state, tool configuration, and token accounting across turns.

use crate::protocol::*;
use aura_core::ExternalToolDefinition;
use aura_executor::ExecutorRouter;
use aura_kernel::{StreamCallback, StreamCallbackEvent, TurnConfig};
use aura_reasoner::{
    Message, ModelProvider, ModelRequest, ModelResponse, StreamEventStream, ToolDefinition,
};
use aura_tools::{DefaultToolRegistry, ToolConfig, ToolExecutor, ToolRegistry};
use async_trait::async_trait;
use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures_util::{SinkExt, StreamExt};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// ============================================================================
// DynProvider — type-erased ModelProvider wrapper
// ============================================================================

/// Wraps `Arc<dyn ModelProvider + Send + Sync>` so it can be used as
/// a concrete `P` type parameter in `TurnProcessor<P, S, R>`.
pub(crate) struct DynProvider(pub Arc<dyn ModelProvider + Send + Sync>);

#[async_trait]
impl ModelProvider for DynProvider {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    async fn complete(&self, request: ModelRequest) -> anyhow::Result<ModelResponse> {
        self.0.complete(request).await
    }

    async fn complete_streaming(&self, request: ModelRequest) -> anyhow::Result<StreamEventStream> {
        self.0.complete_streaming(request).await
    }

    async fn health_check(&self) -> bool {
        self.0.health_check().await
    }
}

// ============================================================================
// Session
// ============================================================================

/// Per-connection session state.
pub struct Session {
    /// Unique session identifier.
    pub session_id: String,
    /// System prompt for the model.
    pub system_prompt: String,
    /// Model identifier.
    pub model: String,
    /// Max tokens per response.
    pub max_tokens: u32,
    /// Sampling temperature.
    pub temperature: Option<f32>,
    /// Maximum agentic steps per turn.
    pub max_turns: u32,
    /// External tools registered for this session.
    pub external_tools: Vec<ExternalToolDefinition>,
    /// Conversation history (accumulated across turns).
    pub messages: Vec<Message>,
    /// Cumulative input tokens across all turns.
    pub cumulative_input_tokens: u64,
    /// Cumulative output tokens across all turns.
    pub cumulative_output_tokens: u64,
    /// Workspace directory for this session.
    pub workspace: PathBuf,
    /// Whether session_init has been received.
    pub initialized: bool,
    /// Available tool definitions (builtin + external).
    pub tool_definitions: Vec<ToolDefinition>,
}

impl Session {
    /// Create a new uninitialized session with defaults.
    fn new(default_workspace: PathBuf) -> Self {
        Self {
            session_id: Uuid::new_v4().to_string(),
            system_prompt: String::new(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 16384,
            temperature: None,
            max_turns: 25,
            external_tools: Vec::new(),
            messages: Vec::new(),
            cumulative_input_tokens: 0,
            cumulative_output_tokens: 0,
            workspace: default_workspace,
            initialized: false,
            tool_definitions: Vec::new(),
        }
    }

    /// Apply a `session_init` message to configure this session.
    fn apply_init(&mut self, init: SessionInit) {
        if let Some(prompt) = init.system_prompt {
            self.system_prompt = prompt;
        }
        if let Some(model) = init.model {
            self.model = model;
        }
        if let Some(max_tokens) = init.max_tokens {
            self.max_tokens = max_tokens;
        }
        if let Some(temperature) = init.temperature {
            self.temperature = Some(temperature);
        }
        if let Some(max_turns) = init.max_turns {
            self.max_turns = max_turns;
        }
        if let Some(tools) = init.external_tools {
            self.external_tools = tools;
        }
        if let Some(workspace) = init.workspace {
            self.workspace = PathBuf::from(workspace);
        }
        self.initialized = true;
    }

    /// Build a `TurnConfig` from session state.
    fn turn_config(&self) -> TurnConfig {
        TurnConfig {
            max_steps: self.max_turns,
            model: self.model.clone(),
            system_prompt: if self.system_prompt.is_empty() {
                TurnConfig::default().system_prompt
            } else {
                self.system_prompt.clone()
            },
            max_tokens: self.max_tokens,
            temperature: self.temperature,
            workspace_base: self.workspace.clone(),
            ..TurnConfig::default()
        }
    }
}

// ============================================================================
// WebSocket Handler
// ============================================================================

/// Configuration passed to the WebSocket handler from the router state.
#[derive(Clone)]
pub struct WsContext {
    /// Default workspace base path.
    pub workspace_base: PathBuf,
    /// Shared model provider (type-erased).
    pub provider: Arc<dyn ModelProvider + Send + Sync>,
    /// Tool configuration (fs/cmd permissions).
    pub tool_config: ToolConfig,
}

/// Handle a WebSocket connection through its full lifecycle.
///
/// Protocol:
/// 1. Client sends `session_init` as the first message.
/// 2. Server responds with `session_ready`.
/// 3. Client sends `user_message` events, server streams responses.
pub async fn handle_ws_connection(socket: WebSocket, ctx: WsContext) {
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel::<OutboundMessage>();

    // Spawn a task that forwards outbound messages to the WebSocket sink.
    let send_task = tokio::spawn(async move {
        while let Some(msg) = outbound_rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(json) => {
                    if ws_tx.send(WsMessage::Text(json.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!(error = %e, "Failed to serialize outbound message");
                }
            }
        }
    });

    let mut session = Session::new(ctx.workspace_base.clone());
    info!(session_id = %session.session_id, "WebSocket connection opened");

    // Message receive loop.
    while let Some(msg_result) = ws_rx.next().await {
        let raw = match msg_result {
            Ok(WsMessage::Text(text)) => text.to_string(),
            Ok(WsMessage::Close(_)) => {
                debug!(session_id = %session.session_id, "Client sent close frame");
                break;
            }
            Ok(WsMessage::Ping(_) | WsMessage::Pong(_)) => continue,
            Ok(_) => continue,
            Err(e) => {
                warn!(session_id = %session.session_id, error = %e, "WebSocket receive error");
                break;
            }
        };

        let inbound: InboundMessage = match serde_json::from_str(&raw) {
            Ok(msg) => msg,
            Err(e) => {
                let _ = outbound_tx.send(OutboundMessage::Error(ErrorMsg {
                    code: "parse_error".into(),
                    message: format!("Invalid message: {e}"),
                    recoverable: true,
                }));
                continue;
            }
        };

        match inbound {
            InboundMessage::SessionInit(init) => {
                handle_session_init(&mut session, init, &outbound_tx);
            }
            InboundMessage::UserMessage(msg) => {
                handle_user_message(&mut session, msg, &outbound_tx, &ctx).await;
            }
            InboundMessage::Cancel => {
                debug!(session_id = %session.session_id, "Cancel requested (not yet implemented)");
            }
            InboundMessage::ApprovalResponse(resp) => {
                debug!(
                    session_id = %session.session_id,
                    tool_use_id = %resp.tool_use_id,
                    approved = resp.approved,
                    "Approval response received (not yet implemented)"
                );
            }
        }
    }

    info!(session_id = %session.session_id, "WebSocket connection closed");
    drop(outbound_tx);
    let _ = send_task.await;
}

/// Handle a `session_init` message.
fn handle_session_init(
    session: &mut Session,
    init: SessionInit,
    outbound_tx: &mpsc::UnboundedSender<OutboundMessage>,
) {
    if session.initialized {
        let _ = outbound_tx.send(OutboundMessage::Error(ErrorMsg {
            code: "already_initialized".into(),
            message: "Session has already been initialized".into(),
            recoverable: true,
        }));
        return;
    }

    session.apply_init(init);

    // Build tool list from builtins.
    let builtin_tools = DefaultToolRegistry::new();
    session.tool_definitions = builtin_tools.list();

    // Add external tool definitions.
    for ext in &session.external_tools {
        session.tool_definitions.push(ToolDefinition::new(
            &ext.name,
            &ext.description,
            ext.input_schema.clone(),
        ));
    }

    let tools: Vec<ToolInfo> = session
        .tool_definitions
        .iter()
        .cloned()
        .map(ToolInfo::from)
        .collect();

    info!(
        session_id = %session.session_id,
        model = %session.model,
        tool_count = tools.len(),
        "Session initialized"
    );

    let _ = outbound_tx.send(OutboundMessage::SessionReady(SessionReady {
        session_id: session.session_id.clone(),
        tools,
    }));
}

/// Handle a `user_message` by running the agentic turn loop.
async fn handle_user_message(
    session: &mut Session,
    msg: UserMessage,
    outbound_tx: &mpsc::UnboundedSender<OutboundMessage>,
    ctx: &WsContext,
) {
    if !session.initialized {
        let _ = outbound_tx.send(OutboundMessage::Error(ErrorMsg {
            code: "not_initialized".into(),
            message: "Send session_init before user_message".into(),
            recoverable: true,
        }));
        return;
    }

    let message_id = Uuid::new_v4().to_string();

    let _ = outbound_tx.send(OutboundMessage::AssistantMessageStart(
        AssistantMessageStart {
            message_id: message_id.clone(),
        },
    ));

    // Build per-turn tool executor with external tools.
    let mut tool_executor = ToolExecutor::new(ctx.tool_config.clone());
    for ext in &session.external_tools {
        tool_executor.register_external(aura_tools::ExternalToolDefinition {
            name: ext.name.clone(),
            description: ext.description.clone(),
            input_schema: ext.input_schema.clone(),
            callback_url: ext.callback_url.clone(),
        });
    }

    let mut executor_router = ExecutorRouter::new();
    executor_router.add_executor(Arc::new(tool_executor));

    // Build per-turn tool registry with external tools.
    let mut tool_registry = DefaultToolRegistry::new();
    for ext in &session.external_tools {
        tool_registry.register(ToolDefinition::new(
            &ext.name,
            &ext.description,
            ext.input_schema.clone(),
        ));
    }

    // Set up streaming callback → channel bridge.
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<StreamCallbackEvent>();
    let outbound_for_stream = outbound_tx.clone();

    let stream_forward_task = tokio::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            let out = match event {
                StreamCallbackEvent::TextDelta(text) => {
                    OutboundMessage::TextDelta(TextDelta { text })
                }
                StreamCallbackEvent::ThinkingDelta(thinking) => {
                    OutboundMessage::ThinkingDelta(ThinkingDelta { thinking })
                }
                StreamCallbackEvent::ThinkingComplete => continue,
                StreamCallbackEvent::ToolStart { id, name } => {
                    OutboundMessage::ToolUseStart(ToolUseStart { id, name })
                }
                StreamCallbackEvent::ToolComplete {
                    name,
                    result,
                    is_error,
                    ..
                } => OutboundMessage::ToolResult(ToolResultMsg {
                    name,
                    result,
                    is_error,
                }),
                StreamCallbackEvent::StepComplete => continue,
            };
            if outbound_for_stream.send(out).is_err() {
                break;
            }
        }
    });

    let callback: StreamCallback = Box::new(move |event| {
        let _ = event_tx.send(event);
    });

    // Create the TurnProcessor for this turn.
    let provider = Arc::new(DynProvider(ctx.provider.clone()));
    // Use a temporary in-memory store placeholder via a temp RocksDB.
    // The session manages its own message history, so the store is unused
    // when calling process_turn_with_messages (Phase 2c).
    // For now, create a simple turn by building messages and calling the model directly.

    let turn_config = session.turn_config();

    // Run the turn loop directly (without TurnProcessor) since we manage
    // messages ourselves. This avoids needing a Store instance.
    let result = run_session_turn(
        provider,
        executor_router,
        &tool_registry,
        &turn_config,
        &msg.content,
        &mut session.messages,
        callback,
    )
    .await;

    // Wait for streaming events to flush.
    let _ = stream_forward_task.await;

    match result {
        Ok(turn_result) => {
            let input_tokens = u64::from(turn_result.total_input_tokens);
            let output_tokens = u64::from(turn_result.total_output_tokens);
            session.cumulative_input_tokens += input_tokens;
            session.cumulative_output_tokens += output_tokens;

            let stop_reason = if turn_result.had_failures {
                "end_turn_with_errors"
            } else {
                "end_turn"
            };

            let _ = outbound_tx.send(OutboundMessage::AssistantMessageEnd(
                AssistantMessageEnd {
                    message_id,
                    stop_reason: stop_reason.into(),
                    usage: SessionUsage {
                        input_tokens,
                        output_tokens,
                        cumulative_input_tokens: session.cumulative_input_tokens,
                        cumulative_output_tokens: session.cumulative_output_tokens,
                    },
                },
            ));
        }
        Err(e) => {
            error!(session_id = %session.session_id, error = %e, "Turn processing failed");
            let _ = outbound_tx.send(OutboundMessage::Error(ErrorMsg {
                code: "turn_error".into(),
                message: format!("Turn processing failed: {e}"),
                recoverable: true,
            }));
        }
    }
}

// ============================================================================
// Session Turn Loop
// ============================================================================

/// Lightweight turn result returned by the session turn loop.
struct SessionTurnResult {
    total_input_tokens: u32,
    total_output_tokens: u32,
    had_failures: bool,
}

/// Run an agentic turn loop for a WebSocket session.
///
/// This is a self-contained loop that:
/// 1. Appends the user message to the conversation history
/// 2. Calls the model (streaming)
/// 3. If tool_use: executes tools, adds results, continues
/// 4. If end_turn: returns
///
/// Message history is maintained in-place on `messages` so multi-turn
/// conversations accumulate naturally.
#[allow(clippy::too_many_arguments)]
async fn run_session_turn(
    provider: Arc<DynProvider>,
    executor_router: ExecutorRouter,
    tool_registry: &DefaultToolRegistry,
    config: &TurnConfig,
    user_content: &str,
    messages: &mut Vec<Message>,
    callback: StreamCallback,
) -> anyhow::Result<SessionTurnResult> {
    use aura_core::{Action, AgentId, ToolCall};
    use aura_executor::ExecuteContext;
    use aura_reasoner::{ContentBlock, StopReason, ToolResultContent};

    let agent_id = AgentId::generate();
    let workspace = config.workspace_base.clone();

    // Ensure workspace directory exists.
    if let Err(e) = tokio::fs::create_dir_all(&workspace).await {
        warn!(error = %e, "Failed to create workspace directory");
    }

    // Append user message.
    messages.push(Message::user(user_content));

    let tools = tool_registry.list();
    let callback = Arc::new(callback);

    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;
    let mut had_failures = false;

    for step in 0..config.max_steps {
        debug!(step, messages = messages.len(), "Session turn step");

        let request = ModelRequest::builder(&config.model, &config.system_prompt)
            .messages(messages.clone())
            .tools(tools.clone())
            .max_tokens(config.max_tokens)
            .temperature(config.temperature.unwrap_or(0.7))
            .build();

        // Stream the model response.
        let response = stream_model_response(&provider, request, &callback).await?;

        total_input_tokens += response.usage.input_tokens;
        total_output_tokens += response.usage.output_tokens;

        messages.push(response.message.clone());

        match response.stop_reason {
            StopReason::EndTurn | StopReason::MaxTokens | StopReason::StopSequence => {
                callback(StreamCallbackEvent::StepComplete);
                break;
            }
            StopReason::ToolUse => {
                // Execute tool calls.
                let mut tool_results: Vec<(String, ToolResultContent, bool)> = Vec::new();

                for block in &response.message.content {
                    if let ContentBlock::ToolUse { id, name, input } = block {
                        let tool_call = ToolCall::new(name.clone(), input.clone());
                        let action = Action::delegate_tool(&tool_call);
                        let exec_ctx =
                            ExecuteContext::new(agent_id, action.action_id, workspace.clone());

                        let effect = executor_router.execute(&exec_ctx, &action).await;

                        let (content, is_error) =
                            if effect.status == aura_core::EffectStatus::Committed {
                                if let Ok(tr) =
                                    serde_json::from_slice::<aura_core::ToolResult>(&effect.payload)
                                {
                                    let text = if tr.stdout.is_empty() {
                                        "Success (no output)".to_string()
                                    } else {
                                        String::from_utf8_lossy(&tr.stdout).to_string()
                                    };
                                    (ToolResultContent::text(text), !tr.ok)
                                } else {
                                    (ToolResultContent::text("Tool executed successfully"), false)
                                }
                            } else {
                                let err = serde_json::from_slice::<aura_core::ToolResult>(
                                    &effect.payload,
                                )
                                .map(|tr| String::from_utf8_lossy(&tr.stderr).to_string())
                                .unwrap_or_else(|_| "Tool execution failed".into());
                                (ToolResultContent::text(err), true)
                            };

                        if is_error {
                            had_failures = true;
                        }

                        let result_text = match &content {
                            ToolResultContent::Text(s) => s.clone(),
                            ToolResultContent::Json(v) => {
                                serde_json::to_string(v).unwrap_or_default()
                            }
                        };

                        callback(StreamCallbackEvent::ToolComplete {
                            name: name.clone(),
                            args: input.clone(),
                            result: result_text,
                            is_error,
                        });

                        tool_results.push((id.clone(), content, is_error));
                    }
                }

                if !tool_results.is_empty() {
                    messages.push(Message::tool_results(tool_results));
                }
            }
        }
    }

    Ok(SessionTurnResult {
        total_input_tokens,
        total_output_tokens,
        had_failures,
    })
}

/// Call the model with streaming, forwarding events through the callback.
async fn stream_model_response(
    provider: &DynProvider,
    request: ModelRequest,
    callback: &Arc<StreamCallback>,
) -> anyhow::Result<ModelResponse> {
    use aura_reasoner::{StreamAccumulator, StreamEvent};
    use futures_util::StreamExt;

    let mut stream = provider.complete_streaming(request).await?;
    let mut accumulator = StreamAccumulator::new();
    let input_tokens = 0u32;
    let start = std::time::Instant::now();
    let mut in_thinking_block = false;

    while let Some(event_result) = stream.next().await {
        match event_result {
            Ok(event) => {
                match &event {
                    StreamEvent::ContentBlockStart {
                        content_type:
                            aura_reasoner::StreamContentType::Thinking,
                        ..
                    } => {
                        in_thinking_block = true;
                    }
                    StreamEvent::ContentBlockStart {
                        content_type:
                            aura_reasoner::StreamContentType::ToolUse { id, name },
                        ..
                    } => {
                        if in_thinking_block {
                            callback(StreamCallbackEvent::ThinkingComplete);
                            in_thinking_block = false;
                        }
                        callback(StreamCallbackEvent::ToolStart {
                            id: id.clone(),
                            name: name.clone(),
                        });
                    }
                    StreamEvent::ContentBlockStart {
                        content_type: aura_reasoner::StreamContentType::Text,
                        ..
                    }
                    | StreamEvent::ContentBlockStop { .. } => {
                        if in_thinking_block {
                            callback(StreamCallbackEvent::ThinkingComplete);
                            in_thinking_block = false;
                        }
                    }
                    StreamEvent::ThinkingDelta { thinking } => {
                        callback(StreamCallbackEvent::ThinkingDelta(
                            thinking.clone(),
                        ));
                    }
                    StreamEvent::TextDelta { text } => {
                        callback(StreamCallbackEvent::TextDelta(text.clone()));
                    }
                    StreamEvent::Error { message } => {
                        anyhow::bail!("Stream error: {message}");
                    }
                    _ => {}
                }

                accumulator.process(&event);

                if matches!(event, StreamEvent::MessageStop) {
                    break;
                }
            }
            Err(e) => {
                anyhow::bail!("Stream error: {e}");
            }
        }
    }

    let latency_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    accumulator.into_response(input_tokens, latency_ms)
}
