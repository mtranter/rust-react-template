-- Runs with search_path set to "widgets" (spec §6).
CREATE SCHEMA IF NOT EXISTS widgets;

CREATE TABLE widgets.widget (
    id         uuid        PRIMARY KEY,
    name       text        NOT NULL,
    created_at timestamptz NOT NULL,
    version    bigint      NOT NULL DEFAULT 1
);
