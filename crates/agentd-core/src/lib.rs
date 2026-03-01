//! Core types for agentd runtime.
//!
//! This crate provides the foundational types used across the agentd system:
//! - AgentProfile: Declarative agent identity and configuration
//! - AuditEvent: Structured event recording for audit trails
//! - AgentError: Central error type for the runtime

pub mod error;
pub mod profile;
pub mod audit;

pub use error::AgentError;
pub use profile::AgentProfile;
pub use audit::AuditEvent;
