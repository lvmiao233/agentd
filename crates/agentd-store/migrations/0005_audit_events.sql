CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    payload_tool_name TEXT,
    payload_message TEXT,
    payload_metadata_json TEXT NOT NULL,
    result TEXT NOT NULL,
    trace_id TEXT,
    session_id TEXT,
    severity TEXT NOT NULL DEFAULT 'info',
    created_at TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_audit_events_agent_time ON audit_events(agent_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_audit_events_trace_id ON audit_events(trace_id);
