-- Users module schema. Runs with search_path set to "users" (the runner also
-- creates the schema), so unqualified objects land here; tables are qualified
-- for clarity and to match the schema-qualified queries in db.rs.
CREATE SCHEMA IF NOT EXISTS users;

CREATE TABLE users.account (
    id           uuid        PRIMARY KEY,
    email        text        NOT NULL UNIQUE,
    display_name text        NOT NULL,
    created_at   timestamptz NOT NULL DEFAULT now()
);
