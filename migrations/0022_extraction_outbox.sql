-- Migration 0022: durable extraction outbox.
-- Stores post-response memory extraction work so crashes/restarts do not lose
-- background fact extraction before it can be retried by the worker.

CREATE TABLE IF NOT EXISTS extraction_jobs (
    id              UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id        TEXT         NOT NULL,
    session_id      TEXT,
    payload         JSONB        NOT NULL,
    status          TEXT         NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'in_progress', 'done', 'failed')),
    attempts        INT          NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    last_error      TEXT,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_extraction_jobs_pending_due
    ON extraction_jobs(next_attempt_at)
    WHERE status = 'pending';
