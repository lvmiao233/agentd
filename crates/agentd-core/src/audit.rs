use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub agent_id: Uuid,
    pub event_type: EventType,
    pub timestamp: DateTime<Utc>,
    pub payload: EventPayload,
    pub result: EventResult,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventType {
    AgentCreated,
    AgentStarted,
    AgentStopped,
    ToolInvoked,
    ToolApproved,
    ToolDenied,
    BudgetExceeded,
    PermissionDenied,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventPayload {
    pub tool_name: Option<String>,
    pub message: Option<String>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventResult {
    Success,
    Failure,
    Pending,
    Cancelled,
}

impl AuditEvent {
    pub fn new(
        agent_id: Uuid,
        event_type: EventType,
        payload: EventPayload,
        result: EventResult,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            event_type,
            timestamp: Utc::now(),
            payload,
            result,
        }
    }
}
