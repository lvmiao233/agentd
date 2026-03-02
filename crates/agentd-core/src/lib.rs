//! Core types for agentd runtime.
//!
//! This crate provides the foundational types used across the agentd system:
//! - AgentProfile: Declarative agent identity and configuration
//! - AuditEvent: Structured event recording for audit trails
//! - AgentError: Central error type for the runtime

pub mod audit;
pub mod error;
pub mod profile;

pub use audit::AuditEvent;
pub use error::AgentError;
pub use profile::{AgentLifecycleState, AgentProfile};
