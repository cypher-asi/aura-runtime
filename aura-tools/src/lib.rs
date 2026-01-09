//! # aura-tools
//!
//! Tool executor and registry for filesystem and command operations.
//!
//! This crate provides:
//! - `ToolRegistry` trait and `DefaultToolRegistry` implementation
//! - `ToolExecutor` for executing tool calls
//! - Sandboxed filesystem and command operations
//!
//! ## Security
//!
//! All filesystem operations are sandboxed to prevent path traversal attacks.
//! Command execution is disabled by default and requires explicit allowlisting.

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

mod error;
mod executor;
mod fs_tools;
pub mod registry;
mod sandbox;

pub use error::ToolError;
pub use executor::ToolExecutor;
pub use registry::{DefaultToolRegistry, ToolRegistry};
pub use sandbox::Sandbox;

/// Tool configuration.
#[derive(Debug, Clone)]
pub struct ToolConfig {
    /// Enable filesystem tools
    pub enable_fs: bool,
    /// Enable command execution
    pub enable_commands: bool,
    /// Allowed commands (empty = all allowed if commands enabled)
    pub command_allowlist: Vec<String>,
    /// Maximum read bytes
    pub max_read_bytes: usize,
    /// Maximum command timeout in milliseconds
    pub max_command_timeout_ms: u64,
}

impl Default for ToolConfig {
    fn default() -> Self {
        Self {
            enable_fs: true,
            enable_commands: false, // Disabled by default for safety
            command_allowlist: vec![],
            max_read_bytes: 5 * 1024 * 1024, // 5MB
            max_command_timeout_ms: 10_000,  // 10s
        }
    }
}
