-- Set when the note's author is deleted: the notes module's reaction to the
-- users module's `UserDeleted` event. Runs with search_path set to "notes".
ALTER TABLE notes.note ADD COLUMN archived_at timestamptz;
