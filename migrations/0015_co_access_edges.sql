-- Migration 0015: co-access graph edges for AMP retrieval augmentation.
-- Edges are undirected (order_agnostic constraint), accumulate weight on
-- co-occurrence, and are decayed periodically by the AMP background job.

CREATE TABLE IF NOT EXISTS co_access_edges (
    memory_a         UUID             NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    memory_b         UUID             NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    weight           DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    last_co_occurred TIMESTAMPTZ      NOT NULL DEFAULT NOW(),
    PRIMARY KEY (memory_a, memory_b),
    CONSTRAINT co_access_order_agnostic CHECK (memory_a < memory_b)
);

CREATE INDEX IF NOT EXISTS idx_co_access_memory_a ON co_access_edges(memory_a);
CREATE INDEX IF NOT EXISTS idx_co_access_memory_b ON co_access_edges(memory_b);
