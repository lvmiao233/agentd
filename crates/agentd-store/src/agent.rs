use agentd_core::profile::{BudgetConfig, ModelConfig, PermissionConfig, PermissionPolicy};
use agentd_core::{AgentError, AgentLifecycleState, AgentProfile};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Error as SqlError};
use serde_json::Value;
use std::collections::BTreeMap;
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
    lifecycle_state: String,
    failure_reason: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RegistryAgentEntry {
    pub agent_id: String,
    pub name: String,
    pub model: String,
    pub provider: String,
    pub endpoint: String,
    pub health: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct DelegationAgentSummary {
    pub agent_id: String,
    pub name: String,
    pub model: String,
    pub provider: String,
    pub health: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ContextSessionSnapshot {
    pub session_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub head_id: Option<String>,
    #[serde(default)]
    pub messages: Vec<Value>,
    #[serde(default)]
    pub tool_results_cache: Value,
    #[serde(default)]
    pub working_directory: BTreeMap<String, String>,
    pub summary: String,
    #[serde(default)]
    pub key_files: Vec<String>,
    pub migration_state: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn delegation_candidates_from_profiles(
    profiles: &[AgentProfile],
) -> Vec<DelegationAgentSummary> {
    let mut candidates = profiles
        .iter()
        .filter(|profile| matches!(profile.status, AgentLifecycleState::Ready))
        .map(|profile| DelegationAgentSummary {
            agent_id: profile.id.to_string(),
            name: profile.name.clone(),
            model: profile.model.model_name.clone(),
            provider: profile.model.provider.clone(),
            health: "ready".to_string(),
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.agent_id.cmp(&right.agent_id))
    });
    candidates
}

pub fn to_registry_agent_entry(
    profile: &AgentProfile,
    endpoint: String,
    health: String,
) -> RegistryAgentEntry {
    RegistryAgentEntry {
        agent_id: profile.id.to_string(),
        name: profile.name.clone(),
        model: profile.model.model_name.clone(),
        provider: profile.model.provider.clone(),
        endpoint,
        health,
        updated_at: Utc::now().to_rfc3339(),
    }
}

pub fn upsert_context_session_snapshot(
    conn: &Connection,
    snapshot: &ContextSessionSnapshot,
) -> Result<(), AgentError> {
    let migration_state = normalize_migration_state(&snapshot.migration_state)?;

    let messages_json = serde_json::to_string(&snapshot.messages)
        .map_err(|err| AgentError::Storage(format!("serialize session messages failed: {err}")))?;
    let tool_results_cache_json =
        serde_json::to_string(&snapshot.tool_results_cache).map_err(|err| {
            AgentError::Storage(format!(
                "serialize session tool_results_cache failed: {err}"
            ))
        })?;
    let working_directory_json =
        serde_json::to_string(&snapshot.working_directory).map_err(|err| {
            AgentError::Storage(format!("serialize session working_directory failed: {err}"))
        })?;
    let key_files_json = serde_json::to_string(&snapshot.key_files)
        .map_err(|err| AgentError::Storage(format!("serialize session key_files failed: {err}")))?;

    conn.execute(
        r#"
        INSERT INTO context_session_snapshots (
            session_id,
            agent_id,
            head_id,
            messages_json,
            tool_results_cache_json,
            working_directory_json,
            summary,
            key_files_json,
            migration_state,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(session_id) DO UPDATE SET
            agent_id = excluded.agent_id,
            head_id = excluded.head_id,
            messages_json = excluded.messages_json,
            tool_results_cache_json = excluded.tool_results_cache_json,
            working_directory_json = excluded.working_directory_json,
            summary = excluded.summary,
            key_files_json = excluded.key_files_json,
            migration_state = excluded.migration_state,
            created_at = excluded.created_at,
            updated_at = excluded.updated_at;
        "#,
        params![
            snapshot.session_id.as_str(),
            snapshot.agent_id.as_str(),
            snapshot.head_id.as_deref(),
            messages_json,
            tool_results_cache_json,
            working_directory_json,
            snapshot.summary.as_str(),
            key_files_json,
            migration_state,
            snapshot.created_at.as_str(),
            snapshot.updated_at.as_str(),
        ],
    )
    .map_err(|err| AgentError::Storage(format!("upsert context session snapshot failed: {err}")))?;

    Ok(())
}

pub fn fetch_context_session_snapshot(
    conn: &Connection,
    session_id: &str,
) -> Result<Option<ContextSessionSnapshot>, AgentError> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                session_id,
                agent_id,
                head_id,
                messages_json,
                tool_results_cache_json,
                working_directory_json,
                summary,
                key_files_json,
                migration_state,
                created_at,
                updated_at
            FROM context_session_snapshots
            WHERE session_id = ?1
            LIMIT 1;
            "#,
        )
        .map_err(|err| {
            AgentError::Storage(format!(
                "prepare fetch context session snapshot failed: {err}"
            ))
        })?;

    let row = stmt.query_row(params![session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
        ))
    });

    match row {
        Ok((
            loaded_session_id,
            agent_id,
            head_id,
            messages_json,
            tool_results_cache_json,
            working_directory_json,
            summary,
            key_files_json,
            migration_state,
            created_at,
            updated_at,
        )) => {
            normalize_migration_state(&migration_state)?;
            let messages = serde_json::from_str::<Vec<Value>>(&messages_json).map_err(|err| {
                AgentError::Storage(format!("parse context session messages failed: {err}"))
            })?;
            let tool_results_cache = serde_json::from_str::<Value>(&tool_results_cache_json)
                .map_err(|err| {
                    AgentError::Storage(format!(
                        "parse context session tool_results_cache failed: {err}"
                    ))
                })?;
            let working_directory = serde_json::from_str::<BTreeMap<String, String>>(
                &working_directory_json,
            )
            .map_err(|err| {
                AgentError::Storage(format!(
                    "parse context session working_directory failed: {err}"
                ))
            })?;
            let key_files =
                serde_json::from_str::<Vec<String>>(&key_files_json).map_err(|err| {
                    AgentError::Storage(format!("parse context session key_files failed: {err}"))
                })?;

            Ok(Some(ContextSessionSnapshot {
                session_id: loaded_session_id,
                agent_id,
                head_id,
                messages,
                tool_results_cache,
                working_directory,
                summary,
                key_files,
                migration_state,
                created_at,
                updated_at,
            }))
        }
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(AgentError::Storage(format!(
            "query context session snapshot failed: {err}"
        ))),
    }
}

