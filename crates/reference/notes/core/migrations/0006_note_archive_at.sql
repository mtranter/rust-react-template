-- When the note is due to archive itself, if ever: a deferred job keyed by
-- the note runs then (ADR 0043). Runs with search_path set to "notes".
ALTER TABLE notes.note ADD COLUMN archive_at timestamptz;
