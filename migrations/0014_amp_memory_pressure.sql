-- Migration 0014: add AMP pressure-tracking columns to memories.
-- access_count and last_accessed_at already exist (migrations 0002 / 0004).
-- This adds the utility EMA, computed pressure, and soft-eviction state.

ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS utility_ema     DOUBLE PRECISION NOT NULL DEFAULT 0.5,
    ADD COLUMN IF NOT EXISTS pressure        DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    ADD COLUMN IF NOT EXISTS soft_evicted    BOOLEAN          NOT NULL DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS soft_evicted_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_memories_soft_evicted
    ON memories(soft_evicted) WHERE soft_evicted = FALSE;
