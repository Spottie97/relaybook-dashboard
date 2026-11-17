-- Mirrors the v4 -> v5 branch in Relaybook migrate.rs at c25c29f.
-- Applied only when those columns are missing, which they are on a v4 ledger
-- created by MIGRATE_V3_TO_V4 (runs has no failure_streak yet; tasks has no run_id).

ALTER TABLE tasks ADD COLUMN run_id INTEGER;
ALTER TABLE runs ADD COLUMN failure_streak INTEGER NOT NULL DEFAULT 0;
ALTER TABLE runs ADD COLUMN current_slug TEXT;
ALTER TABLE runs ADD COLUMN complete_requested INTEGER NOT NULL DEFAULT 0;
