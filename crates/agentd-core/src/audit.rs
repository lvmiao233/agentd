use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    #[serde(rename = "event_id", alias = "id")]
    pub id: Uuid,
    pub agent_id: Uuid,
    pub event_type: EventType,
    pub timestamp: DateTime<Utc>,
    pub severity: EventSeverity,
    pub payload: EventPayload,
    pub result: EventResult,
    pub trace_id: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditContext {
    pub trace_id: String,
    pub session_id: String,
    pub severity: EventSeverity,
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
pub struct PolicyReplayReference {
    pub tool: String,
    pub input: serde_json::Value,
    pub reason: String,
    pub trace_id: String,
}

impl PolicyReplayReference {
    pub fn new(
        tool: impl Into<String>,
        input: serde_json::Value,
        reason: impl Into<String>,
        trace_id: impl Into<String>,
    ) -> Self {
        Self {
            tool: tool.into(),
            input,
            reason: reason.into(),
            trace_id: trace_id.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EventResult {
    Success,
    Failure,
    Pending,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventSeverity {
    Info,
    Warning,
    Error,
}

impl EventSeverity {
    pub fn from_result(result: &EventResult) -> Self {
        match result {
            EventResult::Success => EventSeverity::Info,
            EventResult::Pending | EventResult::Cancelled => EventSeverity::Warning,
            EventResult::Failure => EventSeverity::Error,
        }
    }
}

impl AuditContext {
    pub fn new(
        trace_id: impl Into<String>,
        session_id: impl Into<String>,
        severity: EventSeverity,
    ) -> Self {
        Self {
            trace_id: trace_id.into(),
            session_id: session_id.into(),
            severity,
        }
    }

    pub fn with_severity(&self, severity: EventSeverity) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            session_id: self.session_id.clone(),
            severity,
        }
    }
}

impl Default for AuditContext {
    fn default() -> Self {
        let correlation_id = Uuid::new_v4();
        Self {
            trace_id: format!("trace-{correlation_id}"),
            session_id: format!("session-{correlation_id}"),
            severity: EventSeverity::Info,
        }
    }
}

impl AuditEvent {
    pub fn new(
        agent_id: Uuid,
        event_type: EventType,
        payload: EventPayload,
        result: EventResult,
    ) -> Self {
        let context = AuditContext::default().with_severity(EventSeverity::from_result(&result));
        Self::new_with_context(agent_id, event_type, payload, result, context)
    }

    pub fn new_with_context(
        agent_id: Uuid,
        event_type: EventType,
        payload: EventPayload,
        result: EventResult,
        context: AuditContext,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            agent_id,
            event_type,
            timestamp: Utc::now(),
            severity: context.severity,
            payload,
            result,
            trace_id: context.trace_id,
            session_id: context.session_id,
        }
    }
}
