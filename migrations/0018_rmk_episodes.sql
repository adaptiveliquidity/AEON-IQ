-- Migration 0018: RMK episode log.
-- Each row records the performance metrics and computed reward for one
-- conversational episode, linked to the policy that was active.

CREATE TABLE IF NOT EXISTS rmk_episodes (
    id                  UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id            TEXT             NOT NULL,
    policy_id           UUID             REFERENCES rmk_policies(id) ON DELETE SET NULL,
    task_success        DOUBLE PRECISION NOT NULL,
    token_savings       DOUBLE PRECISION NOT NULL,
    retrieval_precision DOUBLE PRECISION NOT NULL,
    eviction_cost       DOUBLE PRECISION NOT NULL,
    reward              DOUBLE PRECISION NOT NULL,
    created_at          TIMESTAMPTZ      NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_rmk_episodes_agent  ON rmk_episodes(agent_id);
CREATE INDEX IF NOT EXISTS idx_rmk_episodes_policy ON rmk_episodes(policy_id);
