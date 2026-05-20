-- Phase 4.1: memory_entity_links join table
-- Maps memory rows to the entities extracted in the same turn.
-- Enables graph-walk retrieval: given a query mentioning entity E,
-- retrieve memories linked to entities related to E in memory_graph.

CREATE TABLE IF NOT EXISTS memory_entity_links (
    memory_id  UUID        NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
    entity_id  UUID        NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    agent_id   TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (memory_id, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_mel_entity ON memory_entity_links(entity_id);
CREATE INDEX IF NOT EXISTS idx_mel_agent  ON memory_entity_links(agent_id);
