-- Runs with search_path set to "appearance".
CREATE SCHEMA IF NOT EXISTS appearance;

-- The org's one Theme (ADR 0051): at most one row, `org`, absent until first saved. Each map
-- is CSS-variable overrides by name without the `--`.
CREATE TABLE appearance.theme (
    key        text        PRIMARY KEY CHECK (key = 'org'),
    light      jsonb       NOT NULL,
    dark       jsonb       NOT NULL,
    shared     jsonb       NOT NULL,
    updated_at timestamptz NOT NULL,
    updated_by text        NOT NULL,
    version    bigint      NOT NULL
);
