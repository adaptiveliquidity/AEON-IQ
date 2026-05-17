-- Reversible archival versioning: track each L2→L3 compaction run as an
-- auditable batch so individual runs can be inspected and restored atomically.
--
-- archival_batches: one row per compaction cycle per agent
-- memories.archival_batch_id: links both the tombstoned L2 sources and the
--   newly-created L3 facts back to the batch that produced them.

CREATE TABLE IF NOT EXISTS archival_batches (
    id            UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id      TEXT        NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source_count  INT         NOT NULL,
    l3_count      INT         NOT NULL,
    status        TEXT        NOT NULL DEFAULT 'completed'
                              CHECK (status IN ('completed', 'restored'))
);

CREATE INDEX IF NOT EXISTS idx_archival_batches_agent
    ON archival_batches(agent_id, created_at DESC);

-- All memories (L2 sources + L3 products) reference the batch that created them.
-- NULL means the memory was created normally (not via a compaction run).
ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS archival_batch_id UUID REFERENCES archival_batches(id);

CREATE INDEX IF NOT EXISTS idx_memories_archival_batch
    ON memories(archival_batch_id)
    WHERE archival_batch_id IS NOT NULL;
