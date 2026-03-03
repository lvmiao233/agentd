use agentd_core::audit::{EventPayload, EventResult, EventSeverity, EventType};
use agentd_core::{AgentError, AuditEvent};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use uuid::Uuid;

#[derive(Debug)]
struct StoredAuditEvent {
    id: String,
    agent_id: String,
    event_type: String,
    timestamp: String,
    severity: Option<String>,
    payload_tool_name: Option<String>,
    payload_message: Option<String>,
    payload_metadata_json: String,
    result: String,
    trace_id: Option<String>,
    session_id: Option<String>,
}

pub fn insert_event(conn: &Connection, event: &AuditEvent) -> Result<(), AgentError> {
    let stored = to_stored(event)?;
    let created_at = Utc::now().to_rfc3339();
    conn.execute(
        r#"
        INSERT INTO audit_events (
            id,
            agent_id,
            event_type,
            timestamp,
            payload_tool_name,
            payload_message,
            payload_metadata_json,
            result,
            trace_id,
            session_id,
            severity,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12);
        "#,
        params![
            stored.id,
            stored.agent_id,
            stored.event_type,
            stored.timestamp,
            stored.payload_tool_name,
            stored.payload_message,
            stored.payload_metadata_json,
            stored.result,
            stored.trace_id,
            stored.session_id,
            stored.severity,
            created_at,
        ],
    )
    .map_err(|err| AgentError::Storage(format!("insert audit event failed: {err}")))?;
    Ok(())
}

pub fn list_events_for_agent(
    conn: &Connection,
    agent_id: Uuid,
) -> Result<Vec<AuditEvent>, AgentError> {
    let mut stmt = conn
        .prepare(
            r#"
            SELECT
                id,
                agent_id,
                event_type,
                timestamp,
                severity,
                payload_tool_name,
                payload_message,
                payload_metadata_json,
                result,
                trace_id,
                session_id
            FROM audit_events
            WHERE agent_id = ?1
            ORDER BY timestamp DESC;
            "#,
        )
        .map_err(|err| AgentError::Storage(format!("prepare list audit events failed: {err}")))?;

    let rows = stmt
        .query_map(params![agent_id.to_string()], |row| {
            Ok(StoredAuditEvent {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                event_type: row.get(2)?,
                timestamp: row.get(3)?,
                severity: row.get(4)?,
                payload_tool_name: row.get(5)?,
                payload_message: row.get(6)?,
                payload_metadata_json: row.get(7)?,
                result: row.get(8)?,
                trace_id: row.get(9)?,
                session_id: row.get(10)?,
            })
        })
        .map_err(|err| AgentError::Storage(format!("execute list audit events failed: {err}")))?;

    let mut events = Vec::new();
    for row in rows {
        let stored =
            row.map_err(|err| AgentError::Storage(format!("read audit event row failed: {err}")))?;
        events.push(from_stored(stored)?);
    }
    Ok(events)
}

fn to_stored(event: &AuditEvent) -> Result<StoredAuditEvent, AgentError> {
    let payload_metadata_json = serde_json::to_string(&event.payload.metadata).map_err(|err| {
        AgentError::Storage(format!("serialize audit payload metadata failed: {err}"))
    })?;

    Ok(StoredAuditEvent {
        id: event.id.to_string(),
        agent_id: event.agent_id.to_string(),
        event_type: event_type_to_str(&event.event_type).to_string(),
        timestamp: event.timestamp.to_rfc3339(),
        severity: Some(event_severity_to_str(&event.severity).to_string()),
        payload_tool_name: event.payload.tool_name.clone(),
        payload_message: event.payload.message.clone(),
        payload_metadata_json,
        result: event_result_to_str(&event.result).to_string(),
        trace_id: Some(event.trace_id.clone()),
        session_id: Some(event.session_id.clone()),
    })
}

fn from_stored(stored: StoredAuditEvent) -> Result<AuditEvent, AgentError> {
    let id = Uuid::parse_str(&stored.id)
        .map_err(|err| AgentError::Storage(format!("parse audit id failed: {err}")))?;
    let agent_id = Uuid::parse_str(&stored.agent_id)
        .map_err(|err| AgentError::Storage(format!("parse audit agent_id failed: {err}")))?;
    let timestamp = parse_utc_datetime(&stored.timestamp)?;
    let metadata: serde_json::Value =
        serde_json::from_str(&stored.payload_metadata_json).map_err(|err| {
            AgentError::Storage(format!("parse audit payload metadata failed: {err}"))
        })?;
    let result = parse_event_result(&stored.result)?;
    let severity = match stored.severity.as_deref() {
        Some(value) if !value.trim().is_empty() => parse_event_severity(value)?,
        _ => EventSeverity::from_result(&result),
    };
    let trace_id = stored
        .trace_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("trace-{id}"));
    let session_id = stored
        .session_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("session-{id}"));

    Ok(AuditEvent {
        id,
        agent_id,
        event_type: parse_event_type(&stored.event_type)?,
        timestamp,
        severity,
        payload: EventPayload {
            tool_name: stored.payload_tool_name,
            message: stored.payload_message,
            metadata,
        },
        result,
        trace_id,
        session_id,
    })
}

fn event_type_to_str(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::AgentCreated => "agent.created",
        EventType::AgentStarted => "agent.started",
        EventType::AgentStopped => "agent.stopped",
        EventType::ToolInvoked => "tool.invoked",
        EventType::ToolApproved => "tool.approved",
        EventType::ToolDenied => "tool.denied",
        EventType::BudgetExceeded => "budget.exceeded",
        EventType::PermissionDenied => "permission.denied",
        EventType::Error => "error",
    }
}

