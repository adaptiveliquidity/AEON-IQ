-- Migration 0019: memory status, sensitivity, and validity columns.
-- Enables lifecycle management (quarantine, suppression), sensitivity
-- classification, and time-bounded memory validity.

ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS status             TEXT        NOT NULL DEFAULT 'active',
    ADD COLUMN IF NOT EXISTS sensitivity        TEXT        NOT NULL DEFAULT 'unknown',
    ADD COLUMN IF NOT EXISTS valid_from         TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS valid_to           TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS suppression_reason TEXT,
    ADD COLUMN IF NOT EXISTS status_updated_at  TIMESTAMPTZ DEFAULT NOW();

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_memory_status'
          AND conrelid = 'memories'::regclass
    ) THEN
        ALTER TABLE memories
            ADD CONSTRAINT chk_memory_status
            CHECK (status IN ('active', 'candidate', 'quarantined', 'suppressed'));
    END IF;
END $$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'chk_memory_sensitivity'
          AND conrelid = 'memories'::regclass
    ) THEN
        ALTER TABLE memories
            ADD CONSTRAINT chk_memory_sensitivity
            CHECK (sensitivity IN ('unknown', 'normal', 'private', 'sensitive', 'secret'));
    END IF;
END $$;

-- Partial index for the hot retrieval path: live, active memories only.
CREATE INDEX IF NOT EXISTS idx_memories_active
    ON memories(agent_id, created_at DESC)
    WHERE archived_at IS NULL AND status = 'active';
