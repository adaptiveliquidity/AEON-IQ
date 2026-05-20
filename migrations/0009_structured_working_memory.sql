-- Phase 4.3: structured working memory state
-- Adds a JSONB column alongside the existing text summary for backward compat.
-- Shape: { "summary": "...", "active_entities": [...], "current_goal": "...",
--          "open_questions": [...] }

ALTER TABLE working_memory ADD COLUMN IF NOT EXISTS state JSONB;
