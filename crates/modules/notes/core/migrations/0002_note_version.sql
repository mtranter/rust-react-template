-- Optimistic-concurrency version for the note aggregate. Runs with search_path
-- set to "notes". Every write bumps it; updates match on it (see the db layer's
-- `save`), so a stale writer is refused rather than silently overwriting.
ALTER TABLE notes.note ADD COLUMN version bigint NOT NULL DEFAULT 1;
