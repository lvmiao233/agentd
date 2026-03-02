use agentd_core::profile::{BudgetConfig, ModelConfig, PermissionConfig, PermissionPolicy};
use agentd_core::{AgentError, AgentProfile};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Error as SqlError};
use uuid::Uuid;

#[derive(Debug)]
struct StoredAgent {
    id: String,
    name: String,
    model_provider: String,
    model_name: String,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    permission_policy: String,
    allowed_tools_json: String,
    denied_tools_json: String,
    budget_token_limit: Option<i64>,
    budget_cost_limit_usd: Option<f64>,
    budget_time_limit_seconds: Option<i64>,
    created_at: String,
    updated_at: String,
}

pub fn insert_agent(conn: &Connection, profile: &AgentProfile) -> Result<(), AgentError> {
    let stored = from_profile(profile)?;
    conn.execute(
        r#"
        INSERT INTO agents (
            id,
            name,
            model_provider,
            model_name,
            max_tokens,
            temperature,
            permission_policy,
            allowed_tools_json,
            denied_tools_json,
            budget_token_limit,
            budget_cost_limit_usd,
            budget_time_limit_seconds,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14);
        "#,
        params![
            stored.id,
            stored.name,
            stored.model_provider,
            stored.model_name,
            stored.max_tokens,
            stored.temperature,
            stored.permission_policy,
            stored.allowed_tools_json,
            stored.denied_tools_json,
            stored.budget_token_limit,
            stored.budget_cost_limit_usd,
            stored.budget_time_limit_seconds,
            stored.created_at,
            stored.updated_at,
        ],
    )
    .map_err(|err| AgentError::Storage(format!("insert agent failed: {err}")))?;

    Ok(())
}

pub fn fetch_agent_by_id(conn: &Connection, id: Uuid) -> Result<AgentProfile, AgentError> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                id,
                name,
                model_provider,
                model_name,
                max_tokens,
                temperature,
                permission_policy,
                allowed_tools_json,
                denied_tools_json,
                budget_token_limit,
                budget_cost_limit_usd,
                budget_time_limit_seconds,
                created_at,
                updated_at
            FROM agents
            WHERE id = ?1;
            "#,
        )
        .map_err(|err| AgentError::Storage(format!("prepare fetch agent failed: {err}")))?;

    let row = stmt.query_row(params![id.to_string()], |row| {
        Ok(StoredAgent {
            id: row.get(0)?,
            name: row.get(1)?,
            model_provider: row.get(2)?,
            model_name: row.get(3)?,
            max_tokens: row.get(4)?,
            temperature: row.get(5)?,
            permission_policy: row.get(6)?,
            allowed_tools_json: row.get(7)?,
            denied_tools_json: row.get(8)?,
            budget_token_limit: row.get(9)?,
            budget_cost_limit_usd: row.get(10)?,
            budget_time_limit_seconds: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    });

    match row {
        Ok(stored) => to_profile(stored),
        Err(SqlError::QueryReturnedNoRows) => {
            Err(AgentError::NotFound(format!("agent not found: {id}")))
        }
        Err(err) => Err(AgentError::Storage(format!("query agent failed: {err}"))),
    }
}

pub fn list_agents(conn: &Connection) -> Result<Vec<AgentProfile>, AgentError> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                id,
                name,
                model_provider,
                model_name,
                max_tokens,
                temperature,
                permission_policy,
                allowed_tools_json,
                denied_tools_json,
                budget_token_limit,
                budget_cost_limit_usd,
                budget_time_limit_seconds,
                created_at,
                updated_at
            FROM agents
            ORDER BY created_at ASC;
            "#,
        )
        .map_err(|err| AgentError::Storage(format!("prepare list agents failed: {err}")))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(StoredAgent {
                id: row.get(0)?,
                name: row.get(1)?,
                model_provider: row.get(2)?,
                model_name: row.get(3)?,
                max_tokens: row.get(4)?,
                temperature: row.get(5)?,
                permission_policy: row.get(6)?,
                allowed_tools_json: row.get(7)?,
                denied_tools_json: row.get(8)?,
                budget_token_limit: row.get(9)?,
                budget_cost_limit_usd: row.get(10)?,
                budget_time_limit_seconds: row.get(11)?,
                created_at: row.get(12)?,
                updated_at: row.get(13)?,
            })
        })
        .map_err(|err| AgentError::Storage(format!("execute list agents query failed: {err}")))?;

    let mut profiles = Vec::new();
    for row in rows {
        let stored =
            row.map_err(|err| AgentError::Storage(format!("read agent row failed: {err}")))?;
        profiles.push(to_profile(stored)?);
    }

    Ok(profiles)
}

