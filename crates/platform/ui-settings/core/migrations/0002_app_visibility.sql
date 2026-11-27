-- Runs with search_path set to "ui_settings".

-- Which apps the org hides from its SPA: at most one row, `org`, absent until first saved.
-- Hiding is a convenience: the apps' operations still answer.
CREATE TABLE ui_settings.app_visibility (
    key        text        PRIMARY KEY CHECK (key = 'org'),
    hidden     text[]      NOT NULL,
    updated_at timestamptz NOT NULL,
    updated_by text        NOT NULL,
    version    bigint      NOT NULL
);
