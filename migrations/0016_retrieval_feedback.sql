-- Migration 0016: explicit retrieval-quality feedback.
-- Stores user-provided (or inferred) ratings for individual retrieved
-- memories.  Used by the AMP utility tracker and the RMK reward model.

CREATE TABLE IF NOT EXISTS retrieval_feedback (
    id         UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id   TEXT             NOT NULL,
    memory_id  UUID             REFERENCES memories(id) ON DELETE SET NULL,
    feedback   DOUBLE PRECISION NOT NULL CHECK (feedback BETWEEN 0 AND 1),
    created_at TIMESTAMPTZ      NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_retrieval_feedback_agent  ON retrieval_feedback(agent_id);
CREATE INDEX IF NOT EXISTS idx_retrieval_feedback_memory ON retrieval_feedback(memory_id);
