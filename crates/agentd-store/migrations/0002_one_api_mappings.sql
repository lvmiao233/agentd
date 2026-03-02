CREATE TABLE IF NOT EXISTS one_api_mappings (
    agent_id TEXT PRIMARY KEY,
    idempotency_key TEXT NOT NULL UNIQUE,
    one_api_token_id TEXT NOT NULL,
    one_api_access_token TEXT NOT NULL,
    one_api_channel_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_one_api_mappings_token_id ON one_api_mappings(one_api_token_id);
