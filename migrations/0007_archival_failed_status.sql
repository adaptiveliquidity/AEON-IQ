-- Add 'failed' as a valid archival batch status so failed compaction runs
-- are distinguishable from successful ones with zero L3 facts.
ALTER TABLE archival_batches DROP CONSTRAINT IF EXISTS archival_batches_status_check;
ALTER TABLE archival_batches ADD CONSTRAINT archival_batches_status_check
    CHECK (status IN ('completed', 'restored', 'failed'));
