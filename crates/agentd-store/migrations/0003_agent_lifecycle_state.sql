ALTER TABLE agents ADD COLUMN lifecycle_state TEXT NOT NULL DEFAULT 'ready';
ALTER TABLE agents ADD COLUMN failure_reason TEXT;

CREATE INDEX IF NOT EXISTS idx_agents_lifecycle_state ON agents(lifecycle_state);