pub fn update_context_session_migration_state(
    conn: &Connection,
    session_id: &str,
    migration_state: &str,
) -> Result<(), AgentError> {
    let normalized_state = normalize_migration_state(migration_state)?;
    let updated_at = Utc::now().to_rfc3339();
    let rows_affected = conn
        .execute(
            r#"
            UPDATE context_session_snapshots
            SET migration_state = ?2, updated_at = ?3
            WHERE session_id = ?1;
            "#,
            params![session_id, normalized_state, updated_at],
        )
        .map_err(|err| {
            AgentError::Storage(format!(
                "update context session migration state failed: {err}"
            ))
        })?;

    if rows_affected == 0 {
        return Err(AgentError::NotFound(format!(
            "context session snapshot not found: {session_id}"
        )));
    }

    Ok(())
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
            lifecycle_state,
            failure_reason,
            created_at,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16);
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
            stored.lifecycle_state,
            stored.failure_reason,
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
                lifecycle_state,
                failure_reason,
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
            lifecycle_state: row.get(12)?,
            failure_reason: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
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
                lifecycle_state,
                failure_reason,
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
                lifecycle_state: row.get(12)?,
                failure_reason: row.get(13)?,
                created_at: row.get(14)?,
                updated_at: row.get(15)?,
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
                lifecycle_state = ?13,
                failure_reason = ?14,
                created_at = ?15,
                updated_at = ?16
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
                stored.lifecycle_state,
                stored.failure_reason,
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

