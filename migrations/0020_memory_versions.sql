-- Migration 0020: memory versioning — full snapshots on every write.
-- Stores the complete state of a memory at each change point so the full
-- edit history is queryable and diffs can be reconstructed client-side.

CREATE TABLE IF NOT EXISTS memory_versions (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    memory_id        UUID        NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    agent_id         TEXT        NOT NULL,
    version_number   INTEGER     NOT NULL CHECK (version_number > 0),
    content          TEXT        NOT NULL,
    memory_type      TEXT        NOT NULL,
    confidence       REAL        NOT NULL,
    provenance       TEXT        NOT NULL DEFAULT 'unknown',
    importance_score REAL        NOT NULL DEFAULT 0.5,
    importance_source TEXT       NOT NULL DEFAULT 'extractor',
    status           TEXT        NOT NULL DEFAULT 'active',
    sensitivity      TEXT        NOT NULL DEFAULT 'unknown',
    valid_from       TIMESTAMPTZ,
    valid_to         TIMESTAMPTZ,
    source_turn      INTEGER,
    change_type      TEXT        NOT NULL,
    change_reason    TEXT,
    changed_by       TEXT        NOT NULL DEFAULT 'system',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (memory_id, version_number)
);

CREATE INDEX IF NOT EXISTS idx_memory_versions_memory_id
    ON memory_versions(memory_id, version_number DESC);

CREATE INDEX IF NOT EXISTS idx_memory_versions_agent_id
    ON memory_versions(agent_id, created_at DESC);

-- Backfill version 1 for all existing memories.
INSERT INTO memory_versions
    (memory_id, agent_id, version_number, content, memory_type, confidence,
     provenance, importance_score, importance_source, status, sensitivity,
     valid_from, valid_to, source_turn, change_type, change_reason, changed_by,
     created_at)
SELECT
    id,
    agent_id,
    1,
    content,
    memory_type,
    confidence,
    provenance,
    importance_score,
    importance_source,
    COALESCE(NULLIF(status, ''), 'active'),
    COALESCE(NULLIF(sensitivity, ''), 'unknown'),
    valid_from,
    valid_to,
    source_turn,
    'initial',
    'backfill from migration 0020',
    'system',
    created_at
FROM memories
ON CONFLICT (memory_id, version_number) DO NOTHING;
