-- Add updated_at to memories so PATCH edits are trackable.
ALTER TABLE memories
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Backfill existing rows so updated_at reflects when the memory was created.
UPDATE memories SET updated_at = created_at WHERE updated_at = NOW();
