-- Phase B: LTM Archival support
-- Adds per-memory access tracking and a tier column (L2 = recent episodic,
-- L3 = compacted archival).  The archival job promotes stale, never-retrieved
-- L2 memories into L3 summary nodes to keep the working set lean.

ALTER TABLE memories ADD COLUMN IF NOT EXISTS access_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE memories ADD COLUMN IF NOT EXISTS tier TEXT NOT NULL DEFAULT 'L2';

-- Partial index used by the archival job to quickly find stale L2 candidates.
CREATE INDEX IF NOT EXISTS idx_memories_archival_candidates
    ON memories(agent_id, created_at)
    WHERE tier = 'L2' AND access_count = 0;

-- Fast lookup for all L3 nodes per agent (used by retrieval fallback).
CREATE INDEX IF NOT EXISTS idx_memories_tier
    ON memories(agent_id, tier);
