CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    model_provider TEXT NOT NULL,
    model_name TEXT NOT NULL,
    max_tokens INTEGER,
    temperature REAL,
    permission_policy TEXT NOT NULL,
    allowed_tools_json TEXT NOT NULL,
    denied_tools_json TEXT NOT NULL,
    budget_token_limit INTEGER,
    budget_cost_limit_usd REAL,
    budget_time_limit_seconds INTEGER,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);

CREATE TABLE IF NOT EXISTS quota_usage (
    agent_id TEXT NOT NULL,
    day TEXT NOT NULL,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (agent_id, day),
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_quota_usage_updated_at ON quota_usage(updated_at);
