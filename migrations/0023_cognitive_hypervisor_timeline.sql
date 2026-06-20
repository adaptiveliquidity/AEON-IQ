-- Migration 0023: Cognitive Hypervisor timeline ledger.
-- Append-only bridge from Nexus execution events to AEON-IQ memory state.

CREATE TABLE IF NOT EXISTS cognitive_hypervisor_timeline (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id          TEXT        NOT NULL,
    session_id        TEXT,
    nexus_snapshot_id UUID,
    capsule_digest    TEXT,
    event_type        TEXT        NOT NULL,
    occurred_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_cht_agent_time
    ON cognitive_hypervisor_timeline (agent_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_cht_snapshot
    ON cognitive_hypervisor_timeline (nexus_snapshot_id);
