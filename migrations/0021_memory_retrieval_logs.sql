-- Migration 0021: retrieval audit log.
-- Records every retrieve_relevant() call with candidate/injected memory IDs,
-- per-memory scores, and latency.  Used for debugging retrieval quality and
-- auditing what the model "saw" at each turn.

CREATE TABLE IF NOT EXISTS memory_retrieval_logs (
    id                    UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id              TEXT         NOT NULL,
    session_id            TEXT,
    query_text            TEXT,
    query_hash            TEXT         NOT NULL,
    candidate_memory_ids  UUID[]       NOT NULL DEFAULT '{}',
    injected_memory_ids   UUID[]       NOT NULL DEFAULT '{}',
    suppressed_memory_ids UUID[]       NOT NULL DEFAULT '{}',
    scores                JSONB        NOT NULL DEFAULT '{}',
    latency_ms            INTEGER,
    created_at            TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_retrieval_logs_agent_id
    ON memory_retrieval_logs(agent_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_retrieval_logs_session_id
    ON memory_retrieval_logs(session_id)
    WHERE session_id IS NOT NULL;
