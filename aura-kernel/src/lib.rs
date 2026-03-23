//! # aura-kernel
//!
//! Deterministic kernel for Aura.
//!
//! This crate provides:
//! - Single-step kernel processing (Spec-01 legacy)
//! - Policy engine for authorization
//! - Context building for model requests
//!
//! ## Architecture
//!
//! The kernel is the deterministic core of AURA. It:
//! 1. Builds context from the record window
//! 2. Calls the model provider for completions
//! 3. Applies policy to authorize actions
//! 4. Executes actions via the executor router
//! 5. Records all inputs/outputs for replay
//!
//! The turn processor and process manager have been extracted to `aura-runtime`.

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

mod context;
mod kernel;
mod policy;

pub use context::{Context, ContextBuilder};
pub use kernel::{Kernel, KernelConfig, ProcessResult};
pub use policy::{default_tool_permission, PermissionLevel, Policy, PolicyConfig, PolicyResult};

// Re-export ToolResultContent for convenience
pub use aura_reasoner::ToolResultContent;
