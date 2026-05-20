-- Phase 4.2: entity disambiguation via Levenshtein distance
-- Enables upsert_entity to merge near-duplicate names like "Alex" / "Alexander".

CREATE EXTENSION IF NOT EXISTS fuzzystrmatch;
