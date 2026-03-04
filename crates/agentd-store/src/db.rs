use agentd_core::AgentError;
use rusqlite::Connection;
use std::path::Path;

pub const CURRENT_SCHEMA_VERSION: i32 = 8;

const MIGRATION_0001_SQL: &str = include_str!("../migrations/0001_init.sql");
const MIGRATION_0002_SQL: &str = include_str!("../migrations/0002_one_api_mappings.sql");
const MIGRATION_0003_SQL: &str = include_str!("../migrations/0003_agent_lifecycle_state.sql");
const MIGRATION_0004_SQL: &str = include_str!("../migrations/0004_quota_usage_model_breakdown.sql");
const MIGRATION_0005_SQL: &str = include_str!("../migrations/0005_audit_events.sql");
const MIGRATION_0006_SQL: &str = include_str!("../migrations/0006_usage_records_window.sql");
const MIGRATION_0007_SQL: &str = include_str!("../migrations/0007_backfill_audit_context.sql");
const MIGRATION_0008_SQL: &str = include_str!("../migrations/0008_context_session_snapshots.sql");

pub fn initialize_database(path: &Path) -> Result<(), AgentError> {
    if let Some(parent_dir) = path.parent() {
        if !parent_dir.exists() {
            std::fs::create_dir_all(parent_dir)
                .map_err(|err| AgentError::Storage(format!("create db dir failed: {err}")))?;
        }
    }

    let mut conn = Connection::open(path)
        .map_err(|err| AgentError::Storage(format!("open sqlite failed: {err}")))?;

    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|err| AgentError::Storage(format!("apply sqlite pragmas failed: {err}")))?;

    let current_version: i32 = conn
        .query_row("PRAGMA user_version;", [], |row| row.get(0))
        .map_err(|err| AgentError::Storage(format!("read schema version failed: {err}")))?;

    match current_version {
        0 => {
            apply_migration_0001(&mut conn)?;
            apply_migration_0002(&mut conn)?;
            apply_migration_0003(&mut conn)?;
            apply_migration_0004(&mut conn)?;
            apply_migration_0005(&mut conn)?;
            apply_migration_0006(&mut conn)?;
            apply_migration_0007(&mut conn)?;
            apply_migration_0008(&mut conn)
        }
        1 => {
            apply_migration_0002(&mut conn)?;
            apply_migration_0003(&mut conn)?;
            apply_migration_0004(&mut conn)?;
            apply_migration_0005(&mut conn)?;
            apply_migration_0006(&mut conn)?;
            apply_migration_0007(&mut conn)?;
            apply_migration_0008(&mut conn)
        }
        2 => {
            apply_migration_0003(&mut conn)?;
            apply_migration_0004(&mut conn)?;
            apply_migration_0005(&mut conn)?;
            apply_migration_0006(&mut conn)?;
            apply_migration_0007(&mut conn)?;
            apply_migration_0008(&mut conn)
        }
        3 => {
            apply_migration_0004(&mut conn)?;
            apply_migration_0005(&mut conn)?;
            apply_migration_0006(&mut conn)?;
            apply_migration_0007(&mut conn)?;
            apply_migration_0008(&mut conn)
        }
        4 => {
            apply_migration_0005(&mut conn)?;
            apply_migration_0006(&mut conn)?;
            apply_migration_0007(&mut conn)?;
            apply_migration_0008(&mut conn)
        }
        5 => {
            apply_migration_0006(&mut conn)?;
            apply_migration_0007(&mut conn)?;
            apply_migration_0008(&mut conn)
        }
        6 => {
            apply_migration_0007(&mut conn)?;
            apply_migration_0008(&mut conn)
        }
        7 => apply_migration_0008(&mut conn),
        CURRENT_SCHEMA_VERSION => Ok(()),
        version if version > CURRENT_SCHEMA_VERSION => Err(AgentError::Storage(format!(
            "unsupported schema version {version}, expected <= {CURRENT_SCHEMA_VERSION}"
        ))),
        version => Err(AgentError::Storage(format!(
            "unknown schema version {version}, expected {CURRENT_SCHEMA_VERSION}"
        ))),
    }
}

