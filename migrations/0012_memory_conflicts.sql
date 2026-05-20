-- memory_conflicts: flags contradictory fact pairs detected by the LLM.
-- Detection runs asynchronously after each L2 insert (when CONFLICT_DETECTION_ENABLED=true).
-- Conflicts are never auto-resolved; an operator must choose a resolution action.

CREATE TABLE IF NOT EXISTS memory_conflicts (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id    TEXT        NOT NULL,
    memory_a    UUID        REFERENCES memories(id) ON DELETE CASCADE,
    memory_b    UUID        REFERENCES memories(id) ON DELETE CASCADE,
    reason      TEXT        NOT NULL,
    resolved_at TIMESTAMPTZ,
    -- 'keep_a' | 'keep_b' | 'keep_both' | 'dismissed'
    resolution  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_conflicts_agent
    ON memory_conflicts(agent_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_conflicts_unresolved
    ON memory_conflicts(agent_id)
    WHERE resolved_at IS NULL;