pub fn fetch_agent_by_identity(
    conn: &Connection,
    name: &str,
    provider: &str,
    model_name: &str,
) -> Result<Option<AgentProfile>, AgentError> {
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
                lifecycle_state,
                failure_reason,
                created_at,
                updated_at
            FROM agents
            WHERE name = ?1 AND model_provider = ?2 AND model_name = ?3
            ORDER BY created_at ASC
            LIMIT 1;
            "#,
        )
        .map_err(|err| {
            AgentError::Storage(format!("prepare fetch agent by identity failed: {err}"))
        })?;

    let row = stmt.query_row(params![name, provider, model_name], |row| {
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
            lifecycle_state: row.get(12)?,
            failure_reason: row.get(13)?,
            created_at: row.get(14)?,
            updated_at: row.get(15)?,
        })
    });

    match row {
        Ok(stored) => Ok(Some(to_profile(stored)?)),
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(AgentError::Storage(format!(
            "query agent by identity failed: {err}"
        ))),
    }
}

pub fn update_agent_state(
    conn: &Connection,
    id: Uuid,
    state: AgentLifecycleState,
    failure_reason: Option<&str>,
) -> Result<(), AgentError> {
    let state_str = lifecycle_state_to_str(&state);
    let updated_at = Utc::now().to_rfc3339();
    let rows_affected = conn
        .execute(
            r#"
            UPDATE agents
            SET lifecycle_state = ?2, failure_reason = ?3, updated_at = ?4
            WHERE id = ?1;
            "#,
            params![id.to_string(), state_str, failure_reason, updated_at],
        )
        .map_err(|err| AgentError::Storage(format!("update agent state failed: {err}")))?;

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
        lifecycle_state: lifecycle_state_to_str(&profile.status).to_string(),
        failure_reason: profile.failure_reason.clone(),
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
        status: parse_lifecycle_state(&stored.lifecycle_state)?,
        failure_reason: stored.failure_reason,
        created_at,
        updated_at,
    })
}

fn lifecycle_state_to_str(state: &AgentLifecycleState) -> &'static str {
    match state {
        AgentLifecycleState::Creating => "creating",
        AgentLifecycleState::Ready => "ready",
        AgentLifecycleState::Running => "running",
        AgentLifecycleState::Stopped => "stopped",
        AgentLifecycleState::Failed => "failed",
    }
}

