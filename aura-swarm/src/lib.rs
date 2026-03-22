//! # aura-swarm
//!
//! Swarm runtime for Aura.
//!
//! Provides:
//! - HTTP router for transaction submission
//! - Scheduler for agent processing
//! - Per-agent worker loop with single-writer guarantee

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::doc_markdown,
    clippy::must_use_candidate,
    clippy::match_same_arms,
    clippy::too_many_lines,
    clippy::single_match,
    clippy::single_match_else,
    clippy::option_if_let_else,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::unnecessary_map_or
)]

mod config;
pub mod protocol;
mod router;
mod scheduler;
pub mod session;
mod swarm;
mod worker;

pub use config::SwarmConfig;
pub use swarm::Swarm;
