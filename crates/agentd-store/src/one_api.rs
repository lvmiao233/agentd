use agentd_core::AgentError;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Error as SqlError};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct OneApiMapping {
    pub agent_id: Uuid,
    pub idempotency_key: String,
    pub one_api_token_id: String,
    pub one_api_access_token: String,
    pub one_api_channel_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug)]
struct StoredOneApiMapping {
    agent_id: String,
    idempotency_key: String,
    one_api_token_id: String,
    one_api_access_token: String,
    one_api_channel_id: Option<String>,
    created_at: String,
    updated_at: String,
}

pub fn upsert_mapping(conn: &Connection, mapping: &OneApiMapping) -> Result<(), AgentError> {
    let stored = to_stored(mapping);
    conn.execute(
        r#"
        INSERT INTO one_api_mappings (
            agent_id,
            idempotency_key,
            one_api_token_id,
            one_api_access_token,
            one_api_channel_id,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(agent_id) DO UPDATE SET
            idempotency_key = excluded.idempotency_key,
            one_api_token_id = excluded.one_api_token_id,
            one_api_access_token = excluded.one_api_access_token,
            one_api_channel_id = excluded.one_api_channel_id,
            updated_at = excluded.updated_at;
        "#,
        params![
            stored.agent_id,
            stored.idempotency_key,
            stored.one_api_token_id,
            stored.one_api_access_token,
            stored.one_api_channel_id,
            stored.created_at,
            stored.updated_at,
        ],
    )
    .map_err(|err| AgentError::Storage(format!("upsert one-api mapping failed: {err}")))?;
    Ok(())
}

pub fn fetch_mapping_by_idempotency_key(
    conn: &Connection,
    idempotency_key: &str,
) -> Result<Option<OneApiMapping>, AgentError> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                agent_id,
                idempotency_key,
                one_api_token_id,
                one_api_access_token,
                one_api_channel_id,
                created_at,
                updated_at
            FROM one_api_mappings
            WHERE idempotency_key = ?1;
            "#,
        )
        .map_err(|err| {
            AgentError::Storage(format!("prepare fetch one-api mapping failed: {err}"))
        })?;

    let row = stmt.query_row(params![idempotency_key], |row| {
        Ok(StoredOneApiMapping {
            agent_id: row.get(0)?,
            idempotency_key: row.get(1)?,
            one_api_token_id: row.get(2)?,
            one_api_access_token: row.get(3)?,
            one_api_channel_id: row.get(4)?,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    });

    match row {
        Ok(stored) => Ok(Some(from_stored(stored)?)),
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(AgentError::Storage(format!(
            "query one-api mapping by idempotency key failed: {err}"
        ))),
    }
}

fn to_stored(mapping: &OneApiMapping) -> StoredOneApiMapping {
    StoredOneApiMapping {
        agent_id: mapping.agent_id.to_string(),
        idempotency_key: mapping.idempotency_key.clone(),
        one_api_token_id: mapping.one_api_token_id.clone(),
        one_api_access_token: mapping.one_api_access_token.clone(),
        one_api_channel_id: mapping.one_api_channel_id.clone(),
        created_at: mapping.created_at.to_rfc3339(),
        updated_at: mapping.updated_at.to_rfc3339(),
    }
}

fn from_stored(stored: StoredOneApiMapping) -> Result<OneApiMapping, AgentError> {
    let agent_id = Uuid::parse_str(&stored.agent_id)
        .map_err(|err| AgentError::Storage(format!("parse mapping agent id failed: {err}")))?;
    let created_at = DateTime::parse_from_rfc3339(&stored.created_at)
        .map_err(|err| AgentError::Storage(format!("parse mapping created_at failed: {err}")))?
        .with_timezone(&Utc);
    let updated_at = DateTime::parse_from_rfc3339(&stored.updated_at)
        .map_err(|err| AgentError::Storage(format!("parse mapping updated_at failed: {err}")))?
        .with_timezone(&Utc);

    Ok(OneApiMapping {
        agent_id,
        idempotency_key: stored.idempotency_key,
        one_api_token_id: stored.one_api_token_id,
        one_api_access_token: stored.one_api_access_token,
        one_api_channel_id: stored.one_api_channel_id,
        created_at,
        updated_at,
    })
}
