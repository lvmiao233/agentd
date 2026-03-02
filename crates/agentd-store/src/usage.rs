use agentd_core::AgentError;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct ModelUsageBreakdown {
    pub model_name: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentUsageSummary {
    pub agent_id: Uuid,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub total_cost_usd: f64,
    pub model_cost_breakdown: Vec<ModelUsageBreakdown>,
}

pub fn record_usage(
    conn: &Connection,
    agent_id: Uuid,
    model_name: &str,
    input_tokens: i64,
    output_tokens: i64,
    cost_usd: f64,
) -> Result<(), AgentError> {
    if input_tokens < 0 || output_tokens < 0 {
        return Err(AgentError::InvalidInput(
            "usage token values must be non-negative".to_string(),
        ));
    }
    if cost_usd < 0.0 {
        return Err(AgentError::InvalidInput(
            "usage cost must be non-negative".to_string(),
        ));
    }

    let total_tokens = input_tokens
        .checked_add(output_tokens)
        .ok_or_else(|| AgentError::InvalidInput("usage token sum overflow".to_string()))?;
    let day = Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let updated_at = Utc::now().to_rfc3339();
    let agent_id = agent_id.to_string();

    conn.execute(
        r#"
        INSERT INTO quota_usage (
            agent_id,
            day,
            input_tokens,
            output_tokens,
            total_tokens,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(agent_id, day) DO UPDATE SET
            input_tokens = quota_usage.input_tokens + excluded.input_tokens,
            output_tokens = quota_usage.output_tokens + excluded.output_tokens,
            total_tokens = quota_usage.total_tokens + excluded.total_tokens,
            updated_at = excluded.updated_at;
        "#,
        params![
            agent_id,
            day,
            input_tokens,
            output_tokens,
            total_tokens,
            updated_at,
        ],
    )
    .map_err(|err| AgentError::Storage(format!("upsert quota_usage failed: {err}")))?;

    conn.execute(
        r#"
        INSERT INTO quota_usage_by_model (
            agent_id,
            model_name,
            input_tokens,
            output_tokens,
            total_tokens,
            cost_usd,
            updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(agent_id, model_name) DO UPDATE SET
            input_tokens = quota_usage_by_model.input_tokens + excluded.input_tokens,
            output_tokens = quota_usage_by_model.output_tokens + excluded.output_tokens,
            total_tokens = quota_usage_by_model.total_tokens + excluded.total_tokens,
            cost_usd = quota_usage_by_model.cost_usd + excluded.cost_usd,
            updated_at = excluded.updated_at;
        "#,
        params![
            agent_id,
            model_name,
            input_tokens,
            output_tokens,
            total_tokens,
            cost_usd,
            updated_at,
        ],
    )
    .map_err(|err| AgentError::Storage(format!("upsert quota_usage_by_model failed: {err}")))?;

    Ok(())
}

pub fn fetch_daily_total_tokens(
    conn: &Connection,
    agent_id: Uuid,
    day: &str,
) -> Result<i64, AgentError> {
    conn.query_row(
        "SELECT COALESCE(total_tokens, 0) FROM quota_usage WHERE agent_id = ?1 AND day = ?2;",
        params![agent_id.to_string(), day],
        |row| row.get(0),
    )
    .or_else(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => Ok(0),
        _ => Err(err),
    })
    .map_err(|err| AgentError::Storage(format!("query daily quota usage failed: {err}")))
}

pub fn fetch_usage_summary(
    conn: &Connection,
    agent_id: Uuid,
) -> Result<AgentUsageSummary, AgentError> {
    let (input_tokens, output_tokens, total_tokens): (i64, i64, i64) = conn
        .query_row(
            r#"
            SELECT
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(total_tokens), 0)
            FROM quota_usage
            WHERE agent_id = ?1;
            "#,
            params![agent_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|err| AgentError::Storage(format!("query quota_usage aggregate failed: {err}")))?;

    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                model_name,
                input_tokens,
                output_tokens,
                total_tokens,
                cost_usd
            FROM quota_usage_by_model
            WHERE agent_id = ?1
            ORDER BY total_tokens DESC;
            "#,
        )
        .map_err(|err| {
            AgentError::Storage(format!("prepare usage breakdown query failed: {err}"))
        })?;

    let rows = stmt
        .query_map(params![agent_id.to_string()], |row| {
            Ok(ModelUsageBreakdown {
                model_name: row.get(0)?,
                input_tokens: row.get(1)?,
                output_tokens: row.get(2)?,
                total_tokens: row.get(3)?,
                cost_usd: row.get(4)?,
            })
        })
        .map_err(|err| {
            AgentError::Storage(format!("execute usage breakdown query failed: {err}"))
        })?;

    let mut model_cost_breakdown = Vec::new();
    let mut total_cost_usd = 0.0_f64;
    for row in rows {
        let usage =
            row.map_err(|err| AgentError::Storage(format!("read usage row failed: {err}")))?;
        total_cost_usd += usage.cost_usd;
        model_cost_breakdown.push(usage);
    }

    Ok(AgentUsageSummary {
        agent_id,
        input_tokens,
        output_tokens,
        total_tokens,
        total_cost_usd,
        model_cost_breakdown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn usage_record_and_summary_are_aggregated_by_model() {
        let db_path =
            std::env::temp_dir().join(format!("agentd-store-usage-{}.sqlite", Uuid::new_v4()));
        db::initialize_database(&db_path).expect("initialize db");
        let conn = Connection::open(&db_path).expect("open db");

        conn.execute(
            r#"
            INSERT INTO agents (
                id, name, model_provider, model_name,
                permission_policy, allowed_tools_json, denied_tools_json,
                lifecycle_state, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
            "#,
            params![
                "5aa2d5e9-76f6-4633-81ac-5f928ea80ab7",
                "usage-agent",
                "one-api",
                "claude-4-sonnet",
                "ask",
                "[]",
                "[]",
                "ready",
                Utc::now().to_rfc3339(),
                Utc::now().to_rfc3339(),
            ],
        )
        .expect("insert agent fixture");

        let agent_id = Uuid::parse_str("5aa2d5e9-76f6-4633-81ac-5f928ea80ab7").expect("uuid");
        record_usage(&conn, agent_id, "claude-4-sonnet", 60, 40, 0.12).expect("record usage 1");
        record_usage(&conn, agent_id, "claude-4-sonnet", 20, 10, 0.04).expect("record usage 2");
        record_usage(&conn, agent_id, "claude-3-5-haiku", 30, 20, 0.02).expect("record usage 3");

        let summary = fetch_usage_summary(&conn, agent_id).expect("fetch summary");
        assert_eq!(summary.input_tokens, 110);
        assert_eq!(summary.output_tokens, 70);
        assert_eq!(summary.total_tokens, 180);
        assert!((summary.total_cost_usd - 0.18).abs() < 1e-9);
        assert_eq!(summary.model_cost_breakdown.len(), 2);
        assert_eq!(
            summary.model_cost_breakdown[0].model_name,
            "claude-4-sonnet"
        );
        assert_eq!(summary.model_cost_breakdown[0].total_tokens, 130);

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }
}
