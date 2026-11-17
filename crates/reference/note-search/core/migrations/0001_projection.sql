-- note-search's own copy of notes and their authors, kept from the notes'
-- and users' topics from the earliest (ADR 0036).
-- Rows are written only by `app_db::Projection`, which keeps their versions.
CREATE TABLE note_search.note (
    id         uuid        PRIMARY KEY,
    author_id  uuid        NOT NULL,
    title      text        NOT NULL,
    created_at timestamptz NOT NULL
);

CREATE INDEX note_created_at_idx ON note_search.note (created_at DESC, id);
CREATE INDEX note_author_id_idx ON note_search.note (author_id);

CREATE TABLE note_search.author (
    id           uuid PRIMARY KEY,
    display_name text NOT NULL
);
