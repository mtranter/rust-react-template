-- Notes module schema. Runs with search_path set to "notes".
-- author_id intentionally has NO foreign key to the users module — modules are
-- isolated and independently liftable (spec §D1).
CREATE SCHEMA IF NOT EXISTS notes;

CREATE TABLE notes.note (
    id         uuid        PRIMARY KEY,
    author_id  uuid        NOT NULL,
    title      text        NOT NULL,
    body       text        NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX note_author_idx ON notes.note (author_id);
