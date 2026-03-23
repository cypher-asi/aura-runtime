//! # aura-runtime
//!
//! Turn processor and process manager for Aura.
//!
//! This crate provides:
//! - Multi-step turn processor for agentic loops (Spec-02)
//! - Process manager for async command execution
//!
//! ## Turn Processor
//!
//! The `TurnProcessor` implements a Claude Code-like agentic loop:
//!
//! ```text
//! loop {
//!     1. Build context (deterministic)
//!     2. Call ModelProvider.complete()
//!     3. Record assistant response
//!     4. If tool_use: authorize → execute → inject tool_result
//!     5. If end_turn: finalize
//! }
//! ```
//!
//! ## Process Manager
//!
//! The `ProcessManager` tracks long-running processes that exceed the sync
//! threshold and creates completion transactions when they finish.

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

pub mod process_manager;
mod turn_processor;

pub use process_manager::{ProcessManager, ProcessManagerConfig, ProcessOutput, RunningProcess};
pub use turn_processor::{
    ExecutedToolCall, ModelCallDelegate, StepConfig, StepResult, StreamCallback,
    StreamCallbackEvent, ToolCache, TurnConfig, TurnEntry, TurnProcessor, TurnResult,
};
