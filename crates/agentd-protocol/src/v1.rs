use agentd_core::{AgentLifecycleState, AgentProfile, AuditEvent};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentRequest {
    pub profile: AgentProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAgentResponse {
    pub agent_id: Uuid,
    pub profile: AgentProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAgentResponse {
    pub profile: AgentProfile,
    pub audit_events: Vec<AuditEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsResponse {
    pub agents: Vec<AgentProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAgentResponse {
    pub success: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum A2ATaskState {
    Submitted,
    Working,
    InputRequired,
    Completed,
    Failed,
    Canceled,
}

impl A2ATaskState {
    pub fn can_transition_to(self, next: Self) -> bool {
        match (self, next) {
            (Self::Submitted, Self::Working) => true,
            (Self::Working, Self::InputRequired) => true,
            (Self::Working, Self::Completed) => true,
            (Self::Working, Self::Failed) => true,
            (Self::Working, Self::Canceled) => true,
            (Self::InputRequired, Self::Working) => true,
            (Self::InputRequired, Self::Failed) => true,
            (Self::InputRequired, Self::Canceled) => true,
            _ => false,
        }
    }

    pub fn to_agent_lifecycle_state(self) -> AgentLifecycleState {
        match self {
            Self::Submitted => AgentLifecycleState::Creating,
            Self::Working | Self::InputRequired => AgentLifecycleState::Ready,
            Self::Completed => AgentLifecycleState::Ready,
            Self::Failed | Self::Canceled => AgentLifecycleState::Failed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ATask {
    pub id: Uuid,
    pub agent_id: Option<Uuid>,
    pub state: A2ATaskState,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateA2ATaskRequest {
    #[serde(default)]
    pub agent_id: Option<Uuid>,
    #[serde(default)]
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateA2ATaskResponse {
    pub task: A2ATask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetA2ATaskResponse {
    pub task: A2ATask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct A2ATaskEvent {
    pub task_id: Uuid,
    pub state: A2ATaskState,
    pub lifecycle_state: AgentLifecycleState,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub payload: Value,
}
