-- Migration 0017: RMK policy store.
-- Each row is a versioned policy vector θ for one agent.  The meta-learner
-- reads the latest row for an agent and writes new rows as it updates.

CREATE TABLE IF NOT EXISTS rmk_policies (
    id                  UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id            TEXT             NOT NULL,
    pressure_a          DOUBLE PRECISION NOT NULL,
    pressure_b          DOUBLE PRECISION NOT NULL,
    kp                  DOUBLE PRECISION NOT NULL,
    ki                  DOUBLE PRECISION NOT NULL,
    graph_bonus_weight  DOUBLE PRECISION NOT NULL,
    retrieval_threshold DOUBLE PRECISION NOT NULL,
    created_at          TIMESTAMPTZ      NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_rmk_policies_agent ON rmk_policies(agent_id);
