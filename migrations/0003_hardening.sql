-- Hardening migration: provenance tracking, tombstone archival, archived_at filter
-- Safe to run on existing databases (all ALTER TABLE … ADD COLUMN IF NOT EXISTS).

-- Issue 4: role-aware provenance for each memory
ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS provenance TEXT NOT NULL DEFAULT 'unknown';

-- Issue 6: tombstone archival — originals are never hard-deleted
ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS archived_at TIMESTAMPTZ;

-- Update archival candidate partial index to exclude tombstoned rows
DROP INDEX IF EXISTS idx_memories_archival_candidates;
CREATE INDEX IF NOT EXISTS idx_memories_archival_candidates
    ON memories(agent_id, created_at)
    WHERE tier = 'L2' AND access_count = 0 AND archived_at IS NULL;

-- Exclude tombstoned memories from general lookups
CREATE INDEX IF NOT EXISTS idx_memories_live
    ON memories(agent_id, created_at DESC)
    WHERE archived_at IS NULL;
