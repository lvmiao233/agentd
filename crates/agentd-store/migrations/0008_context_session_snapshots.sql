CREATE TABLE IF NOT EXISTS context_session_snapshots (
    session_id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    head_id TEXT,
    messages_json TEXT NOT NULL,
    tool_results_cache_json TEXT NOT NULL,
    working_directory_json TEXT NOT NULL,
    summary TEXT NOT NULL,
    key_files_json TEXT NOT NULL,
    migration_state TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_context_session_snapshots_agent
ON context_session_snapshots(agent_id);

CREATE INDEX IF NOT EXISTS idx_context_session_snapshots_state
ON context_session_snapshots(migration_state);
