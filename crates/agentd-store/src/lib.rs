use agentd_core::{AgentProfile, AuditEvent, AgentError};
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait AgentStore: Send + Sync {
    async fn create_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError>;
    async fn get_agent(&self, id: Uuid) -> Result<AgentProfile, AgentError>;
    async fn list_agents(&self) -> Result<Vec<AgentProfile>, AgentError>;
    async fn update_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError>;
    async fn delete_agent(&self, id: Uuid) -> Result<(), AgentError>;
}

#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append_event(&self, event: AuditEvent) -> Result<(), AgentError>;
    async fn get_events(&self, agent_id: Uuid) -> Result<Vec<AuditEvent>, AgentError>;
}
