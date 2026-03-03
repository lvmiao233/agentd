CREATE TABLE IF NOT EXISTS usage_records (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    model_name TEXT NOT NULL,
    input_tokens INTEGER NOT NULL,
    output_tokens INTEGER NOT NULL,
    total_tokens INTEGER NOT NULL,
    cost_usd REAL NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_usage_records_agent_time ON usage_records(agent_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_usage_records_model ON usage_records(model_name);
