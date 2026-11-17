-- Published view (spec D8): the read contract other modules' SQL may use, so
-- `notes.note` stays private and free to change. Evolve it only additively
-- (`CREATE OR REPLACE VIEW` may append columns); a breaking change ships as
-- `note_v2` alongside this one until every reader has moved.
CREATE SCHEMA IF NOT EXISTS notes_api;

CREATE VIEW notes_api.note_v1 AS
SELECT id, author_id, title, created_at
FROM notes.note;
