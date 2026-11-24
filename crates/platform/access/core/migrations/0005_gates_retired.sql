-- Runs with search_path set to "access" (ADR 0021).

-- Whether `migrate` has retired the Modules' gates (ADR 0047): a row once it
-- has. Retiring revokes every grant its role's gate didn't let through, so it
-- runs once: run again, with no gate grants left, it would revoke them all.
CREATE TABLE access.gates_retired (
    id         boolean     PRIMARY KEY DEFAULT true CHECK (id),
    retired_at timestamptz NOT NULL
);
