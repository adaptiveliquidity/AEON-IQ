-- Migration 0024: Timeline schema v2 branch and digest-chain metadata.
-- Keeps all new fields nullable so existing timeline rows remain valid.

ALTER TABLE cognitive_hypervisor_timeline
    ADD COLUMN IF NOT EXISTS prev_event_digest TEXT,
    ADD COLUMN IF NOT EXISTS branch_id TEXT;

CREATE INDEX IF NOT EXISTS idx_cht_agent_branch_time
    ON cognitive_hypervisor_timeline (agent_id, branch_id, occurred_at DESC)
    WHERE branch_id IS NOT NULL;