fn parse_event_type(value: &str) -> Result<EventType, AgentError> {
    match value {
        "agent.created" => Ok(EventType::AgentCreated),
        "agent.started" => Ok(EventType::AgentStarted),
        "agent.stopped" => Ok(EventType::AgentStopped),
        "tool.invoked" => Ok(EventType::ToolInvoked),
        "tool.approved" => Ok(EventType::ToolApproved),
        "tool.denied" => Ok(EventType::ToolDenied),
        "budget.exceeded" => Ok(EventType::BudgetExceeded),
        "permission.denied" => Ok(EventType::PermissionDenied),
        "error" => Ok(EventType::Error),
        other => Err(AgentError::Storage(format!(
            "invalid persisted audit event_type: {other}"
        ))),
    }
}

fn event_result_to_str(result: &EventResult) -> &'static str {
    match result {
        EventResult::Success => "success",
        EventResult::Failure => "failure",
        EventResult::Pending => "pending",
        EventResult::Cancelled => "cancelled",
    }
}

fn event_severity_to_str(severity: &EventSeverity) -> &'static str {
    match severity {
        EventSeverity::Info => "info",
        EventSeverity::Warning => "warning",
        EventSeverity::Error => "error",
    }
}

fn parse_event_result(value: &str) -> Result<EventResult, AgentError> {
    match value {
        "success" => Ok(EventResult::Success),
        "failure" => Ok(EventResult::Failure),
        "pending" => Ok(EventResult::Pending),
        "cancelled" => Ok(EventResult::Cancelled),
        other => Err(AgentError::Storage(format!(
            "invalid persisted audit result: {other}"
        ))),
    }
}

fn parse_event_severity(value: &str) -> Result<EventSeverity, AgentError> {
    match value {
        "info" => Ok(EventSeverity::Info),
        "warning" => Ok(EventSeverity::Warning),
        "error" => Ok(EventSeverity::Error),
        other => Err(AgentError::Storage(format!(
            "invalid persisted audit severity: {other}"
        ))),
    }
}

fn parse_utc_datetime(value: &str) -> Result<DateTime<Utc>, AgentError> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| AgentError::Storage(format!("parse audit timestamp failed: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use agentd_core::audit::AuditContext;

    fn insert_agent_fixture(conn: &Connection, agent_id: Uuid) {
        conn.execute(
            r#"
            INSERT INTO agents (
                id, name, model_provider, model_name,
                permission_policy, allowed_tools_json, denied_tools_json,
                lifecycle_state, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
            "#,
            params![
                agent_id.to_string(),
                "audit-agent",
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
    }

    #[test]
    fn append_and_list_audit_events_roundtrip() {
        let db_path =
            std::env::temp_dir().join(format!("agentd-store-audit-{}.sqlite", Uuid::new_v4()));
        db::initialize_database(&db_path).expect("initialize db");
        let conn = Connection::open(&db_path).expect("open db");

        let agent_id = Uuid::new_v4();
        insert_agent_fixture(&conn, agent_id);

        let mut event1 = AuditEvent::new(
            agent_id,
            EventType::AgentStarted,
            EventPayload {
                tool_name: None,
                message: Some("started".to_string()),
                metadata: serde_json::json!({"source": "test"}),
            },
            EventResult::Success,
        );
        event1.timestamp = Utc::now() - chrono::Duration::seconds(1);
        insert_event(&conn, &event1).expect("append event1");

        let mut event2 = AuditEvent::new(
            agent_id,
            EventType::ToolDenied,
            EventPayload {
                tool_name: Some("bash:rm".to_string()),
                message: Some("denied".to_string()),
                metadata: serde_json::json!({"rule": "bash:rm"}),
            },
            EventResult::Failure,
        );
        event2.timestamp = Utc::now();
        insert_event(&conn, &event2).expect("append event2");

        let events = list_events_for_agent(&conn, agent_id).expect("list events");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, EventType::ToolDenied);
        assert_eq!(events[1].event_type, EventType::AgentStarted);
        assert_eq!(events[0].payload.tool_name.as_deref(), Some("bash:rm"));
        assert_eq!(events[0].severity, EventSeverity::Error);
        assert_eq!(events[1].severity, EventSeverity::Info);
        assert!(!events[0].trace_id.is_empty());
        assert!(!events[0].session_id.is_empty());

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }

    #[test]
    fn append_event_persists_trace_session_and_severity_from_context() {
        let db_path =
            std::env::temp_dir().join(format!("agentd-store-audit-ctx-{}.sqlite", Uuid::new_v4()));
        db::initialize_database(&db_path).expect("initialize db");
        let conn = Connection::open(&db_path).expect("open db");

        let agent_id = Uuid::new_v4();
        insert_agent_fixture(&conn, agent_id);

        let event = AuditEvent::new_with_context(
            agent_id,
            EventType::ToolInvoked,
            EventPayload {
                tool_name: Some("builtin.lite.upper".to_string()),
                message: Some("rpc authorize".to_string()),
                metadata: serde_json::json!({"source": "unit-test"}),
            },
            EventResult::Pending,
            AuditContext::new("trace-rpc-15", "rpc-15", EventSeverity::Warning),
        );
        insert_event(&conn, &event).expect("append event with context");

        let events = list_events_for_agent(&conn, agent_id).expect("list events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].trace_id, "trace-rpc-15");
        assert_eq!(events[0].session_id, "rpc-15");
        assert_eq!(events[0].severity, EventSeverity::Warning);

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }
}
