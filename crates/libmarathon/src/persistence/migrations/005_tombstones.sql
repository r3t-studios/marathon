-- Migration 005: Add tombstones table
-- Stores deletion tombstones to prevent resurrection of deleted entities

CREATE TABLE IF NOT EXISTS tombstones (
    entity_id BLOB PRIMARY KEY,
    deleting_node TEXT NOT NULL,
    deletion_clock BLOB NOT NULL,
    created_at INTEGER NOT NULL
);

-- Index for querying tombstones by session (for future session scoping)
CREATE INDEX IF NOT EXISTS idx_tombstones_created
ON tombstones(created_at DESC);
