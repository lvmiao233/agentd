use agentd_core::AgentError;
use rusqlite::Connection;
use std::path::Path;

pub const CURRENT_SCHEMA_VERSION: i32 = 1;

const MIGRATION_0001_SQL: &str = include_str!("../migrations/0001_init.sql");

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
        0 => apply_migration_0001(&mut conn),
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

        std::fs::remove_file(&db_path).expect("cleanup temp db");
    }
}
