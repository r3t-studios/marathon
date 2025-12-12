-- Migration 004: Add session support
-- Adds session tables and session-scopes existing tables

-- Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id BLOB PRIMARY KEY,
    code TEXT NOT NULL,
    name TEXT,
    created_at INTEGER NOT NULL,
    last_active INTEGER NOT NULL,
    entity_count INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL,
    secret BLOB,
    UNIQUE(id),
    UNIQUE(code)
);

-- Index for finding recent sessions
CREATE INDEX IF NOT EXISTS idx_sessions_last_active
ON sessions(last_active DESC);

-- Session membership (which node was in which session)
CREATE TABLE IF NOT EXISTS session_membership (
    session_id BLOB NOT NULL,
    node_id TEXT NOT NULL,
    joined_at INTEGER NOT NULL,
    left_at INTEGER,
    PRIMARY KEY (session_id, node_id),
    FOREIGN KEY (session_id) REFERENCES sessions(id) ON DELETE CASCADE
);

-- Add session_id to entities table
ALTER TABLE entities ADD COLUMN session_id BLOB;

-- Index for session-scoped entity queries
CREATE INDEX IF NOT EXISTS idx_entities_session
ON entities(session_id);

-- Add session_id to vector_clock
ALTER TABLE vector_clock ADD COLUMN session_id BLOB;

-- Composite index for session + node lookups
CREATE INDEX IF NOT EXISTS idx_vector_clock_session_node
ON vector_clock(session_id, node_id);

-- Add session_id to operation_log
ALTER TABLE operation_log ADD COLUMN session_id BLOB;

-- Index for session-scoped operation queries
CREATE INDEX IF NOT EXISTS idx_operation_log_session
ON operation_log(session_id, node_id, sequence_number);
