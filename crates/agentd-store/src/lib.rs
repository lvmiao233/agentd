use agentd_core::{AgentError, AgentProfile, AuditEvent};
use async_trait::async_trait;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub mod agent;
pub mod db;

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
}

#[async_trait]
pub trait AgentStore: Send + Sync {
    async fn create_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError>;
    async fn get_agent(&self, id: Uuid) -> Result<AgentProfile, AgentError>;
    async fn list_agents(&self) -> Result<Vec<AgentProfile>, AgentError>;
    async fn update_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError>;
    async fn delete_agent(&self, id: Uuid) -> Result<(), AgentError>;
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

    async fn list_agents(&self) -> Result<Vec<AgentProfile>, AgentError> {
        let conn = self.open_connection()?;
        agent::list_agents(&conn)
    }

    async fn update_agent(&self, profile: AgentProfile) -> Result<AgentProfile, AgentError> {
        let conn = self.open_connection()?;
        agent::update_agent(&conn, &profile)?;
        Ok(profile)
    }

    async fn delete_agent(&self, id: Uuid) -> Result<(), AgentError> {
        let conn = self.open_connection()?;
        agent::delete_agent(&conn, id)
    }
}

#[async_trait]
pub trait AuditStore: Send + Sync {
    async fn append_event(&self, event: AuditEvent) -> Result<(), AgentError>;
    async fn get_events(&self, agent_id: Uuid) -> Result<Vec<AuditEvent>, AgentError>;
}
