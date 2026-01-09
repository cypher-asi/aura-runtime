//! # aura-reasoner
//!
//! Provider-agnostic model interface for the Aura Swarm.
//!
//! This crate provides:
//! - Normalized conversation types (`Message`, `ContentBlock`, `ToolDefinition`)
//! - `ModelProvider` trait for provider-agnostic completions
//! - `AnthropicProvider` implementation using `anthropic-sdk`
//! - `MockProvider` for testing
//!
//! ## Architecture
//!
//! The reasoner abstraction separates AURA's deterministic kernel from
//! probabilistic model calls. All model interactions go through the
//! `ModelProvider` trait, enabling:
//!
//! - Provider switching (Anthropic, `OpenAI`, local models)
//! - Recording/replay of model outputs for determinism
//! - Testing with mock providers

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

mod anthropic;
mod client;
mod mock;
mod request;
pub mod types;

pub use anthropic::{AnthropicConfig, AnthropicProvider};
pub use client::HttpReasoner;
pub use mock::{MockProvider, MockReasoner, MockResponse};
pub use request::{ProposeLimits, ProposeRequest, RecordSummary};
pub use types::{
    ContentBlock, Message, ModelRequest, ModelResponse, ProviderTrace, Role, StopReason,
    ToolChoice, ToolDefinition, ToolResultContent, Usage,
};

use async_trait::async_trait;
use aura_core::ProposalSet;

// ============================================================================
// ModelProvider Trait (New in Spec-02)
// ============================================================================

/// Provider-agnostic interface for model completions.
///
/// This trait abstracts over different LLM providers (Anthropic, `OpenAI`, etc.)
/// allowing the kernel to work with any provider that implements this interface.
///
/// # Recording and Replay
///
/// During normal operation, the kernel calls `complete()` and records the
/// `ModelResponse`. During replay, the kernel loads the recorded response
/// instead of calling `complete()`, ensuring deterministic state reconstruction.
///
/// # Tool Use
///
/// When the model wants to use tools, it returns with `StopReason::ToolUse`.
/// The kernel extracts tool calls from the response message, executes them,
/// and continues the conversation with tool results.
#[async_trait]
pub trait ModelProvider: Send + Sync {
    /// Provider name (e.g., "anthropic", "openai", "mock").
    fn name(&self) -> &'static str;

    /// Complete a conversation, potentially with tool use.
    ///
    /// # Arguments
    ///
    /// * `request` - The model request containing system prompt, messages, and tools
    ///
    /// # Returns
    ///
    /// * `Ok(ModelResponse)` - The model's response with stop reason and content
    /// * `Err(_)` - If the request fails (network, auth, rate limit, etc.)
    ///
    /// # Errors
    ///
    /// Returns error if the provider request fails.
    async fn complete(&self, request: ModelRequest) -> anyhow::Result<ModelResponse>;

    /// Check if the provider is available.
    ///
    /// This can be used for health checks and load balancing.
    async fn health_check(&self) -> bool;
}

// ============================================================================
// Legacy Reasoner Trait (Spec-01 Compatibility)
// ============================================================================

/// Reasoner trait for generating proposals.
///
/// **Note**: This is the legacy interface from Spec-01. New code should use
/// `ModelProvider` instead. This trait is kept for backwards compatibility.
///
/// The reasoner is the probabilistic component that suggests actions
/// based on context. The kernel records proposals and makes final decisions.
#[async_trait]
pub trait Reasoner: Send + Sync {
    /// Generate proposals based on context.
    ///
    /// # Errors
    /// Returns error if the reasoner fails or times out.
    async fn propose(&self, request: ProposeRequest) -> anyhow::Result<ProposalSet>;

    /// Check if the reasoner is available.
    async fn health_check(&self) -> bool;
}

// ============================================================================
// Configuration
// ============================================================================

/// Reasoner configuration (legacy).
#[derive(Debug, Clone)]
pub struct ReasonerConfig {
    /// Gateway URL
    pub gateway_url: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retries
    pub max_retries: u32,
}

impl Default for ReasonerConfig {
    fn default() -> Self {
        Self {
            gateway_url: "http://localhost:3000".to_string(),
            timeout_ms: 30_000,
            max_retries: 2,
        }
    }
}