pub fn health_check(path: &Path) -> Result<(), AgentError> {
    let conn = Connection::open(path)
        .map_err(|err| AgentError::Storage(format!("open sqlite failed: {err}")))?;

    conn.query_row("SELECT 1;", [], |row| row.get::<_, i64>(0))
        .map_err(|err| AgentError::Storage(format!("sqlite health query failed: {err}")))?;

    Ok(())
}

fn apply_migration_0001(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0001_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0001 failed: {err}")))?;

    tx.execute_batch("PRAGMA user_version = 1;")
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

fn apply_migration_0002(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0002_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0002 failed: {err}")))?;

    tx.execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

fn apply_migration_0003(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0003_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0003 failed: {err}")))?;

    tx.execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

fn apply_migration_0004(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0004_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0004 failed: {err}")))?;

    tx.execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

fn apply_migration_0005(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0005_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0005 failed: {err}")))?;

    tx.execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

fn apply_migration_0006(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0006_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0006 failed: {err}")))?;

    tx.execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

fn apply_migration_0007(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0007_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0007 failed: {err}")))?;

    tx.execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

fn apply_migration_0008(conn: &mut Connection) -> Result<(), AgentError> {
    let tx = conn
        .transaction()
        .map_err(|err| AgentError::Storage(format!("start migration transaction failed: {err}")))?;

    tx.execute_batch(MIGRATION_0008_SQL)
        .map_err(|err| AgentError::Storage(format!("apply migration 0008 failed: {err}")))?;

    tx.execute_batch(&format!("PRAGMA user_version = {CURRENT_SCHEMA_VERSION};"))
        .map_err(|err| AgentError::Storage(format!("set schema version failed: {err}")))?;

    tx.commit()
        .map_err(|err| AgentError::Storage(format!("commit migration failed: {err}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use uuid::Uuid;

    fn test_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("agentd-store-db-{}.sqlite", Uuid::new_v4()))
    }

    #[test]
    fn initialize_database_is_idempotent_and_creates_expected_tables() {
        let db_path = test_db_path();

        initialize_database(&db_path).expect("first migration should succeed");
        initialize_database(&db_path).expect("second migration should be idempotent");

        let conn = Connection::open(&db_path).expect("open sqlite for verification");

        let user_version: i32 = conn
            .query_row("PRAGMA user_version;", [], |row| row.get(0))
            .expect("query user_version");
        assert_eq!(user_version, CURRENT_SCHEMA_VERSION);

        let has_agents: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='agents';",
                [],
                |row| row.get(0),
            )
            .expect("check agents table");
        assert_eq!(has_agents, 1);

        let has_quota_usage: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='quota_usage';",
                [],
                |row| row.get(0),
            )
            .expect("check quota_usage table");
        assert_eq!(has_quota_usage, 1);

        let has_one_api_mappings: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='one_api_mappings';",
                [],
                |row| row.get(0),
            )
            .expect("check one_api_mappings table");
        assert_eq!(has_one_api_mappings, 1);

        let has_quota_usage_by_model: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='quota_usage_by_model';",
                [],
                |row| row.get(0),
            )
            .expect("check quota_usage_by_model table");
        assert_eq!(has_quota_usage_by_model, 1);

        let has_lifecycle_state: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name='lifecycle_state';",
                [],
                |row| row.get(0),
            )
            .expect("check lifecycle_state column");
        assert_eq!(has_lifecycle_state, 1);

        let has_failure_reason: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('agents') WHERE name='failure_reason';",
                [],
                |row| row.get(0),
            )
            .expect("check failure_reason column");
        assert_eq!(has_failure_reason, 1);

        let has_audit_events: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='audit_events';",
                [],
                |row| row.get(0),
            )
            .expect("check audit_events table");
        assert_eq!(has_audit_events, 1);

        let has_usage_records: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='usage_records';",
                [],
                |row| row.get(0),
            )
            .expect("check usage_records table");
        assert_eq!(has_usage_records, 1);

        let has_context_session_snapshots: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='context_session_snapshots';",
                [],
                |row| row.get(0),
            )
            .expect("check context_session_snapshots table");
        assert_eq!(has_context_session_snapshots, 1);

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }
}