pub fn update_agent(conn: &Connection, profile: &AgentProfile) -> Result<(), AgentError> {
    let stored = from_profile(profile)?;
    let rows_affected = conn
        .execute(
            r#"
            UPDATE agents
            SET
                name = ?2,
                model_provider = ?3,
                model_name = ?4,
                max_tokens = ?5,
                temperature = ?6,
                permission_policy = ?7,
                allowed_tools_json = ?8,
                denied_tools_json = ?9,
                budget_token_limit = ?10,
                budget_cost_limit_usd = ?11,
                budget_time_limit_seconds = ?12,
                created_at = ?13,
                updated_at = ?14
            WHERE id = ?1;
            "#,
            params![
                stored.id,
                stored.name,
                stored.model_provider,
                stored.model_name,
                stored.max_tokens,
                stored.temperature,
                stored.permission_policy,
                stored.allowed_tools_json,
                stored.denied_tools_json,
                stored.budget_token_limit,
                stored.budget_cost_limit_usd,
                stored.budget_time_limit_seconds,
                stored.created_at,
                stored.updated_at,
            ],
        )
        .map_err(|err| AgentError::Storage(format!("update agent failed: {err}")))?;

    if rows_affected == 0 {
        return Err(AgentError::NotFound(format!(
            "agent not found: {}",
            profile.id
        )));
    }

    Ok(())
}

pub fn delete_agent(conn: &Connection, id: Uuid) -> Result<(), AgentError> {
    let rows_affected = conn
        .execute("DELETE FROM agents WHERE id = ?1;", params![id.to_string()])
        .map_err(|err| AgentError::Storage(format!("delete agent failed: {err}")))?;

    if rows_affected == 0 {
        return Err(AgentError::NotFound(format!("agent not found: {id}")));
    }

    Ok(())
}

fn from_profile(profile: &AgentProfile) -> Result<StoredAgent, AgentError> {
    let policy = match profile.permissions.policy {
        PermissionPolicy::Allow => "allow",
        PermissionPolicy::Ask => "ask",
        PermissionPolicy::Deny => "deny",
    }
    .to_string();

    let allowed_tools_json = serde_json::to_string(&profile.permissions.allowed_tools)
        .map_err(|err| AgentError::Storage(format!("serialize allowed_tools failed: {err}")))?;
    let denied_tools_json = serde_json::to_string(&profile.permissions.denied_tools)
        .map_err(|err| AgentError::Storage(format!("serialize denied_tools failed: {err}")))?;

    Ok(StoredAgent {
        id: profile.id.to_string(),
        name: profile.name.clone(),
        model_provider: profile.model.provider.clone(),
        model_name: profile.model.model_name.clone(),
        max_tokens: profile.model.max_tokens,
        temperature: profile.model.temperature,
        permission_policy: policy,
        allowed_tools_json,
        denied_tools_json,
        budget_token_limit: profile
            .budget
            .token_limit
            .map(|v| {
                i64::try_from(v)
                    .map_err(|err| AgentError::Storage(format!("token_limit overflow: {err}")))
            })
            .transpose()?,
        budget_cost_limit_usd: profile.budget.cost_limit_usd,
        budget_time_limit_seconds: profile
            .budget
            .time_limit_seconds
            .map(|v| {
                i64::try_from(v).map_err(|err| {
                    AgentError::Storage(format!("time_limit_seconds overflow: {err}"))
                })
            })
            .transpose()?,
        created_at: profile.created_at.to_rfc3339(),
        updated_at: profile.updated_at.to_rfc3339(),
    })
}

