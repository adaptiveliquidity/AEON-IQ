-- Track when a memory was last retrieved so the decay scoring formula can
-- penalise memories that haven't been recalled recently.
ALTER TABLE memories ADD COLUMN IF NOT EXISTS last_accessed_at TIMESTAMPTZ;

-- Partial index used by the decay-weighted retrieval query.
-- Covers non-tombstoned rows ordered by recency of access.
CREATE INDEX IF NOT EXISTS idx_memories_last_accessed
    ON memories (agent_id, last_accessed_at NULLS LAST)
    WHERE archived_at IS NULL;
