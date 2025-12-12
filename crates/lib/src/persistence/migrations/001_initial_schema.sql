-- Migration 001: Initial schema
-- Creates the base tables for entity persistence and CRDT sync

-- Entities table - stores entity metadata
CREATE TABLE IF NOT EXISTS entities (
    id BLOB PRIMARY KEY,
    entity_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Components table - stores serialized component data
CREATE TABLE IF NOT EXISTS components (
    entity_id BLOB NOT NULL,
    component_type TEXT NOT NULL,
    data BLOB NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (entity_id, component_type),
    FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
);

-- Index for querying components by entity
CREATE INDEX IF NOT EXISTS idx_components_entity
ON components(entity_id);

-- Operation log - for CRDT sync protocol
CREATE TABLE IF NOT EXISTS operation_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_id TEXT NOT NULL,
    sequence_number INTEGER NOT NULL,
    operation BLOB NOT NULL,
    timestamp INTEGER NOT NULL,
    UNIQUE(node_id, sequence_number)
);

-- Index for efficient operation log queries
CREATE INDEX IF NOT EXISTS idx_oplog_node_seq
ON operation_log(node_id, sequence_number);

-- Vector clock table - for causality tracking
CREATE TABLE IF NOT EXISTS vector_clock (
    node_id TEXT PRIMARY KEY,
    counter INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Session state table - for crash detection
CREATE TABLE IF NOT EXISTS session_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- WAL checkpoint tracking
CREATE TABLE IF NOT EXISTS checkpoint_state (
    last_checkpoint INTEGER NOT NULL,
    wal_size_bytes INTEGER NOT NULL
);

-- Initialize checkpoint state if not exists
INSERT OR IGNORE INTO checkpoint_state (rowid, last_checkpoint, wal_size_bytes)
VALUES (1, strftime('%s', 'now'), 0);
