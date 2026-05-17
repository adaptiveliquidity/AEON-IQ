-- Importance scoring: additive, backward-compatible.
-- Existing rows default to score=0.5 (neutral) and source='extractor'.
-- With IMPORTANCE_BOOST_FACTOR=0.0 (default) these columns have no effect.

ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS importance_score REAL NOT NULL DEFAULT 0.5
        CHECK (importance_score BETWEEN 0.0 AND 1.0);

ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS importance_source TEXT NOT NULL DEFAULT 'extractor'
        CHECK (importance_source IN ('user_stated', 'agent_marked', 'extractor'));

CREATE INDEX IF NOT EXISTS idx_memories_importance
    ON memories (importance_score DESC)
    WHERE archived_at IS NULL;
