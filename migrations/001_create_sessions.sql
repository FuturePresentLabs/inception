-- Create sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    agent_type TEXT NOT NULL,
    status TEXT NOT NULL,
    capabilities TEXT NOT NULL DEFAULT '[]',
    metadata TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_heartbeat TEXT
);

-- Index for status queries
CREATE INDEX IF NOT EXISTS idx_sessions_status ON sessions(status);

-- Index for created_at ordering
CREATE INDEX IF NOT EXISTS idx_sessions_created_at ON sessions(created_at DESC);
