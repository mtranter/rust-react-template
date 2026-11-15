-- This module's copy of each root of the topic it projects, by key (ADR 0036).
-- Rows are written only through `app_db::Projection`, which keeps their
-- versions. `entity` holds the event's whole entity: replace it with the
-- typed columns this module lists, searches or sorts by.
CREATE TABLE widget_index.entry (
    key        text        PRIMARY KEY,
    entity     jsonb       NOT NULL,
    updated_at timestamptz NOT NULL
);

CREATE INDEX entry_updated_at_idx ON widget_index.entry (updated_at DESC, key);
