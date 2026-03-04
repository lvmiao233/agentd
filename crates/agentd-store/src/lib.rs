use agentd_core::{AgentError, AgentLifecycleState, AgentProfile, AuditEvent};
use async_trait::async_trait;
use chrono::Utc;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub mod agent;
pub mod audit;
pub mod db;
pub mod one_api;
pub mod usage;

pub use agent::RegistryAgentEntry;
pub use one_api::OneApiMapping;
pub use usage::{AgentUsageSummary, UsageWindow};

#[derive(Debug, Clone)]
pub struct SqliteStore {
    db_path: PathBuf,
}

impl SqliteStore {
    pub fn new(db_path: &Path) -> Result<Self, AgentError> {
        db::initialize_database(db_path)?;
        db::health_check(db_path)?;
        Ok(Self {
            db_path: db_path.to_path_buf(),
        })
    }

    pub fn database_path(&self) -> &Path {
        &self.db_path
    }

    pub fn health_check(&self) -> Result<(), AgentError> {
        db::health_check(&self.db_path)
    }

    fn open_connection(&self) -> Result<Connection, AgentError> {
        Connection::open(&self.db_path)
            .map_err(|err| AgentError::Storage(format!("open sqlite failed: {err}")))
    }

    pub async fn append_audit_event(&self, event: AuditEvent) -> Result<(), AgentError> {
        let conn = self.open_connection()?;
        audit::insert_event(&conn, &event)
    }

    pub async fn get_audit_events(&self, agent_id: Uuid) -> Result<Vec<AuditEvent>, AgentError> {
        let conn = self.open_connection()?;
        audit::list_events_for_agent(&conn, agent_id)
    }
}

#[async_trait]
pub trait AgentStore: Send + Sync {
    async fn create_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError>;
    async fn get_agent(&self, id: Uuid) -> Result<AgentProfile, AgentError>;
    async fn get_agent_by_identity(
        &self,
        name: &str,
        provider: &str,
        model_name: &str,
    ) -> Result<Option<AgentProfile>, AgentError>;
    async fn list_agents(&self) -> Result<Vec<AgentProfile>, AgentError>;
    async fn update_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError>;
    async fn update_agent_state(
        &self,
        id: Uuid,
        state: AgentLifecycleState,
        failure_reason: Option<String>,
    ) -> Result<AgentProfile, AgentError>;
    async fn delete_agent(&self, id: Uuid) -> Result<(), AgentError>;
    async fn get_mapping_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<OneApiMapping>, AgentError>;
    async fn save_mapping(&self, mapping: OneApiMapping) -> Result<OneApiMapping, AgentError>;
    async fn record_usage(
        &self,
        agent_id: Uuid,
        model_name: &str,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
    ) -> Result<AgentUsageSummary, AgentError>;
    async fn get_usage(&self, agent_id: Uuid) -> Result<AgentUsageSummary, AgentError>;
    async fn get_usage_in_window(
        &self,
        agent_id: Uuid,
        window: UsageWindow,
    ) -> Result<AgentUsageSummary, AgentError>;
    async fn get_daily_total_tokens(&self, agent_id: Uuid, day: &str) -> Result<i64, AgentError>;
}

#[async_trait]
impl AgentStore for SqliteStore {
    async fn create_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError> {
        let conn = self.open_connection()?;
        agent::insert_agent(&conn, &profile)?;
        Ok(profile)
    }

    async fn get_agent(&self, id: Uuid) -> Result<AgentProfile, AgentError> {
        let conn = self.open_connection()?;
        agent::fetch_agent_by_id(&conn, id)
    }

    async fn get_agent_by_identity(
        &self,
        name: &str,
        provider: &str,
        model_name: &str,
    ) -> Result<Option<AgentProfile>, AgentError> {
        let conn = self.open_connection()?;
        agent::fetch_agent_by_identity(&conn, name, provider, model_name)
    }

    async fn list_agents(&self) -> Result<Vec<AgentProfile>, AgentError> {
        let conn = self.open_connection()?;
        agent::list_agents(&conn)
    }

    async fn update_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError> {
        let conn = self.open_connection()?;
        agent::update_agent(&conn, &profile)?;
        Ok(profile)
    }

    async fn update_agent_state(
        &self,
        id: Uuid,
        state: AgentLifecycleState,
        failure_reason: Option<String>,
    ) -> Result<AgentProfile, AgentError> {
        let conn = self.open_connection()?;
        agent::update_agent_state(&conn, id, state, failure_reason.as_deref())?;
        agent::fetch_agent_by_id(&conn, id)
    }

    async fn delete_agent(&self, id: Uuid) -> Result<(), AgentError> {
        let conn = self.open_connection()?;
        agent::delete_agent(&conn, id)
    }

    async fn get_mapping_by_idempotency_key(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<OneApiMapping>, AgentError> {
        let conn = self.open_connection()?;
        one_api::fetch_mapping_by_idempotency_key(&conn, idempotency_key)
    }

    async fn save_mapping(&self, mapping: OneApiMapping) -> Result<OneApiMapping, AgentError> {
        let conn = self.open_connection()?;
        let mut persisted = mapping;
        persisted.updated_at = Utc::now();
        one_api::upsert_mapping(&conn, &persisted)?;
        Ok(persisted)
    }

    async fn record_usage(
        &self,
        agent_id: Uuid,
        model_name: &str,
        input_tokens: i64,
        output_tokens: i64,
        cost_usd: f64,
    ) -> Result<AgentUsageSummary, AgentError> {
        let conn = self.open_connection()?;
        usage::record_usage(
            &conn,
            agent_id,
            model_name,
            input_tokens,
            output_tokens,
            cost_usd,
        )?;
        usage::fetch_usage_summary(&conn, agent_id)
    }

    async fn get_usage(&self, agent_id: Uuid) -> Result<AgentUsageSummary, AgentError> {
        let conn = self.open_connection()?;
        usage::fetch_usage_summary(&conn, agent_id)
    }

    async fn get_usage_in_window(
        &self,
        agent_id: Uuid,
        window: UsageWindow,
    ) -> Result<AgentUsageSummary, AgentError> {
        let conn = self.open_connection()?;
        usage::fetch_usage_summary_in_window(&conn, agent_id, window)
    }

    async fn get_daily_total_tokens(&self, agent_id: Uuid, day: &str) -> Result<i64, AgentError> {
        let conn = self.open_connection()?;
        usage::fetch_daily_total_tokens(&conn, agent_id, day)
    }
}

#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append_event(&self, event: AuditEvent) -> Result<(), AgentError>;
    async fn get_events(&self, agent_id: Uuid) -> Result<Vec<AuditEvent>, AgentError>;
}

#[async_trait]
impl AuditStore for SqliteStore {
    async fn append_event(&self, event: AuditEvent) -> Result<(), AgentError> {
        self.append_audit_event(event).await
    }

    async fn get_events(&self, agent_id: Uuid) -> Result<Vec<AuditEvent>, AgentError> {
        self.get_audit_events(agent_id).await
    }
}