fn to_profile(stored: StoredAgent) -> Result<AgentProfile, AgentError> {
    let id = Uuid::parse_str(&stored.id)
        .map_err(|err| AgentError::Storage(format!("parse agent id failed: {err}")))?;

    let permissions = PermissionConfig {
        policy: parse_permission_policy(&stored.permission_policy)?,
        allowed_tools: serde_json::from_str(&stored.allowed_tools_json)
            .map_err(|err| AgentError::Storage(format!("parse allowed_tools failed: {err}")))?,
        denied_tools: serde_json::from_str(&stored.denied_tools_json)
            .map_err(|err| AgentError::Storage(format!("parse denied_tools failed: {err}")))?,
    };

    let created_at = parse_utc_datetime(&stored.created_at, "created_at")?;
    let updated_at = parse_utc_datetime(&stored.updated_at, "updated_at")?;

    Ok(AgentProfile {
        id,
        name: stored.name,
        model: ModelConfig {
            provider: stored.model_provider,
            model_name: stored.model_name,
            max_tokens: stored.max_tokens,
            temperature: stored.temperature,
        },
        permissions,
        budget: BudgetConfig {
            token_limit: stored
                .budget_token_limit
                .map(|v| {
                    u64::try_from(v).map_err(|err| {
                        AgentError::Storage(format!("invalid persisted token_limit value: {err}"))
                    })
                })
                .transpose()?,
            cost_limit_usd: stored.budget_cost_limit_usd,
            time_limit_seconds: stored
                .budget_time_limit_seconds
                .map(|v| {
                    u64::try_from(v).map_err(|err| {
                        AgentError::Storage(format!(
                            "invalid persisted time_limit_seconds value: {err}"
                        ))
                    })
                })
                .transpose()?,
        },
        created_at,
        updated_at,
    })
}

fn parse_permission_policy(value: &str) -> Result<PermissionPolicy, AgentError> {
    match value {
        "allow" => Ok(PermissionPolicy::Allow),
        "ask" => Ok(PermissionPolicy::Ask),
        "deny" => Ok(PermissionPolicy::Deny),
        other => Err(AgentError::Storage(format!(
            "invalid permission policy: {other}"
        ))),
    }
}

fn parse_utc_datetime(value: &str, field: &str) -> Result<DateTime<Utc>, AgentError> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| AgentError::Storage(format!("parse {field} failed: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use rusqlite::Connection;

    fn test_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("agentd-store-agent-{}.sqlite", Uuid::new_v4()))
    }

    #[test]
    fn insert_and_list_agents_roundtrip() {
        let db_path = test_db_path();
        db::initialize_database(&db_path).expect("initialize db");

        let conn = Connection::open(&db_path).expect("open db");

        let mut profile = AgentProfile::new(
            "demo-agent".to_string(),
            ModelConfig {
                provider: "one-api".to_string(),
                model_name: "claude-4-sonnet".to_string(),
                max_tokens: Some(2048),
                temperature: Some(0.2),
            },
        );
        profile.budget.token_limit = Some(100_000);

        insert_agent(&conn, &profile).expect("insert agent");

        let listed = list_agents(&conn).expect("list agents");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "demo-agent");
        assert_eq!(listed[0].model.model_name, "claude-4-sonnet");
        assert_eq!(listed[0].budget.token_limit, Some(100_000));

        let loaded = fetch_agent_by_id(&conn, profile.id).expect("fetch agent by id");
        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.permissions.policy, PermissionPolicy::Ask);

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }
}
