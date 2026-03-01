use serde::{Deserialize, Serialize};
use uuid::Uuid;
use agentd_core::{AgentProfile, AuditEvent};

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
