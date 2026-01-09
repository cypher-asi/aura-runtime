//! # aura-core
//!
//! Core types, identifiers, schemas, and serialization for the Aura Swarm.
//!
//! This crate provides:
//! - Strongly-typed identifiers (`AgentId`, `TxId`, `ActionId`)
//! - Domain types (`Transaction`, `Action`, `Effect`, `RecordEntry`)
//! - Error types
//! - Hashing utilities

#![forbid(unsafe_code)]
#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

pub mod error;
pub mod hash;
pub mod ids;
pub mod types;

pub use error::{AuraError, Result};
pub use ids::{ActionId, AgentId, TxId};
pub use types::{
    Action, ActionKind, Decision, Effect, EffectKind, EffectStatus, Identity, Proposal,
    ProposalSet, RecordEntry, RejectedProposal, ToolCall, ToolResult, Trace, Transaction,
    TransactionKind,
};