fn parse_lifecycle_state(value: &str) -> Result<AgentLifecycleState, AgentError> {
    match value {
        "creating" => Ok(AgentLifecycleState::Creating),
        "ready" => Ok(AgentLifecycleState::Ready),
        "running" => Ok(AgentLifecycleState::Running),
        "stopped" => Ok(AgentLifecycleState::Stopped),
        "failed" => Ok(AgentLifecycleState::Failed),
        other => Err(AgentError::Storage(format!(
            "invalid lifecycle state: {other}"
        ))),
    }
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

fn normalize_migration_state(value: &str) -> Result<String, AgentError> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "active" | "migrated" => Ok(normalized),
        other => Err(AgentError::Storage(format!(
            "invalid context migration state: {other}"
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
    use serde_json::json;

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
        update_agent_state(&conn, profile.id, AgentLifecycleState::Ready, None)
            .expect("update agent state");

        let listed = list_agents(&conn).expect("list agents");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].name, "demo-agent");
        assert_eq!(listed[0].model.model_name, "claude-4-sonnet");
        assert_eq!(listed[0].budget.token_limit, Some(100_000));
        assert_eq!(listed[0].status, AgentLifecycleState::Ready);

        let loaded = fetch_agent_by_id(&conn, profile.id).expect("fetch agent by id");
        assert_eq!(loaded.id, profile.id);
        assert_eq!(loaded.permissions.policy, PermissionPolicy::Ask);

        let by_identity =
            fetch_agent_by_identity(&conn, "demo-agent", "one-api", "claude-4-sonnet")
                .expect("fetch by identity")
                .expect("agent should exist");
        assert_eq!(by_identity.id, profile.id);
        assert_eq!(by_identity.status, AgentLifecycleState::Ready);

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }

    #[test]
    fn delegation_candidates_only_include_ready_agents() {
        let model = ModelConfig {
            provider: "one-api".to_string(),
            model_name: "claude-4-sonnet".to_string(),
            max_tokens: None,
            temperature: None,
        };

        let mut ready_a = AgentProfile::new("ready-a".to_string(), model.clone());
        ready_a.status = AgentLifecycleState::Ready;

        let mut creating = AgentProfile::new("creating".to_string(), model.clone());
        creating.status = AgentLifecycleState::Creating;

        let mut ready_b = AgentProfile::new("ready-b".to_string(), model);
        ready_b.status = AgentLifecycleState::Ready;

        let candidates = delegation_candidates_from_profiles(&[ready_b, creating, ready_a]);
        assert_eq!(candidates.len(), 2, "only ready agents should be delegated");
        assert_eq!(candidates[0].name, "ready-a");
        assert_eq!(candidates[1].name, "ready-b");
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.health == "ready"),
            "delegation candidates should carry ready health"
        );
    }

    #[test]
    fn context_session_snapshot_roundtrip() {
        let db_path = test_db_path();
        db::initialize_database(&db_path).expect("initialize db");
        let conn = Connection::open(&db_path).expect("open db");

        let now = Utc::now().to_rfc3339();
        let snapshot = ContextSessionSnapshot {
            session_id: "session-26-roundtrip".to_string(),
            agent_id: "agent-26".to_string(),
            head_id: Some("msg-2".to_string()),
            messages: vec![
                json!({
                    "id": "msg-1",
                    "parent_id": Value::Null,
                    "role": "user",
                    "content": "remember alpha",
                }),
                json!({
                    "id": "msg-2",
                    "parent_id": "msg-1",
                    "role": "assistant",
                    "content": "alpha acknowledged",
                }),
            ],
            tool_results_cache: json!({"call-1": {"status": "ok"}}),
            working_directory: BTreeMap::from([(
                "README.md".to_string(),
                "# migrated".to_string(),
            )]),
            summary: "context summary: remember alpha".to_string(),
            key_files: vec!["README.md".to_string()],
            migration_state: "active".to_string(),
            created_at: now.clone(),
            updated_at: now,
        };

        upsert_context_session_snapshot(&conn, &snapshot).expect("persist context snapshot");
        let loaded = fetch_context_session_snapshot(&conn, &snapshot.session_id)
            .expect("fetch context snapshot")
            .expect("context snapshot should exist");
        assert_eq!(loaded, snapshot);

        update_context_session_migration_state(&conn, &snapshot.session_id, "migrated")
            .expect("mark context snapshot migrated");
        let migrated = fetch_context_session_snapshot(&conn, &snapshot.session_id)
            .expect("fetch migrated snapshot")
            .expect("migrated context snapshot should exist");
        assert_eq!(migrated.migration_state, "migrated");

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }

    #[test]
    fn context_session_snapshot_rejects_unknown_state() {
        let db_path = test_db_path();
        db::initialize_database(&db_path).expect("initialize db");
        let conn = Connection::open(&db_path).expect("open db");

        let now = Utc::now().to_rfc3339();
        let snapshot = ContextSessionSnapshot {
            session_id: "session-26-invalid-state".to_string(),
            agent_id: "agent-26".to_string(),
            head_id: None,
            messages: Vec::new(),
            tool_results_cache: json!({}),
            working_directory: BTreeMap::new(),
            summary: "summary".to_string(),
            key_files: Vec::new(),
            migration_state: "unknown".to_string(),
            created_at: now.clone(),
            updated_at: now,
        };

        let err = upsert_context_session_snapshot(&conn, &snapshot)
            .expect_err("unknown state should be rejected");
        assert!(
            err.to_string().contains("invalid context migration state"),
            "unexpected error: {err}"
        );

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }
}
